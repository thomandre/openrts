//! Base de départ de la Coalition, et scène de démonstration des bâtiments.
//!
//! Partie normale : un Quartier général déjà construit près du centre de la
//! carte et quelques Techniciens devant lui, en plus de l'armée.
//!
//! `OPENRTS_DEMO=buildings` met en scène au démarrage, pour vérifier le rendu
//! d'un coup d'œil (et les captures) : les cinq bâtiments posés, un chantier
//! à mi-construction avec son Technicien, un fantôme vert et un rouge, et un
//! groupe d'unités qui contourne la Caserne (l'ordre part au bout de 11 s, pour
//! que la capture automatique, prise à 14 s, les montre en plein détour).

use bevy::prelude::*;
use rts_engine::camera::RtsCamera;
use rts_engine::buildings::{BuildingCatalog, ConstructionStyle, ConstructionTask, PlacementGhost};
use rts_engine::navigation::{GridFrame, MoveGroupCounter};
use rts_engine::selection::Selected;
use rts_engine::simulation::MoveTarget;
use rts_engine::units::UnitCatalog;

use crate::coalition::{self, building::*, kind::*};

/// Le catalogue des bâtiments de la Coalition. Ils partagent les matériaux
/// (et donc la couleur d'équipe) du catalogue d'unités `units`.
pub fn building_catalog(
    units: &UnitCatalog,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> BuildingCatalog {
    // Les Techniciens construisent, et restent sur le chantier jusqu'au bout.
    BuildingCatalog::new(
        coalition::building_types(),
        vec![TECHNICIEN],
        ConstructionStyle::WorkerStays,
        units,
        meshes,
        materials,
    )
}

/// Pose un bâtiment terminé centré au plus près du point (x, z).
fn place(commands: &mut Commands, catalog: &BuildingCatalog, frame: GridFrame, type_index: usize, at: Vec2) -> Entity {
    let min_cell = frame.min_cell_centered(at, catalog.types[type_index].footprint);
    catalog.spawn(commands, frame, type_index, min_cell, None)
}

/// Partie normale : le Quartier général et quatre Techniciens.
pub fn spawn_start(commands: &mut Commands, buildings: &BuildingCatalog, units: &UnitCatalog, frame: GridFrame) {
    place(commands, buildings, frame, QUARTIER_GENERAL, Vec2::new(0.0, -13.0));
    for x in [-3.0, -1.0, 1.0, 3.0] {
        units.spawn_facing(commands, TECHNICIEN, Vec2::new(x, -7.8), std::f32::consts::PI);
    }
}

/// Marqueur : les unités de la démonstration qui contourneront la Caserne.
#[derive(Component)]
struct DemoGroup;

/// La scène de démonstration des bâtiments.
pub fn spawn_demo(commands: &mut Commands, buildings: &BuildingCatalog, units: &UnitCatalog, frame: GridFrame) {
    let yaw = std::f32::consts::PI;
    // Les cinq bâtiments, terminés.
    place(commands, buildings, frame, QUARTIER_GENERAL, Vec2::new(-10.0, -9.0));
    place(commands, buildings, frame, CASERNE, Vec2::new(2.5, -9.0));
    place(commands, buildings, frame, HABITAT, Vec2::new(10.5, -11.0));
    place(commands, buildings, frame, EXTRACTEUR, Vec2::new(-19.0, -4.0));
    place(commands, buildings, frame, TOURELLE, Vec2::new(9.0, -3.5));

    // Un chantier de Caserne au tiers, et son Technicien au travail.
    let site_cell = frame.min_cell_centered(Vec2::new(15.5, 1.0), buildings.types[CASERNE].footprint);
    let site = buildings.spawn(commands, frame, CASERNE, site_cell, Some(0.2));
    let rect = frame.rect(site_cell, buildings.types[CASERNE].footprint);
    let worker = units.spawn_facing(commands, TECHNICIEN, Vec2::new(rect.min.x - 0.75, rect.center().y + 0.5), -std::f32::consts::FRAC_PI_2);
    commands.entity(worker).insert((ConstructionTask { sites: vec![site] }, Selected));

    // Deux fantômes : un Module d'habitat sur un terrain libre (vert), un
    // Extracteur à cheval sur le Quartier général (rouge).
    for (type_index, at) in [(HABITAT, Vec2::new(-12.0, 4.0)), (EXTRACTEUR, Vec2::new(-5.5, -4.5))] {
        let min_cell = frame.min_cell_centered(at, buildings.types[type_index].footprint);
        let ghost = PlacementGhost { type_index, min_cell, follows_pointer: false, valid: true };
        buildings.spawn_ghost(commands, ghost, frame);
    }

    // Le groupe qui va contourner la Caserne : devant elle, il visera l'arrière.
    let group = [SENTINELLE, SENTINELLE, SENTINELLE, SENTINELLE, LANCIER, LANCIER, LONGUE_VUE, BASTION, SENTINELLE, SENTINELLE];
    for (i, type_index) in group.into_iter().enumerate() {
        let pos = Vec2::new(-1.0 + (i % 5) as f32 * 1.3, 0.5 + (i / 5) as f32 * 1.6);
        let pos = if type_index == BASTION { Vec2::new(1.5, 4.8) } else { pos };
        let unit = units.spawn_facing(commands, type_index, pos, yaw);
        commands.entity(unit).insert(DemoGroup);
    }
    // Deux Faucons, qui survolent tout en ligne droite.
    for x in [5.0, 6.5] {
        let unit = units.spawn_facing(commands, FAUCON, Vec2::new(x, 3.0), yaw);
        commands.entity(unit).insert(DemoGroup);
    }
}

/// Donne l'ordre de contourner la Caserne, une seule fois, au bout de 11 s.
fn order_demo_group(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut groups: ResMut<MoveGroupCounter>,
    units: Query<Entity, With<DemoGroup>>,
    mut done: Local<bool>,
) {
    if *done || time.elapsed_secs() < 11.0 {
        return;
    }
    *done = true;
    let group = groups.next_group();
    let mut members: Vec<Entity> = units.iter().collect();
    members.sort_unstable();
    for (i, entity) in members.into_iter().enumerate() {
        let slot = Vec2::new(0.5 + (i % 4) as f32 * 1.4, -16.0 - (i / 4) as f32 * 1.4);
        commands.entity(entity).insert((MoveTarget(slot), group));
    }
}

/// `OPENRTS_FOCUS=x,z` : point du sol visé par la caméra au démarrage de la
/// démonstration (pour une vue rapprochée d'un bâtiment).
fn focus_camera(mut cameras: Query<&mut RtsCamera, Added<RtsCamera>>) {
    let Ok(text) = std::env::var("OPENRTS_FOCUS") else { return };
    let values: Vec<f32> = text.split(',').filter_map(|v| v.trim().parse().ok()).collect();
    if let [x, z] = values[..] {
        for mut camera in &mut cameras {
            camera.focus = Vec3::new(x, 0.0, z);
        }
    }
}

/// Active la démonstration (`OPENRTS_DEMO=buildings`).
pub struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (order_demo_group, focus_camera));
    }
}

/// La démonstration est-elle demandée ?
pub fn demo_requested() -> bool {
    std::env::var("OPENRTS_DEMO").is_ok_and(|d| d.eq_ignore_ascii_case("buildings"))
}
