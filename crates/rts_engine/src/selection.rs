//! Sélection des unités et ordres contextuels.
//!
//! Consomme les actions `SelectAt`, `SelectBox` et `CommandAt` produites par
//! la couche d'entrées, quel que soit le périphérique d'origine. Les ordres
//! ne sont pas appliqués ici : ils partent dans la file des ordres
//! ([`CommandQueue`]), appliquée au prochain pas de simulation.

use bevy::prelude::*;

use crate::camera::{ray_at_height, screen_ray, screen_to_ground};
use crate::collision::StaticObstacle;
use crate::combat::{Attacker, Combatant, OrderMarker};
use crate::economy::production::ProductionQueue;
use crate::economy::{EconomyRules, ResourceNode};
use crate::input::{GameAction, SelectionDrag, TargetedOrder};
use crate::orders::{CommandQueue, PlayerCommand};
use crate::players::{are_enemies, LocalPlayer, Owner};
use crate::simulation::MoveSpeed;
use crate::units::{Unit, UnitCatalog};
use crate::RtsSystems;

/// Une entité qu'on peut sélectionner.
#[derive(Component, Debug, Clone, Copy)]
pub struct Selectable {
    /// Rayon de clic autour du centre de l'unité, en mètres.
    pub radius: f32,
    /// Pour un bâtiment : demi-largeur et demi-profondeur (x, z) du rectangle
    /// cliquable, qui remplace alors le cercle.
    pub half_extents: Option<Vec2>,
}

impl Selectable {
    /// Le point (x, z) est-il dans la zone cliquable autour de `center` ?
    fn contains(&self, center: Vec2, point: Vec2) -> bool {
        match self.half_extents {
            Some(half) => (point - center).abs().cmple(half).all(),
            None => point.distance(center) <= self.radius,
        }
    }
}

/// Marqueur : l'unité fait partie de la sélection actuelle.
#[derive(Component, Debug, Default)]
pub struct Selected;

/// Marqueur : l'anneau affiché au sol sous une unité sélectionnée.
#[derive(Component, Debug, Default)]
pub struct SelectionRing;

/// Marqueur : le rectangle de sélection affiché à l'écran.
#[derive(Component)]
struct SelectionBoxUi;

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_selection_box_ui).add_systems(
            Update,
            (
                handle_actions,
                show_selection_rings,
                update_selection_box_ui,
            )
                .chain()
                .in_set(RtsSystems::Actions),
        );
    }
}

/// L'entité sélectionnable sous le rayon `ray`, la plus proche de la caméra.
///
/// Pour chaque entité, on regarde où le rayon traverse le plan horizontal
/// passant par son centre : si c'est dans sa zone cliquable, elle est touchée.
fn pick(ray: Ray3d, units: &Query<(Entity, &GlobalTransform, &Selectable)>) -> Option<Entity> {
    units
        .iter()
        .filter_map(|(entity, transform, selectable)| {
            let center = transform.translation();
            let point = ray_at_height(ray, center.y)?;
            selectable.contains(center.xz(), point.xz()).then(|| (entity, point.distance(ray.origin)))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(entity, _)| entity)
}

/// Le gisement visé par un clic : celui sous le point du sol `ground`
/// (un peu agrandi, les cristaux dépassent du sol), ou la source couverte par
/// le bâtiment `picked` (un Extracteur).
fn pick_node(ground: Vec2, picked: Option<Entity>, nodes: &Query<(Entity, &ResourceNode, &StaticObstacle)>) -> Option<Entity> {
    let mut list: Vec<(Entity, &ResourceNode, &StaticObstacle)> = nodes.iter().collect();
    list.sort_unstable_by_key(|n| n.0);
    list.iter()
        .find(|(_, node, _)| picked.is_some() && node.extractor == picked)
        .or_else(|| list.iter().find(|(.., obstacle)| obstacle.0.inflate(0.5).contains(ground)))
        .map(|(entity, ..)| *entity)
}

fn handle_actions(
    mut commands: Commands,
    mut actions: MessageReader<GameAction>,
    camera: Single<(&Camera, &GlobalTransform)>,
    units: Query<(Entity, &GlobalTransform, &Selectable)>,
    selected: Query<Entity, With<Selected>>,
    // Seules les entités qui peuvent bouger (pas les bâtiments) reçoivent les ordres de déplacement.
    mobile: Query<(), With<MoveSpeed>>,
    owners: Query<&Owner>,
    unit_types: Query<&Unit>,
    nodes: Query<(Entity, &ResourceNode, &StaticObstacle)>,
    queues: Query<(), With<ProductionQueue>>,
    rules: Option<Res<EconomyRules>>,
    local: Res<LocalPlayer>,
    mut orders: ResMut<CommandQueue>,
    catalog: Option<Res<UnitCatalog>>,
    combat: (Query<(), With<Attacker>>, Query<(), With<Combatant>>),
    mut markers: MessageWriter<OrderMarker>,
) {
    let (armed, attackable) = combat;
    // `Single` garantit qu'il y a exactement une caméra ; `into_inner` donne ses composants.
    let (camera, camera_transform) = camera.into_inner();

    for action in actions.read() {
        match *action {
            GameAction::SelectAt { screen_pos, additive } => {
                let Some(ray) = screen_ray(camera, camera_transform, screen_pos) else { continue };
                // Pour chaque unité, on regarde où le rayon traverse le plan horizontal
                // passant par son centre : si c'est dans son rayon, elle est touchée.
                // On garde la plus proche de la caméra.
                let hit = pick(ray, &units);
                // Une entité adverse (ou sans propriétaire) se sélectionne
                // seule, pour voir ses infos : on ne peut pas lui donner d'ordre.
                let is_mine = |e: Entity| owners.get(e).is_ok_and(|o| o.0 == local.0);
                let foreign_hit = hit.is_some_and(|e| !is_mine(e));

                if !additive || foreign_hit {
                    clear_selection(&mut commands, &selected);
                } else {
                    // Ajout à une sélection qui montrait une entité adverse : on la retire.
                    for entity in selected.iter().filter(|&e| !is_mine(e)) {
                        commands.entity(entity).remove::<Selected>();
                    }
                }
                if let Some(entity) = hit {
                    if additive && !foreign_hit && selected.contains(entity) {
                        commands.entity(entity).remove::<Selected>();
                    } else {
                        commands.entity(entity).insert(Selected);
                    }
                }
            }

            GameAction::SelectBox { corner_a, corner_b, additive } => {
                // Test en espace écran : une unité est prise si son centre, projeté
                // à l'écran, tombe dans le rectangle. C'est ce que voit le joueur.
                let rect = Rect::from_corners(corner_a, corner_b);
                // Le rectangle ne prend que les entités du joueur local.
                let is_mine = |e: Entity| owners.get(e).is_ok_and(|o| o.0 == local.0);
                if !additive {
                    clear_selection(&mut commands, &selected);
                } else {
                    for entity in selected.iter().filter(|&e| !is_mine(e)) {
                        commands.entity(entity).remove::<Selected>();
                    }
                }
                let inside: Vec<Entity> = units
                    .iter()
                    .filter(|(entity, ..)| is_mine(*entity))
                    .filter(|(_, transform, _)| {
                        camera
                            .world_to_viewport(camera_transform, transform.translation())
                            .is_ok_and(|pos| rect.contains(pos))
                    })
                    .map(|(entity, ..)| entity)
                    .collect();
                // Comme dans tout RTS : si le rectangle prend des unités, on
                // laisse de côté les bâtiments pris avec elles.
                let any_mobile = inside.iter().any(|e| mobile.contains(*e));
                for entity in inside {
                    if !any_mobile || mobile.contains(entity) {
                        commands.entity(entity).insert(Selected);
                    }
                }
            }

            GameAction::CommandAt { screen_pos } => {
                let Some(target) = screen_to_ground(camera, camera_transform, screen_pos) else { continue };
                // Ordre contextuel : sur un gisement (ou un Extracteur), les
                // ouvriers récoltent et les autres unités s'y rendent.
                let picked = screen_ray(camera, camera_transform, screen_pos).and_then(|ray| pick(ray, &units));
                let node = pick_node(target.xz(), picked, &nodes);
                // Les unités sélectionnées qui peuvent bouger et que l'on commande.
                let mut movers: Vec<Entity> = selected
                    .iter()
                    .filter(|&e| mobile.contains(e) && owners.get(e).is_ok_and(|o| o.0 == local.0))
                    .collect();
                // Ordre fixe (la sélection est parcourue dans un ordre quelconque).
                movers.sort_unstable();
                let is_harvester = |e: &Entity| {
                    let unit = unit_types.get(*e).ok();
                    rules.as_ref().zip(unit).is_some_and(|(r, u)| r.harvesters.contains(&u.type_index))
                };
                // Sur un ennemi : on l'attaque (les unités sans arme s'y rendent).
                let enemy = picked.filter(|&e| attackable.contains(e) && owners.get(e).is_ok_and(|o| are_enemies(local.0, o.0)));
                if let Some(enemy) = enemy {
                    let (fighters, others): (Vec<Entity>, Vec<Entity>) = movers.into_iter().partition(|e| armed.contains(*e));
                    if !fighters.is_empty() {
                        orders.push(local.0, PlayerCommand::Attack { units: fighters, target: enemy });
                    }
                    movers = others;
                }
                if let Some(node) = node {
                    // `partition` sépare une liste en deux selon un critère.
                    let (harvesters, others): (Vec<Entity>, Vec<Entity>) = movers.into_iter().partition(is_harvester);
                    if !harvesters.is_empty() {
                        orders.push(local.0, PlayerCommand::Harvest { units: harvesters, node });
                    }
                    movers = others;
                }
                // Sur le sol : **attaque-déplacement par défaut** pour les unités
                // armées (voir GAME_DESIGN.md) ; les ouvriers et les unités sans
                // arme se déplacent simplement.
                let (fighters, walkers): (Vec<Entity>, Vec<Entity>) =
                    movers.into_iter().partition(|e| armed.contains(*e) && !is_harvester(e));
                if !fighters.is_empty() {
                    orders.push(local.0, PlayerCommand::AttackMove { units: fighters, target: target.xz() });
                    markers.write(OrderMarker { point: target.xz(), attack: true });
                }
                if !walkers.is_empty() {
                    orders.push(local.0, PlayerCommand::Move { units: walkers, target: target.xz() });
                    markers.write(OrderMarker { point: target.xz(), attack: false });
                }
                // Les bâtiments producteurs sélectionnés y mettent leur point de ralliement.
                let mut producers: Vec<Entity> = selected
                    .iter()
                    .filter(|&e| queues.contains(e) && owners.get(e).is_ok_and(|o| o.0 == local.0))
                    .collect();
                producers.sort_unstable();
                if !producers.is_empty() {
                    let point = node.and_then(|n| nodes.get(n).ok()).map_or(target.xz(), |(_, _, o)| o.0.center());
                    orders.push(local.0, PlayerCommand::SetRally { buildings: producers, point, node });
                }
            }

            GameAction::OrderAt { order, screen_pos, .. } => {
                let Some(target) = screen_to_ground(camera, camera_transform, screen_pos) else { continue };
                let mut movers: Vec<Entity> = selected
                    .iter()
                    .filter(|&e| mobile.contains(e) && owners.get(e).is_ok_and(|o| o.0 == local.0))
                    .collect();
                movers.sort_unstable();
                if movers.is_empty() {
                    continue;
                }
                let picked = screen_ray(camera, camera_transform, screen_pos).and_then(|ray| pick(ray, &units));
                let enemy = picked.filter(|&e| attackable.contains(e) && owners.get(e).is_ok_and(|o| are_enemies(local.0, o.0)));
                match (order, enemy) {
                    // A puis clic sur un ennemi : l'attaquer.
                    (TargetedOrder::AttackMove, Some(enemy)) => {
                        orders.push(local.0, PlayerCommand::Attack { units: movers, target: enemy });
                    }
                    // A puis clic au sol : attaque-déplacement, ouvriers compris.
                    (TargetedOrder::AttackMove, None) => {
                        orders.push(local.0, PlayerCommand::AttackMove { units: movers, target: target.xz() });
                        markers.write(OrderMarker { point: target.xz(), attack: true });
                    }
                    // M puis clic : déplacement simple, qui ignore les ennemis.
                    (TargetedOrder::Move, _) => {
                        orders.push(local.0, PlayerCommand::Move { units: movers, target: target.xz() });
                        markers.write(OrderMarker { point: target.xz(), attack: false });
                    }
                }
            }

            GameAction::Stop | GameAction::HoldPosition => {
                let mut list: Vec<Entity> = selected
                    .iter()
                    .filter(|&e| mobile.contains(e) && owners.get(e).is_ok_and(|o| o.0 == local.0))
                    .collect();
                list.sort_unstable();
                if !list.is_empty() {
                    orders.push(
                        local.0,
                        if *action == GameAction::Stop { PlayerCommand::Stop { units: list } } else { PlayerCommand::HoldPosition { units: list } },
                    );
                }
            }

            GameAction::SpawnStressTest => {
                if let Some(catalog) = &catalog {
                    spawn_stress_test(&mut commands, catalog);
                }
            }
            // Actions de construction et de production : voir les modules
            // `buildings` et `economy`.
            GameAction::ChooseBuilding { .. }
            | GameAction::PlaceBuildingAt { .. }
            | GameAction::CancelPlacement
            | GameAction::Train { .. }
            | GameAction::CancelQueued { .. }
            | GameAction::CancelConstruction
            | GameAction::ChooseOrder { .. }
            | GameAction::CancelOrder => {}
        }
    }
}

fn clear_selection(commands: &mut Commands, selected: &Query<Entity, With<Selected>>) {
    for entity in selected {
        commands.entity(entity).remove::<Selected>();
    }
}

/// Place les unités en grille autour du point visé (une place par unité, de
/// rayon `radii[i]`) pour qu'elles ne visent pas toutes le même point. Les
/// collisions (module `collision`) règlent ensuite les bousculades en route.
/// `pub(crate)` : visible dans tout le moteur (pour les tests), pas au-dehors.
pub(crate) fn formation(radii: &[f32], center: Vec2) -> Vec<Vec2> {
    let count = radii.len();
    if count == 0 {
        return Vec::new();
    }
    let spacing = radii.iter().copied().fold(0.0, f32::max) * 2.0 + 0.3;
    let columns = (count as f32).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let origin = center - Vec2::new(columns as f32 - 1.0, rows as f32 - 1.0) * spacing / 2.0;
    (0..count)
        .map(|i| origin + Vec2::new((i % columns) as f32, (i / columns) as f32) * spacing)
        .collect()
}

/// Outil de test de charge : 300 unités en carré au centre de la carte.
fn spawn_stress_test(commands: &mut Commands, catalog: &UnitCatalog) {
    const COUNT: usize = 300;
    let side = (COUNT as f32).sqrt().ceil() as usize;
    for i in 0..COUNT {
        let pos = (Vec2::new((i % side) as f32, (i / side) as f32) - side as f32 / 2.0) * 2.0;
        catalog.spawn(commands, i % catalog.types.len(), pos);
    }
    info!("Test de charge : {COUNT} unités ajoutées");
}

/// Affiche l'anneau des unités sélectionnées, cache celui des autres.
/// L'anneau d'une entité adverse est rouge.
fn show_selection_rings(
    catalog: Option<Res<UnitCatalog>>,
    local: Res<LocalPlayer>,
    units: Query<(Has<Selected>, Option<&Owner>, &Children), With<Selectable>>,
    mut rings: Query<(&mut Visibility, &mut MeshMaterial3d<StandardMaterial>), With<SelectionRing>>,
) {
    for (is_selected, owner, children) in &units {
        let wanted = if is_selected { Visibility::Inherited } else { Visibility::Hidden };
        let foreign = owner.is_none_or(|o| o.0 != local.0);
        for child in children.iter() {
            if let Ok((mut visibility, mut material)) = rings.get_mut(child) {
                // `set_if_neq` n'écrit que si la valeur change (évite du travail inutile à Bevy).
                visibility.set_if_neq(wanted);
                if is_selected
                    && foreign
                    && let Some(catalog) = &catalog
                {
                    let red = catalog.enemy_ring_material();
                    if material.0 != red {
                        material.0 = red;
                    }
                }
            }
        }
    }
}

fn spawn_selection_box_ui(mut commands: Commands) {
    commands.spawn((
        SelectionBoxUi,
        Node {
            position_type: PositionType::Absolute,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(Color::srgb(0.2, 1.0, 0.35)),
        BackgroundColor(Color::srgba(0.2, 1.0, 0.35, 0.12)),
        Visibility::Hidden,
    ));
}

fn update_selection_box_ui(
    drag: Res<SelectionDrag>,
    ui: Single<(&mut Node, &mut Visibility), With<SelectionBoxUi>>,
) {
    let (mut node, mut visibility) = ui.into_inner();
    match drag.0 {
        Some((a, b)) => {
            let rect = Rect::from_corners(a, b);
            node.left = Val::Px(rect.min.x);
            node.top = Val::Px(rect.min.y);
            node.width = Val::Px(rect.width());
            node.height = Val::Px(rect.height());
            visibility.set_if_neq(Visibility::Inherited);
        }
        None => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}
