//! Simulation : la logique de jeu, séparée de l'affichage.
//!
//! La simulation avance par pas fixes (`FixedUpdate`, 32 fois par seconde),
//! quelle que soit la vitesse d'affichage. C'est ce qui la rend reproductible,
//! condition du futur multijoueur en lockstep. L'affichage, lui, tourne à la
//! fréquence de l'écran et interpole entre les deux derniers pas pour que le
//! mouvement reste fluide.

use bevy::prelude::*;

use crate::collision::{self, Agent, CollisionLayer, CollisionSettings, Collider, MapBounds, StaticObstacle, Steering};
use crate::navigation::{self, MoveGroup, MoveGroupCounter, NavGrid, NavPath, NavSettings, PathQueue, PathScratch};

/// Nombre de pas de simulation par seconde.
pub const TICKS_PER_SECOND: f64 = 32.0;

/// Position d'une unité sur le sol (x, z), selon la simulation.
#[derive(Component, Debug, Clone, Copy)]
pub struct SimPosition {
    pub current: Vec2,
    /// Position au pas précédent, utilisée pour interpoler l'affichage.
    pub previous: Vec2,
}

impl SimPosition {
    pub fn new(pos: Vec2) -> Self {
        Self { current: pos, previous: pos }
    }
}

/// Vitesse de déplacement en mètres par seconde.
#[derive(Component, Debug, Clone, Copy)]
pub struct MoveSpeed(pub f32);

/// Destination courante, si l'unité a reçu un ordre de déplacement.
/// Le composant est retiré une fois l'unité arrivée.
#[derive(Component, Debug, Clone, Copy)]
pub struct MoveTarget(pub Vec2);

/// Les étapes d'un pas de simulation, dans l'ordre. Le jeu et les autres
/// modules du moteur y rangent leurs systèmes de `FixedUpdate`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimSet {
    /// Début du pas : les ordres donnés depuis le pas précédent sont figés
    /// (voir le module [`crate::orders`]).
    CollectOrders,
    /// Application des ordres des joueurs (déplacements, pose de bâtiments…)
    /// et décisions des unités (aller au chantier…).
    Orders,
    /// Grille de navigation et calcul des chemins.
    Navigation,
    /// Déplacements et collisions.
    Movement,
    /// Ce qui dépend des positions finales du pas (construction…).
    Construction,
}

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(TICKS_PER_SECOND))
            .init_resource::<MapBounds>()
            .init_resource::<CollisionSettings>()
            .init_resource::<NavSettings>()
            .init_resource::<PathQueue>()
            .init_resource::<PathScratch>()
            .init_resource::<MoveGroupCounter>()
            .configure_sets(
                FixedUpdate,
                (SimSet::CollectOrders, SimSet::Orders, SimSet::Navigation, SimSet::Movement, SimSet::Construction).chain(),
            )
            .add_systems(
                FixedUpdate,
                (
                    (navigation::rebuild_grid, navigation::request_paths, navigation::compute_paths)
                        .chain()
                        .in_set(SimSet::Navigation),
                    move_units.in_set(SimSet::Movement),
                ),
            )
            .add_systems(PostUpdate, sync_transforms.before(TransformSystems::Propagate));
    }
}

/// Un pas de simulation : suivi des chemins, déplacement des unités et
/// collisions (voir les modules [`crate::navigation`] et [`crate::collision`]).
fn move_units(
    mut commands: Commands,
    time: Res<Time>,
    bounds: Res<MapBounds>,
    settings: Res<CollisionSettings>,
    grid: Option<Res<NavGrid>>,
    obstacles: Query<(Entity, &StaticObstacle)>,
    // `Ref<T>` : comme `&T`, mais permet de savoir si le composant a changé
    // depuis le dernier passage de ce système (ici : nouvel ordre reçu).
    mut units: Query<(
        Entity,
        &mut SimPosition,
        &MoveSpeed,
        Option<Ref<MoveTarget>>,
        Option<&Collider>,
        Option<&mut Steering>,
        Option<&mut NavPath>,
    )>,
) {
    // Dans FixedUpdate, `Time` donne la durée fixe du pas, pas celle de l'image.
    let step = time.delta_secs();

    // Les obstacles fixes, triés par identifiant (ordre reproductible).
    let mut obstacle_list: Vec<(Entity, Rect)> = obstacles.iter().map(|(e, o)| (e, o.0)).collect();
    obstacle_list.sort_unstable_by_key(|(entity, _)| *entity);
    let obstacle_rects: Vec<Rect> = obstacle_list.into_iter().map(|(_, rect)| rect).collect();

    // 1. On copie l'état des unités dans une liste simple, en faisant
    // avancer au passage le suivi de chemin des unités au sol.
    let mut agents: Vec<(Entity, Agent)> = units
        .iter_mut()
        .map(|(entity, pos, speed, target, collider, steering, path)| {
            let mut steering = steering.as_deref().copied().unwrap_or_default();
            if target.as_ref().is_some_and(|t| t.is_changed()) {
                steering.start_order();
            }
            // Une unité sans `Collider` ne pousse personne, n'est pas poussée
            // et va en ligne droite.
            let radius = collider.map_or(0.0, |c| c.radius);
            let layer = collider.map_or(CollisionLayer::Air, |c| c.layer);
            let (mut waypoint, mut remaining, mut hold) = (None, 0.0, false);
            if let (Some(target), CollisionLayer::Ground, Some(grid)) = (&target, layer, &grid) {
                match path {
                    Some(mut path) if path.matches(target.0) => {
                        let reach = radius.max(speed.0 * step);
                        (waypoint, remaining) = path.advance(pos.current, radius, reach, grid);
                    }
                    // Pas encore de chemin pour cet ordre : on attend sur place.
                    _ => hold = true,
                }
            }
            let agent = Agent {
                position: pos.current,
                radius,
                mass: collider.map_or(1.0, |c| c.mass),
                speed: speed.0,
                target: target.map(|t| t.0),
                waypoint,
                remaining,
                hold,
                layer,
                steering,
            };
            (entity, agent)
        })
        .collect();

    // 2. L'ordre de parcours d'une requête Bevy dépend du rangement interne
    // des entités : on trie par identifiant pour que chaque pas soit
    // reproductible. `sort_unstable_by_key` trie selon une clé ; « unstable »
    // ne garde pas l'ordre des égaux, sans importance ici car les clés sont uniques.
    // (Pour le lockstep, il faudra des identifiants de simulation garantis
    // identiques sur toutes les machines plutôt que les `Entity` de Bevy.)
    agents.sort_unstable_by_key(|(entity, _)| *entity);

    // 3. Le calcul lui-même, dans une fonction pure testable sans fenêtre.
    let mut simulated: Vec<Agent> = agents.iter().map(|(_, agent)| *agent).collect();
    collision::step(&mut simulated, step, bounds.0, &obstacle_rects, &settings);

    // 4. On recopie le résultat dans les composants.
    for ((entity, before), after) in agents.iter().zip(&simulated) {
        let Ok((_, mut pos, _, _, _, steering, _)) = units.get_mut(*entity) else { continue };
        pos.previous = pos.current;
        pos.current = after.position;
        if let Some(mut steering) = steering {
            *steering = after.steering;
        }
        if before.target.is_some() && after.target.is_none() {
            commands.entity(*entity).remove::<(MoveTarget, NavPath, MoveGroup)>();
        }
    }
}

/// Affichage : place chaque unité entre ses deux dernières positions simulées
/// et l'oriente dans sa direction de marche.
fn sync_transforms(
    fixed: Res<Time<Fixed>>,
    // `Has<T>` : vrai si l'entité possède le composant, sans le lire.
    mut units: Query<(&SimPosition, &mut Transform, Has<MoveTarget>)>,
) {
    let alpha = fixed.overstep_fraction();
    for (pos, mut transform, is_moving) in &mut units {
        let p = pos.previous.lerp(pos.current, alpha);
        transform.translation.x = p.x;
        transform.translation.z = p.y;

        // Une unité immobile bousculée par une autre recule sans se retourner.
        let dir = pos.current - pos.previous;
        if is_moving && dir.length_squared() > 1e-6 {
            // Le « devant » d'une entité Bevy est -Z.
            transform.rotation = Quat::from_rotation_y(f32::atan2(-dir.x, -dir.y));
        }
    }
}
