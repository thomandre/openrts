//! Sélection des unités et ordres contextuels.
//!
//! Consomme les actions `SelectAt`, `SelectBox` et `CommandAt` produites par
//! la couche d'entrées, quel que soit le périphérique d'origine. Les ordres
//! ne sont pas appliqués ici : ils partent dans la file des ordres
//! ([`CommandQueue`]), appliquée au prochain pas de simulation.

use bevy::prelude::*;

use crate::camera::{ray_at_height, screen_ray, screen_to_ground};
use crate::input::{GameAction, SelectionDrag};
use crate::orders::{CommandQueue, PlayerCommand};
use crate::players::{LocalPlayer, Owner};
use crate::simulation::MoveSpeed;
use crate::units::UnitCatalog;
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

fn handle_actions(
    mut commands: Commands,
    mut actions: MessageReader<GameAction>,
    camera: Single<(&Camera, &GlobalTransform)>,
    units: Query<(Entity, &GlobalTransform, &Selectable)>,
    selected: Query<Entity, With<Selected>>,
    // Seules les entités qui peuvent bouger (pas les bâtiments) reçoivent les ordres de déplacement.
    mobile: Query<(), With<MoveSpeed>>,
    owners: Query<&Owner>,
    local: Res<LocalPlayer>,
    mut orders: ResMut<CommandQueue>,
    catalog: Option<Res<UnitCatalog>>,
) {
    // `Single` garantit qu'il y a exactement une caméra ; `into_inner` donne ses composants.
    let (camera, camera_transform) = camera.into_inner();

    for action in actions.read() {
        match *action {
            GameAction::SelectAt { screen_pos, additive } => {
                let Some(ray) = screen_ray(camera, camera_transform, screen_pos) else { continue };
                // Pour chaque unité, on regarde où le rayon traverse le plan horizontal
                // passant par son centre : si c'est dans son rayon, elle est touchée.
                // On garde la plus proche de la caméra.
                let hit = units
                    .iter()
                    .filter_map(|(entity, transform, selectable)| {
                        let center = transform.translation();
                        let point = ray_at_height(ray, center.y)?;
                        selectable
                            .contains(center.xz(), point.xz())
                            .then(|| (entity, point.distance(ray.origin)))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(entity, _)| entity);

                if !additive {
                    clear_selection(&mut commands, &selected);
                }
                if let Some(entity) = hit {
                    if additive && selected.contains(entity) {
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
                if !additive {
                    clear_selection(&mut commands, &selected);
                }
                let inside: Vec<Entity> = units
                    .iter()
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
                // Les unités sélectionnées qui peuvent bouger et que l'on commande.
                let mut movers: Vec<Entity> = selected
                    .iter()
                    .filter(|&e| mobile.contains(e) && owners.get(e).is_ok_and(|o| o.0 == local.0))
                    .collect();
                // Ordre fixe (la sélection est parcourue dans un ordre quelconque).
                movers.sort_unstable();
                if !movers.is_empty() {
                    orders.push(local.0, PlayerCommand::Move { units: movers, target: target.xz() });
                }
            }

            GameAction::SpawnStressTest => {
                if let Some(catalog) = &catalog {
                    spawn_stress_test(&mut commands, catalog);
                }
            }
            // Actions de construction : voir le module `buildings`.
            GameAction::ChooseBuilding { .. } | GameAction::PlaceBuildingAt { .. } | GameAction::CancelPlacement => {}
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
fn show_selection_rings(
    units: Query<(Has<Selected>, &Children), With<Selectable>>,
    mut rings: Query<&mut Visibility, With<SelectionRing>>,
) {
    for (is_selected, children) in &units {
        let wanted = if is_selected { Visibility::Inherited } else { Visibility::Hidden };
        for child in children.iter() {
            if let Ok(mut visibility) = rings.get_mut(child) {
                // `set_if_neq` n'écrit que si la valeur change (évite du travail inutile à Bevy).
                visibility.set_if_neq(wanted);
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
