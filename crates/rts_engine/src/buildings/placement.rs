//! Mode pose : choisir un bâtiment, le promener sous le pointeur (fantôme
//! vert ou rouge, aligné sur la grille), puis le poser.
//!
//! Ce module consomme les actions `ChooseBuilding`, `PlaceBuildingAt` et
//! `CancelPlacement` de la couche d'entrées : il ne sait pas si elles viennent
//! d'un clic, d'un raccourci clavier ou d'un tap. Il ne change pas lui-même
//! l'état du jeu : une pose acceptée devient un ordre
//! [`PlayerCommand::Build`], que la simulation appliquera (et revérifiera) à
//! son prochain pas.

use bevy::prelude::*;

use super::{completed_types, Building, BuildingCatalog, Footprint, GhostPiece, PlacementGhost, UnderConstruction};
use crate::camera::{screen_to_ground, RtsCamera};
use crate::cursor::Pointer;
use crate::economy::{Notice, ResourceNode};
use crate::input::{GameAction, InputContext};
use crate::navigation::NavGrid;
use crate::orders::{CommandQueue, PlayerCommand};
use crate::economy::NoticeKind;
use crate::players::{LocalPlayer, Owner, Players};
use crate::selection::Selected;
use crate::units::Unit;

/// Le mode pose : le type de bâtiment en cours de pose, s'il y en a un.
#[derive(Resource, Debug, Default)]
pub struct Placement {
    pub active: Option<usize>,
}

/// Les ouvriers de la sélection qui appartiennent au joueur local, triés
/// (ordre reproductible).
fn selected_builders(
    catalog: &BuildingCatalog,
    local: LocalPlayer,
    selected: &Query<(Entity, &Unit, &Owner), With<Selected>>,
) -> Vec<Entity> {
    let mut builders: Vec<Entity> = selected
        .iter()
        .filter(|(_, unit, owner)| catalog.builders.contains(&unit.type_index) && owner.0 == local.0)
        .map(|(entity, ..)| entity)
        .collect();
    builders.sort_unstable();
    builders
}

/// Consomme les actions de construction.
pub(crate) fn handle_build_actions(
    mut actions: MessageReader<GameAction>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    local: Res<LocalPlayer>,
    selected: Query<(Entity, &Unit, &Owner), With<Selected>>,
    buildings: Query<(&Building, &Owner, Has<UnderConstruction>)>,
    nodes: Query<(Entity, &ResourceNode, &Footprint)>,
    mut placement: ResMut<Placement>,
    mut orders: ResMut<CommandQueue>,
    mut context: ResMut<InputContext>,
    mut notices: MessageWriter<Notice>,
    players: Res<Players>,
) {
    let Some(catalog) = catalog else {
        actions.clear();
        return;
    };
    let (camera, camera_transform) = camera.into_inner();
    let builders = selected_builders(&catalog, *local, &selected);
    let completed = completed_types(&catalog, local.0, buildings.iter());

    for action in actions.read() {
        match *action {
            GameAction::ChooseBuilding { type_index } => {
                if type_index < catalog.types.len() && !builders.is_empty() && catalog.requirements_met(type_index, &completed) {
                    // Trop cher : on le dit tout de suite, sans entrer en mode pose.
                    if players.get(local.0).stock.covers(catalog.types[type_index].cost) {
                        placement.active = Some(type_index);
                    } else {
                        notices.write(Notice { player: local.0, kind: NoticeKind::NotEnoughResources });
                    }
                }
            }
            GameAction::PlaceBuildingAt { screen_pos, keep_placing } => {
                let (Some(type_index), Some(grid)) = (placement.active, grid.as_deref()) else { continue };
                let Some(ground) = screen_to_ground(camera, camera_transform, screen_pos) else { continue };
                let min_cell = catalog.snap_min_cell(type_index, ground.xz(), grid.frame(), nodes.iter());
                // Emplacement interdit : on le dit, et on reste en mode pose
                // (le fantôme est rouge). La simulation revérifiera de toute façon.
                if let Err(kind) = catalog.check_placement(type_index, min_cell, grid, &completed, nodes.iter()) {
                    notices.write(Notice { player: local.0, kind });
                    continue;
                }
                if !players.get(local.0).stock.covers(catalog.types[type_index].cost) {
                    notices.write(Notice { player: local.0, kind: NoticeKind::NotEnoughResources });
                    continue;
                }
                orders.push(local.0, PlayerCommand::Build { type_index, min_cell, builders: builders.clone() });
                if !keep_placing {
                    placement.active = None;
                }
            }
            GameAction::CancelPlacement => placement.active = None,
            _ => {}
        }
    }
    // Plus d'ouvrier sélectionné : plus de pose possible.
    if builders.is_empty() {
        placement.active = None;
    }
    context.placing = placement.active.is_some();
}

/// Le fantôme du mode pose suit le pointeur, aligné sur la grille.
pub(crate) fn update_pointer_ghost(
    mut commands: Commands,
    placement: Res<Placement>,
    pointer: Res<Pointer>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    nodes: Query<(Entity, &ResourceNode, &Footprint)>,
    mut ghosts: Query<(Entity, &mut PlacementGhost, &mut Visibility)>,
) {
    let (Some(catalog), Some(grid)) = (catalog, grid) else { return };
    let (camera, camera_transform) = camera.into_inner();
    let current = ghosts.iter_mut().find(|(_, ghost, _)| ghost.follows_pointer);
    let Some(type_index) = placement.active else {
        if let Some((entity, ..)) = current {
            commands.entity(entity).despawn();
        }
        return;
    };
    let ground = pointer.position.and_then(|pos| screen_to_ground(camera, camera_transform, pos));
    let min_cell = ground.map(|g| catalog.snap_min_cell(type_index, g.xz(), grid.frame(), nodes.iter()));
    match current {
        Some((_, mut ghost, mut visibility)) if ghost.type_index == type_index => {
            if let Some(min_cell) = min_cell {
                // `set_if_neq` : n'écrit (et ne signale un changement) que si la valeur change.
                ghost.set_if_neq(PlacementGhost { min_cell, ..*ghost });
            }
            visibility.set_if_neq(if min_cell.is_some() { Visibility::Inherited } else { Visibility::Hidden });
        }
        other => {
            // Pas de fantôme, ou celui d'un autre bâtiment : on (re)crée le bon.
            if let Some((entity, ..)) = other {
                commands.entity(entity).despawn();
            }
            let ghost = PlacementGhost {
                type_index,
                min_cell: min_cell.unwrap_or_default(),
                follows_pointer: true,
                valid: true,
            };
            catalog.spawn_ghost(&mut commands, ghost, grid.frame());
        }
    }
}

/// Place les fantômes sur leurs cases et les colore : vert si l'emplacement
/// est libre (dans la carte, sans bâtiment ni obstacle ; ou, pour un bâtiment
/// d'extraction, pile sur une source libre) et les prérequis remplis, rouge sinon.
pub(crate) fn refresh_ghosts(
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    local: Res<LocalPlayer>,
    buildings: Query<(&Building, &Owner, Has<UnderConstruction>)>,
    nodes: Query<(Entity, &ResourceNode, &Footprint)>,
    mut ghosts: Query<(&mut PlacementGhost, &mut Transform, &Children)>,
    mut pieces: Query<&mut MeshMaterial3d<StandardMaterial>, With<GhostPiece>>,
) {
    let (Some(catalog), Some(grid)) = (catalog, grid) else { return };
    let completed = completed_types(&catalog, local.0, buildings.iter());
    for (mut ghost, mut transform, children) in &mut ghosts {
        let size = catalog.types[ghost.type_index].footprint;
        let center = grid.frame().rect(ghost.min_cell, size).center();
        transform.translation = Vec3::new(center.x, 0.0, center.y);
        let valid = catalog.check_placement(ghost.type_index, ghost.min_cell, &grid, &completed, nodes.iter()).is_ok();
        if ghost.valid == valid && !ghost.is_added() {
            continue;
        }
        ghost.valid = valid;
        let material = catalog.ghost_material(valid);
        for child in children.iter() {
            if let Ok(mut piece) = pieces.get_mut(child) {
                piece.0 = material.clone();
            }
        }
    }
}
