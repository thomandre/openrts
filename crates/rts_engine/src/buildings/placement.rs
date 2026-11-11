//! Mode pose : choisir un bâtiment, le promener sous le pointeur (fantôme
//! vert ou rouge, aligné sur la grille), puis le poser.
//!
//! Ce module consomme les actions `ChooseBuilding`, `PlaceBuildingAt` et
//! `CancelPlacement` de la couche d'entrées : il ne sait pas si elles viennent
//! d'un clic, d'un raccourci clavier ou d'un tap. Il ne change pas lui-même
//! l'état du jeu : une pose acceptée devient une [`BuildOrder`], que la
//! simulation appliquera (et revérifiera) à son prochain pas.

use bevy::prelude::*;

use super::{
    completed_types, Building, BuildingCatalog, BuildOrder, ConstructionTask, GhostPiece, PendingBuildOrders,
    PlacementGhost, UnderConstruction,
};
use crate::camera::{screen_to_ground, RtsCamera};
use crate::cursor::Pointer;
use crate::input::{GameAction, InputContext};
use crate::navigation::NavGrid;
use crate::selection::Selected;
use crate::units::Unit;

/// Le mode pose : le type de bâtiment en cours de pose, s'il y en a un.
#[derive(Resource, Debug, Default)]
pub struct Placement {
    pub active: Option<usize>,
}

/// Les ouvriers de la sélection, triés (ordre reproductible).
fn selected_builders(catalog: &BuildingCatalog, selected: &Query<(Entity, &Unit), With<Selected>>) -> Vec<Entity> {
    let mut builders: Vec<Entity> = selected
        .iter()
        .filter(|(_, unit)| catalog.builders.contains(&unit.type_index))
        .map(|(entity, _)| entity)
        .collect();
    builders.sort_unstable();
    builders
}

/// Consomme les actions de construction.
pub(crate) fn handle_build_actions(
    mut commands: Commands,
    mut actions: MessageReader<GameAction>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    selected: Query<(Entity, &Unit), With<Selected>>,
    buildings: Query<(&Building, Has<UnderConstruction>)>,
    mut placement: ResMut<Placement>,
    mut orders: ResMut<PendingBuildOrders>,
    mut context: ResMut<InputContext>,
) {
    let Some(catalog) = catalog else {
        actions.clear();
        return;
    };
    let (camera, camera_transform) = camera.into_inner();
    let builders = selected_builders(&catalog, &selected);
    let completed = completed_types(&catalog, buildings.iter().map(|(b, c)| (b.type_index, c)));

    for action in actions.read() {
        match *action {
            GameAction::ChooseBuilding { type_index } => {
                if type_index < catalog.types.len() && !builders.is_empty() && catalog.requirements_met(type_index, &completed) {
                    placement.active = Some(type_index);
                }
            }
            GameAction::PlaceBuildingAt { screen_pos, keep_placing } => {
                let (Some(type_index), Some(grid)) = (placement.active, grid.as_deref()) else { continue };
                let Some(ground) = screen_to_ground(camera, camera_transform, screen_pos) else { continue };
                let size = catalog.types[type_index].footprint;
                let min_cell = grid.frame().min_cell_centered(ground.xz(), size);
                // Emplacement interdit : on reste en mode pose (le fantôme est rouge).
                if !grid.area_free(min_cell, size) {
                    continue;
                }
                orders.0.push(BuildOrder { type_index, min_cell, builders: builders.clone() });
                if !keep_placing {
                    placement.active = None;
                }
            }
            GameAction::CancelPlacement => placement.active = None,
            // Un ordre de déplacement donné à la main détourne les ouvriers de leurs chantiers.
            GameAction::CommandAt { .. } => {
                for &builder in &builders {
                    commands.entity(builder).remove::<ConstructionTask>();
                }
            }
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
    let size = catalog.types[type_index].footprint;
    let ground = pointer.position.and_then(|pos| screen_to_ground(camera, camera_transform, pos));
    let min_cell = ground.map(|g| grid.frame().min_cell_centered(g.xz(), size));
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
/// est libre (dans la carte, sans bâtiment ni obstacle) et les prérequis
/// remplis, rouge sinon.
pub(crate) fn refresh_ghosts(
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    buildings: Query<(&Building, Has<UnderConstruction>)>,
    mut ghosts: Query<(&mut PlacementGhost, &mut Transform, &Children)>,
    mut pieces: Query<&mut MeshMaterial3d<StandardMaterial>, With<GhostPiece>>,
) {
    let (Some(catalog), Some(grid)) = (catalog, grid) else { return };
    let completed = completed_types(&catalog, buildings.iter().map(|(b, c)| (b.type_index, c)));
    for (mut ghost, mut transform, children) in &mut ghosts {
        let size = catalog.types[ghost.type_index].footprint;
        let center = grid.frame().rect(ghost.min_cell, size).center();
        transform.translation = Vec3::new(center.x, 0.0, center.y);
        let valid = grid.area_free(ghost.min_cell, size) && catalog.requirements_met(ghost.type_index, &completed);
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
