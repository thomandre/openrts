//! Base de départ de la Coalition, et scènes de démonstration.
//!
//! Partie normale : un Quartier général déjà construit près du centre de la
//! carte, ses gisements et quatre Techniciens qui récoltent, en plus de l'armée.
//!
//! `OPENRTS_DEMO=economy` montre l'économie en marche : base de départ,
//! douze Techniciens sur la ligne de Cristal et trois à l'Extracteur posé sur
//! une source d'Éther, Quartier général sélectionné avec trois Techniciens en
//! file et son point de ralliement sur un gisement.
//!
//! `OPENRTS_DEMO=buildings` met en scène au démarrage, pour vérifier le rendu
//! d'un coup d'œil (et les captures) : les cinq bâtiments posés, un chantier
//! à mi-construction avec son Technicien, un fantôme vert et un rouge, et un
//! groupe d'unités qui contourne la Caserne (l'ordre part au bout de 11 s, pour
//! que la capture automatique, prise à 14 s, les montre en plein détour).

use bevy::prelude::*;
use rts_engine::camera::RtsCamera;
use rts_engine::buildings::{BuildingCatalog, ConstructionStyle, ConstructionTask, PlacementGhost};
use rts_engine::collision::StaticObstacle;
use rts_engine::economy::ResourceNode;
use rts_engine::navigation::GridFrame;
use rts_engine::orders::{CommandQueue, PlayerCommand};
use rts_engine::players::PlayerId;
use rts_engine::selection::Selected;
use rts_engine::units::UnitCatalog;

use crate::coalition::{self, building::*, kind::*};
use crate::gisements::{self, DepositVisuals};

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

/// Le joueur humain (le seul pour l'instant).
const HUMAN: PlayerId = PlayerId(0);

/// Pose un bâtiment terminé centré au plus près du point (x, z).
fn place(commands: &mut Commands, catalog: &BuildingCatalog, frame: GridFrame, type_index: usize, at: Vec2) -> Entity {
    let min_cell = frame.min_cell_centered(at, catalog.types[type_index].footprint);
    catalog.spawn(commands, frame, HUMAN, type_index, min_cell, None)
}

/// Centre du Quartier général de départ (les gisements se placent autour).
pub const START_HQ: Vec2 = Vec2::new(0.0, -13.0);

/// Partie normale : le Quartier général, deux Modules d'habitat, les
/// gisements et quatre Techniciens qui se mettent tout de suite à récolter.
pub fn spawn_start(
    commands: &mut Commands,
    buildings: &BuildingCatalog,
    units: &UnitCatalog,
    deposits: &DepositVisuals,
    frame: GridFrame,
) {
    place(commands, buildings, frame, QUARTIER_GENERAL, START_HQ);
    // Deux Modules d'habitat : l'armée de présentation occupe déjà plus que
    // les 15 de population du Quartier général.
    for x in [-8.5, 8.5] {
        place(commands, buildings, frame, HABITAT, Vec2::new(x, -8.5));
    }
    let (nodes, _) = gisements::spawn_base(commands, deposits, frame, &gisements::starting_base(START_HQ));
    let mut orders = Vec::new();
    for (i, x) in [-3.0, -1.0, 1.0, 3.0].into_iter().enumerate() {
        let worker = units.spawn_facing(commands, TECHNICIEN, START_HQ + Vec2::new(x, -4.6), 0.0);
        // Chacun son gisement, au milieu de la ligne.
        orders.push((worker, nodes[2 + i]));
    }
    harvest_at_start(commands, orders);
}

/// Donne des ordres de récolte au premier pas, par la file des ordres
/// (`commands.queue` : une fonction exécutée plus tard avec tout le monde
/// accessible, une fois les entités créées).
fn harvest_at_start(commands: &mut Commands, orders: Vec<(Entity, Entity)>) {
    commands.queue(move |world: &mut World| {
        let mut queue = world.resource_mut::<CommandQueue>();
        for (worker, node) in orders {
            queue.push(HUMAN, PlayerCommand::Harvest { units: vec![worker], node });
        }
    });
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
    let site = buildings.spawn(commands, frame, HUMAN, CASERNE, site_cell, Some(0.2));
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

/// Donne l'ordre de contourner la Caserne, une seule fois, au bout de 11 s
/// (par la file des ordres, comme le ferait le joueur).
fn order_demo_group(
    time: Res<Time<Real>>,
    mut orders: ResMut<CommandQueue>,
    units: Query<Entity, With<DemoGroup>>,
    mut done: Local<bool>,
) {
    if *done || time.elapsed_secs() < 11.0 {
        return;
    }
    *done = true;
    let mut members: Vec<Entity> = units.iter().collect();
    members.sort_unstable();
    orders.push(HUMAN, PlayerCommand::Move { units: members, target: Vec2::new(2.6, -16.7) });
}

/// `OPENRTS_FOCUS=x,z` : point du sol visé par la caméra au démarrage (pour
/// une vue rapprochée d'un bâtiment, d'un gisement…).
pub fn focus_camera(mut cameras: Query<&mut RtsCamera, Added<RtsCamera>>) {
    let Ok(text) = std::env::var("OPENRTS_FOCUS") else { return };
    let values: Vec<f32> = text.split(',').filter_map(|v| v.trim().parse().ok()).collect();
    if let [x, z] = values[..] {
        for mut camera in &mut cameras {
            camera.focus = Vec3::new(x, 0.0, z);
        }
    }
}

/// Active la démonstration des bâtiments (`OPENRTS_DEMO=buildings`).
pub struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, order_demo_group);
    }
}

/// Les scènes de démonstration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    Buildings,
    Economy,
}

/// La démonstration demandée (`OPENRTS_DEMO=buildings` ou `economy`), s'il y en a une.
pub fn demo_requested() -> Option<Demo> {
    match std::env::var("OPENRTS_DEMO").map(|d| d.to_lowercase()).as_deref() {
        Ok("buildings") => Some(Demo::Buildings),
        Ok("economy") => Some(Demo::Economy),
        _ => None,
    }
}

/// La démonstration de l'économie (voir l'en-tête du module).
pub fn spawn_economy_demo(
    commands: &mut Commands,
    buildings: &BuildingCatalog,
    units: &UnitCatalog,
    deposits: &DepositVisuals,
    frame: GridFrame,
) {
    let hq = place(commands, buildings, frame, QUARTIER_GENERAL, START_HQ);
    for x in [-8.5, 8.5] {
        place(commands, buildings, frame, HABITAT, Vec2::new(x, -8.5));
    }
    place(commands, buildings, frame, CASERNE, Vec2::new(14.5, -4.0));
    let layout = gisements::starting_base(START_HQ);
    let (nodes, sources) = gisements::spawn_base(commands, deposits, frame, &layout);
    // Un Extracteur terminé sur la source de gauche.
    let extractor = place(commands, buildings, frame, EXTRACTEUR, layout.rare[0].0);
    let source = sources[0];

    let mut harvest = Vec::new();
    // Douze Techniciens en rang derrière le Quartier général, répartis sur les gisements.
    for i in 0..12 {
        let worker = units.spawn_facing(commands, TECHNICIEN, START_HQ + Vec2::new(-8.25 + i as f32 * 1.5, -4.4), 0.0);
        harvest.push((worker, nodes[(i * 5 + 2) % nodes.len()]));
    }
    // Trois à l'Extracteur.
    for z in [-1.5, 0.0, 1.5] {
        let worker = units.spawn_facing(commands, TECHNICIEN, START_HQ + Vec2::new(-6.5, z), 0.0);
        harvest.push((worker, source));
    }
    commands.entity(hq).insert(Selected);
    let rally_node = nodes[6];
    commands.queue(move |world: &mut World| {
        // L'Extracteur couvre sa source (sa pose normale passe par la simulation).
        if let Some(mut node) = world.get_mut::<ResourceNode>(source) {
            node.extractor = Some(extractor);
        }
        let rally = world.get::<StaticObstacle>(rally_node).map(|o| o.0.center()).unwrap_or_default();
        let mut queue = world.resource_mut::<CommandQueue>();
        for (worker, node) in harvest {
            queue.push(HUMAN, PlayerCommand::Harvest { units: vec![worker], node });
        }
        queue.push(HUMAN, PlayerCommand::SetRally { buildings: vec![hq], point: rally, node: Some(rally_node) });
        for _ in 0..3 {
            queue.push(HUMAN, PlayerCommand::Train { building: hq, unit_type: TECHNICIEN });
        }
    });
}
