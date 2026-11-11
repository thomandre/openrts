//! Démonstration de bataille (`OPENRTS_DEMO=battle`) : deux armées de la
//! même faction (`OPENRTS_FACTION`, la Garde par défaut), le joueur 1 en
//! orange au sud, le joueur 2 en bleu au nord, chacune devant deux défenses
//! fixes (Tourelles de la Garde, Épineux des Sylves ; le Collectif n'a pas
//! encore de bâtiments). Au bout de quelques secondes, les deux armées
//! reçoivent un ordre d'attaque-déplacement vers la base adverse, par la file
//! des ordres comme le ferait un joueur, et se rencontrent au milieu.
//!
//! Avec `OPENRTS_STRESS=1` : 300 unités contre 300.

use bevy::prelude::*;
use rts_engine::buildings::BuildingCatalog;
use rts_engine::navigation::GridFrame;
use rts_engine::orders::{CommandQueue, PlayerCommand};
use rts_engine::players::{Owner, PlayerId};
use rts_engine::selection::Selected;
use rts_engine::units::{Unit, UnitCatalog};

use crate::{garde, sylves, collectif, Faction, ENEMY};

/// Le joueur humain.
const HUMAN: PlayerId = PlayerId(0);

/// Instant (secondes après le lancement) de l'ordre d'attaque.
const ATTACK_AT: f32 = 8.0;

/// Ligne de front de chaque armée au départ (distance au centre, en mètres).
const FRONT: f32 = 16.0;

/// La démonstration est-elle demandée (`OPENRTS_DEMO=battle`) ?
pub fn requested() -> bool {
    std::env::var("OPENRTS_DEMO").is_ok_and(|d| d.eq_ignore_ascii_case("battle"))
}

/// La composition d'une armée (types d'unités, des premiers rangs aux derniers).
fn army(faction: Faction, stress: bool) -> Vec<usize> {
    // (type, nombre) pour l'armée normale, d'une trentaine d'unités.
    let mix: Vec<(usize, usize)> = match faction {
        Faction::Garde => {
            use garde::kind::*;
            vec![(SENTINELLE, 14), (FAUCON, 4), (LANCIER, 5), (LONGUE_VUE, 4), (BASTION, 2)]
        }
        Faction::Sylves => {
            use sylves::kind::*;
            vec![(RONCIER, 14), (SAMARE, 5), (SOURCIER, 4), (LANCEUR, 4), (GARDIEN, 2)]
        }
        Faction::Collectif | Faction::CollectifOrRose => {
            use collectif::kind::*;
            vec![(AUTOMATE, 12), (PERCEUR, 5), (ESSAIM, 6), (ESSAIMEUR, 1), (TITAN, 2)]
        }
    };
    let total: usize = mix.iter().map(|(_, n)| n).sum();
    // Test de charge : les mêmes proportions, jusqu'à 300.
    let scale = |n: usize| if stress { (n * 300).div_ceil(total) } else { n };
    let mut list: Vec<usize> = mix.iter().flat_map(|&(kind, n)| std::iter::repeat_n(kind, scale(n))).collect();
    list.truncate(if stress { 300 } else { total });
    list
}

/// Places en rangs, du front (`front`, côté ennemi) vers l'arrière
/// (`back` : direction de l'arrière, ±z). Chaque rang fait au plus `width` m.
fn layout(catalog: &UnitCatalog, list: &[usize], front: f32, back: f32, width: f32) -> Vec<Vec2> {
    let mut slots = Vec::new();
    let mut row: Vec<(usize, f32)> = Vec::new();
    let mut depth = front;
    let flush = |row: &mut Vec<(usize, f32)>, depth: &mut f32, slots: &mut Vec<Vec2>| {
        if row.is_empty() {
            return;
        }
        let row_width: f32 = row.iter().map(|(_, d)| d).sum();
        let row_depth = row.iter().map(|(_, d)| *d).fold(0.0, f32::max);
        let mut x = -row_width / 2.0;
        for &(_, d) in row.iter() {
            slots.push(Vec2::new(x + d / 2.0, *depth + back * row_depth / 2.0));
            x += d;
        }
        *depth += back * row_depth;
        row.clear();
    };
    for &kind in list {
        let diameter = catalog.types[kind].footprint_radius * 2.0 + 0.5;
        if row.iter().map(|(_, d)| d).sum::<f32>() + diameter > width {
            flush(&mut row, &mut depth, &mut slots);
        }
        row.push((kind, diameter));
    }
    flush(&mut row, &mut depth, &mut slots);
    slots
}

/// Les deux armées et leurs défenses.
pub fn spawn(commands: &mut Commands, faction: Faction, units: &UnitCatalog, buildings: Option<&BuildingCatalog>, frame: GridFrame, stress: bool) {
    let list = army(faction, stress);
    let width = if stress { 34.0 } else { 22.0 };
    // Joueur 1 au sud (+z, côté caméra), regard vers le nord (−z) ; joueur 2 en face.
    for (player, back, yaw) in [(HUMAN, 1.0, 0.0), (ENEMY, -1.0, std::f32::consts::PI)] {
        for (&kind, slot) in list.iter().zip(layout(units, &list, back * FRONT, back, width)) {
            let unit = units.spawn_owned(commands, player, kind, slot, yaw);
            // L'armée du joueur est sélectionnée (anneaux, barre d'ordres),
            // sauf pour le test de charge, où 300 barres de vie masqueraient tout.
            if player == HUMAN && !stress {
                commands.entity(unit).insert(Selected);
            }
        }
        // Deux défenses fixes en retrait de l'armée.
        let defense = match faction {
            Faction::Garde => Some(garde::building::TOURELLE),
            Faction::Sylves => Some(sylves::building::EPINEUX),
            Faction::Collectif | Faction::CollectifOrRose => None,
        };
        if let (Some(catalog), Some(defense)) = (buildings, defense) {
            for x in [-9.0, 9.0] {
                let depth = if stress { 44.0 } else { FRONT + 12.0 };
                let min_cell = frame.min_cell_centered(Vec2::new(x, back * depth), catalog.types[defense].footprint);
                catalog.spawn(commands, frame, player, defense, min_cell, None);
            }
        }
    }
}

/// Au bout de [`ATTACK_AT`] secondes, chaque armée part à l'attaque de la
/// base adverse (une seule fois).
fn order_attack(time: Res<Time<Real>>, mut orders: ResMut<CommandQueue>, units: Query<(Entity, &Owner), With<Unit>>, mut done: Local<bool>) {
    if *done || time.elapsed_secs() < ATTACK_AT {
        return;
    }
    *done = true;
    for (player, goal) in [(HUMAN, Vec2::new(0.0, -30.0)), (ENEMY, Vec2::new(0.0, 30.0))] {
        let mut army: Vec<Entity> = units.iter().filter(|(_, o)| o.0 == player).map(|(e, _)| e).collect();
        army.sort_unstable();
        orders.push(player, PlayerCommand::AttackMove { units: army, target: goal });
    }
}

pub struct BattlePlugin;

impl Plugin for BattlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, order_attack);
    }
}
