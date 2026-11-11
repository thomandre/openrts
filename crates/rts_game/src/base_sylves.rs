//! Base de départ des Sylves, et leur scène de démonstration.
//!
//! Partie normale (`OPENRTS_FACTION=sylves`) : un Arbre-mère déjà poussé
//! près du centre de la carte, deux Souches nourricières, les gisements (même
//! disposition que pour la Garde) et quatre Pousses qui récoltent, en
//! plus de l'armée de présentation.
//!
//! `OPENRTS_FACTION=sylves OPENRTS_DEMO=buildings` met en scène au démarrage,
//! pour vérifier le rendu d'un coup d'œil (et les captures) : les cinq
//! bâtiments poussés, un Calice sur une source d'Éther avec deux Pousses à
//! son pied, des Pousses sur la ligne de Cristal, une Ronceraie en pleine
//! croissance, un fantôme vert et un rouge, et une Pousse (sélectionnée,
//! donc avec la barre de construction) qui part s'enraciner : l'ordre part au
//! bout de 10,5 s, pour que la capture automatique, prise à 14 s, la montre en
//! route vers son emplacement promis.

use bevy::prelude::*;
use rts_engine::buildings::{BuildingCatalog, PlacementGhost, RootedWorker};
use rts_engine::economy::ResourceNode;
use rts_engine::navigation::GridFrame;
use rts_engine::orders::{CommandQueue, PlayerCommand};
use rts_engine::selection::Selected;
use rts_engine::units::UnitCatalog;

use crate::base::{harvest_at_start, place, HUMAN, START_HQ};
use crate::gisements::{self, DepositVisuals};
use crate::sylves::{self, building::*, kind::*};

/// Le catalogue des bâtiments des Sylves : les Pousses construisent en
/// devenant le bâtiment (voir [`sylves::CONSTRUCTION`]).
pub fn building_catalog(
    units: &UnitCatalog,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> BuildingCatalog {
    BuildingCatalog::new(sylves::building_types(), vec![POUSSE], sylves::CONSTRUCTION, units, meshes, materials)
}

/// Partie normale : l'Arbre-mère, deux Souches nourricières, les gisements
/// et quatre Pousses qui se mettent tout de suite à récolter.
pub fn spawn_start(
    commands: &mut Commands,
    buildings: &BuildingCatalog,
    units: &UnitCatalog,
    deposits: &DepositVisuals,
    frame: GridFrame,
) {
    place(commands, buildings, frame, ARBRE_MERE, START_HQ);
    // Deux Souches : l'armée de présentation occupe déjà plus que les 15 de
    // population de l'Arbre-mère.
    for x in [-8.5, 8.5] {
        place(commands, buildings, frame, SOUCHE, Vec2::new(x, -8.5));
    }
    let (nodes, _) = gisements::spawn_base(commands, deposits, frame, &gisements::starting_base(START_HQ));
    let mut orders = Vec::new();
    for (i, x) in [-3.0, -1.0, 1.0, 3.0].into_iter().enumerate() {
        let worker = units.spawn_facing(commands, POUSSE, START_HQ + Vec2::new(x, -4.6), 0.0);
        orders.push((worker, nodes[2 + i]));
    }
    harvest_at_start(commands, orders);
}

/// Marqueur : la Pousse de la démonstration qui partira s'enraciner.
#[derive(Component)]
struct RootingPousse;

/// Où la Pousse de la démonstration ira s'enraciner (une Souche nourricière).
const DEMO_PLAN: Vec2 = Vec2::new(-1.5, 3.5);

/// La scène de démonstration (voir l'en-tête du module).
pub fn spawn_demo(
    commands: &mut Commands,
    buildings: &BuildingCatalog,
    units: &UnitCatalog,
    deposits: &DepositVisuals,
    frame: GridFrame,
) {
    place(commands, buildings, frame, ARBRE_MERE, START_HQ);
    place(commands, buildings, frame, SOUCHE, Vec2::new(8.5, -8.5));
    place(commands, buildings, frame, RONCERAIE, Vec2::new(14.5, -3.5));
    place(commands, buildings, frame, EPINEUX, Vec2::new(8.0, -2.0));
    let layout = gisements::starting_base(START_HQ);
    let (nodes, sources) = gisements::spawn_base(commands, deposits, frame, &layout);
    // Un Calice poussé sur la source de gauche.
    let calice = place(commands, buildings, frame, CALICE, layout.rare[0].0);
    let source = sources[0];

    // Une Ronceraie aux deux cinquièmes de sa croissance, issue d'une Pousse.
    let site_cell = frame.min_cell_centered(Vec2::new(-13.5, -2.5), buildings.types[RONCERAIE].footprint);
    let site = buildings.spawn(commands, frame, HUMAN, RONCERAIE, site_cell, Some(0.4));
    commands.entity(site).insert(RootedWorker { unit_type: POUSSE });

    // Deux fantômes : un Épineux sur un terrain libre (vert), un Calice
    // loin de toute source (rouge).
    for (type_index, at) in [(EPINEUX, Vec2::new(4.0, 1.0)), (CALICE, Vec2::new(-6.0, 1.5))] {
        let min_cell = frame.min_cell_centered(at, buildings.types[type_index].footprint);
        let ghost = PlacementGhost { type_index, min_cell, follows_pointer: false, valid: true };
        buildings.spawn_ghost(commands, ghost, frame);
    }

    // Six Pousses sur la ligne de Cristal, deux au pied du Calice.
    let mut harvest = Vec::new();
    for i in 0..6 {
        let worker = units.spawn_facing(commands, POUSSE, START_HQ + Vec2::new(-5.0 + i as f32 * 2.0, -4.6), 0.0);
        harvest.push((worker, nodes[1 + i]));
    }
    for z in [-1.0, 1.0] {
        let worker = units.spawn_facing(commands, POUSSE, START_HQ + Vec2::new(-6.5, z), 0.0);
        harvest.push((worker, source));
    }
    // La Pousse qui partira s'enraciner, sélectionnée.
    let rooting = units.spawn_facing(commands, POUSSE, Vec2::new(16.0, 7.0), std::f32::consts::PI);
    commands.entity(rooting).insert((RootingPousse, Selected));
    // Quelques Ronciers sortis de la Ronceraie.
    for (x, z) in [(11.0, 1.5), (12.4, 2.2), (13.8, 1.4), (15.2, 2.1)] {
        units.spawn_facing(commands, RONCIER, Vec2::new(x, z), std::f32::consts::PI);
    }
    commands.queue(move |world: &mut World| {
        // Le Calice couvre sa source (sa pose normale passe par la simulation).
        if let Some(mut node) = world.get_mut::<ResourceNode>(source) {
            node.extractor = Some(calice);
        }
    });
    harvest_at_start(commands, harvest);
}

/// Envoie la Pousse de la démonstration s'enraciner, une seule fois, au bout
/// de 10,5 s (par la file des ordres, comme le ferait le joueur).
fn order_rooting(
    time: Res<Time<Real>>,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<rts_engine::navigation::NavGrid>>,
    mut orders: ResMut<CommandQueue>,
    pousse: Query<Entity, With<RootingPousse>>,
    mut done: Local<bool>,
) {
    let (Some(catalog), Some(grid)) = (catalog, grid) else { return };
    if *done || time.elapsed_secs() < 10.5 {
        return;
    }
    *done = true;
    let min_cell = grid.frame().min_cell_centered(DEMO_PLAN, catalog.types[SOUCHE].footprint);
    let builders: Vec<Entity> = pousse.iter().collect();
    orders.push(HUMAN, PlayerCommand::Build { type_index: SOUCHE, min_cell, builders });
}

/// Active la démonstration des Sylves (`OPENRTS_FACTION=sylves OPENRTS_DEMO=buildings`).
pub struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, order_rooting);
    }
}
