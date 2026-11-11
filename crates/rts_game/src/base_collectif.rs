//! Base de départ du Collectif, et sa scène de démonstration.
//!
//! Partie normale (`OPENRTS_FACTION=collectif`) : un Écrin déjà serti près
//! du centre de la carte, trois Résonateurs, les gisements (même disposition
//! que pour la Garde) et quatre Collecteurs qui récoltent, en plus de
//! l'armée de présentation.
//!
//! `OPENRTS_FACTION=collectif OPENRTS_DEMO=buildings` met en scène au
//! démarrage, pour vérifier le rendu d'un coup d'œil (et les captures) : les
//! cinq bâtiments terminés, un Creuset sur une source d'Éther avec deux
//! Collecteurs à son bord, des Collecteurs sur la ligne de Cristal, une
//! Orfèvrerie en cours d'assemblage (sa gemme d'ancrage au-dessus), les
//! champs de résonance au sol, un fantôme vert dans un champ et un rouge
//! hors champ, et un Collecteur (sélectionné, donc avec la barre de
//! construction) qui quitte la ligne de Cristal au bout de 10 s pour aller
//! sertir un Aiguillon : la capture automatique, prise à 14 s, le montre
//! reparti vers sa récolte, la gemme posée.

use bevy::prelude::*;
use rts_engine::buildings::{BuildingCatalog, PlacementGhost};
use rts_engine::economy::ResourceNode;
use rts_engine::navigation::GridFrame;
use rts_engine::orders::{CommandQueue, PlayerCommand};
use rts_engine::selection::Selected;
use rts_engine::units::UnitCatalog;

use crate::base::{harvest_at_start, place, HUMAN, START_HQ};
use crate::collectif::{building::*, kind::*};
use crate::gisements::{self, DepositVisuals};

/// Marqueur : le Collecteur de la démonstration qui partira sertir un Aiguillon.
#[derive(Component)]
struct AnchoringCollecteur;

/// Où le Collecteur de la démonstration posera sa gemme (un Aiguillon, dans
/// le champ du Résonateur de gauche).
const DEMO_ANCHOR: Vec2 = Vec2::new(-5.0, -6.0);

/// La scène de démonstration (voir l'en-tête du module).
pub fn spawn_demo(
    commands: &mut Commands,
    buildings: &BuildingCatalog,
    units: &UnitCatalog,
    deposits: &DepositVisuals,
    frame: GridFrame,
) {
    let yaw = std::f32::consts::PI;
    place(commands, buildings, frame, ECRIN, START_HQ);
    // Deux Résonateurs, et dans le champ de celui de droite, l'Orfèvrerie
    // et un Aiguillon.
    place(commands, buildings, frame, RESONATEUR, Vec2::new(9.0, -4.0));
    place(commands, buildings, frame, RESONATEUR, Vec2::new(-9.0, -5.0));
    place(commands, buildings, frame, ORFEVRERIE, Vec2::new(13.5, -8.5));
    place(commands, buildings, frame, AIGUILLON, Vec2::new(5.5, -1.0));
    let layout = gisements::starting_base(START_HQ);
    let (nodes, sources) = gisements::spawn_base(commands, deposits, frame, &layout);
    // Un Creuset terminé sur la source de gauche.
    let creuset = place(commands, buildings, frame, CREUSET, layout.rare[0].0);
    let source = sources[0];

    // Une seconde Orfèvrerie aux deux cinquièmes de son assemblage, dans le
    // champ du Résonateur de gauche (sa gemme d'ancrage flotte au-dessus).
    let site_cell = frame.min_cell_centered(Vec2::new(-13.0, -1.5), buildings.types[ORFEVRERIE].footprint);
    buildings.spawn(commands, frame, HUMAN, ORFEVRERIE, site_cell, Some(0.4));

    // Deux fantômes : un Aiguillon dans le champ du Résonateur de droite
    // (vert), une Orfèvrerie loin de tout champ (rouge).
    for (type_index, at) in [(AIGUILLON, Vec2::new(12.0, -1.0)), (ORFEVRERIE, Vec2::new(2.0, 4.5))] {
        let min_cell = frame.min_cell_centered(at, buildings.types[type_index].footprint);
        let ghost = PlacementGhost { type_index, min_cell, follows_pointer: false, valid: true };
        buildings.spawn_ghost(commands, ghost, frame);
    }

    // Six Collecteurs sur la ligne de Cristal, deux au bord du Creuset.
    let mut harvest = Vec::new();
    for i in 0..6 {
        let worker = units.spawn_facing(commands, COLLECTEUR, START_HQ + Vec2::new(-5.0 + i as f32 * 2.0, -4.6), 0.0);
        harvest.push((worker, nodes[1 + i]));
    }
    for z in [-1.0, 1.0] {
        let worker = units.spawn_facing(commands, COLLECTEUR, START_HQ + Vec2::new(-6.5, z), 0.0);
        harvest.push((worker, source));
    }
    // Le Collecteur qui partira sertir : il récolte au bout de la ligne,
    // sélectionné (la barre de construction s'affiche).
    let anchoring = units.spawn_facing(commands, COLLECTEUR, START_HQ + Vec2::new(-7.0, -4.0), 0.0);
    commands.entity(anchoring).insert((AnchoringCollecteur, Selected));
    harvest.push((anchoring, nodes[0]));
    // Quelques Automates sortis de l'Orfèvrerie.
    for (x, z) in [(15.5, -3.6), (16.9, -2.9), (15.3, -2.1), (16.7, -1.4)] {
        units.spawn_facing(commands, AUTOMATE, Vec2::new(x, z), yaw);
    }
    commands.queue(move |world: &mut World| {
        // Le Creuset couvre sa source (sa pose normale passe par la simulation).
        if let Some(mut node) = world.get_mut::<ResourceNode>(source) {
            node.extractor = Some(creuset);
        }
    });
    harvest_at_start(commands, HUMAN, harvest);
}

/// Envoie le Collecteur de la démonstration sertir un Aiguillon, une seule
/// fois, au bout de 10 s (par la file des ordres, comme le ferait le joueur).
fn order_anchoring(
    time: Res<Time<Real>>,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<rts_engine::navigation::NavGrid>>,
    mut orders: ResMut<CommandQueue>,
    collecteur: Query<Entity, With<AnchoringCollecteur>>,
    mut done: Local<bool>,
) {
    let (Some(catalog), Some(grid)) = (catalog, grid) else { return };
    if *done || time.elapsed_secs() < 10.0 {
        return;
    }
    *done = true;
    let min_cell = grid.frame().min_cell_centered(DEMO_ANCHOR, catalog.types[AIGUILLON].footprint);
    let builders: Vec<Entity> = collecteur.iter().collect();
    orders.push(HUMAN, PlayerCommand::Build { type_index: AIGUILLON, min_cell, builders });
}

/// Active la démonstration du Collectif (`OPENRTS_FACTION=collectif OPENRTS_DEMO=buildings`).
pub struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, order_anchoring);
    }
}
