//! Les camps d'une partie : qui joue quelle faction, et où.
//!
//! Les catalogues (unités, bâtiments) et les règles (économie, combat) sont
//! communs à toute la partie : chaque faction y ajoute ses types à la suite
//! des autres. Un [`Side`] retient, pour un joueur, où commencent ceux de sa
//! faction : son type local `garde::kind::SENTINELLE` devient le type
//! `side.unit(SENTINELLE)` des catalogues.
//!
//! La carte est symétrique par rapport à son centre : la base et l'armée de
//! chaque joueur sont décrites une seule fois, pour le camp du nord (le
//! joueur humain, en haut de l'écran), et le camp adverse en est le miroir.

use std::f32::consts::PI;

use bevy::prelude::*;
use rts_engine::buildings::BuildingCatalog;
use rts_engine::navigation::GridFrame;
use rts_engine::players::PlayerId;
use rts_engine::units::UnitCatalog;

use crate::base::{harvest_at_start, START_HQ};
use crate::gisements::{self, DepositVisuals};
use crate::Faction;

/// Décalage des bases en partie normale : le Quartier général du camp nord
/// est à 33 m du centre, celui du camp sud à 33 m de l'autre côté, et les
/// armées de présentation se font face à 40 m l'une de l'autre.
const MATCH_SHIFT: Vec2 = Vec2::new(0.0, -20.0);

/// Ce que joue un joueur, et où ses types commencent dans les catalogues.
///
/// `Copy` : un `Side` est petit, on le passe par valeur sans `&`.
#[derive(Debug, Clone, Copy)]
pub struct Side {
    pub player: PlayerId,
    pub faction: Faction,
    /// Numéro de sa faction dans les catalogues.
    pub faction_id: usize,
    /// Indice de son premier type d'unité dans le catalogue commun.
    pub first_unit: usize,
    /// Indice de son premier type de bâtiment dans le catalogue commun.
    pub first_building: usize,
    /// Camp sud : sa base est le miroir de celle du nord.
    pub mirrored: bool,
}

impl Side {
    /// Le type d'unité local `kind` (voir `garde::kind`…) dans le catalogue commun.
    pub fn unit(self, kind: usize) -> usize {
        self.first_unit + kind
    }

    /// Le type de bâtiment local `building` dans le catalogue commun.
    pub fn building(self, building: usize) -> usize {
        self.first_building + building
    }

    /// Le point de la carte qui correspond, pour ce camp, au point `p` du
    /// plan de base (celui du camp nord, avant décalage).
    pub fn at(self, p: Vec2) -> Vec2 {
        let p = p + MATCH_SHIFT;
        if self.mirrored { -p } else { p }
    }

    /// L'orientation `yaw` du plan de base, pour ce camp.
    pub fn yaw(self, yaw: f32) -> f32 {
        if self.mirrored { yaw + PI } else { yaw }
    }
}

/// Ce que contient la base de départ d'une faction (types locaux).
pub struct StartPlan {
    /// Le bâtiment principal (dépôt), posé en [`START_HQ`].
    pub hq: usize,
    /// Les bâtiments de population et leurs centres.
    pub supply: usize,
    pub supply_at: Vec<Vec2>,
    /// L'ouvrier : quatre d'entre eux récoltent dès le départ.
    pub worker: usize,
}

/// Partie normale : la base de départ du camp `side` (bâtiment principal,
/// bâtiments de population, gisements, quatre ouvriers qui récoltent), puis
/// son armée de présentation.
pub fn spawn_start(
    commands: &mut Commands,
    side: Side,
    buildings: &BuildingCatalog,
    units: &UnitCatalog,
    deposits: &DepositVisuals,
    frame: GridFrame,
) {
    if let Some(plan) = side.faction.start_plan() {
        let mut place = |type_index: usize, at: Vec2| {
            let min_cell = frame.min_cell_centered(side.at(at), buildings.types[type_index].footprint);
            buildings.spawn(commands, frame, side.player, type_index, min_cell, None);
        };
        place(side.building(plan.hq), START_HQ);
        for &at in &plan.supply_at {
            place(side.building(plan.supply), at);
        }
        let layout = gisements::starting_base(START_HQ).mapped(|p| side.at(p));
        let (nodes, _) = gisements::spawn_base(commands, deposits, frame, &layout);
        let mut orders = Vec::new();
        for (i, x) in [-3.0, -1.0, 1.0, 3.0].into_iter().enumerate() {
            let at = side.at(START_HQ + Vec2::new(x, -4.6));
            let worker = units.spawn_owned(commands, side.player, side.unit(plan.worker), at, side.yaw(0.0));
            // Chacun son gisement, au milieu de la ligne.
            orders.push((worker, nodes[2 + i]));
        }
        harvest_at_start(commands, side.player, orders);
    }
    // L'armée, tournée vers la caméra (le camp sud : vers le nord).
    for (kind, pos) in side.faction.army() {
        units.spawn_owned(commands, side.player, side.unit(kind), side.at(pos), side.yaw(PI));
    }
}

/// Point du sol que la caméra regarde au départ d'une partie normale :
/// entre la base du joueur humain et son armée.
pub fn match_focus() -> Vec2 {
    Vec2::new(0.0, -6.0) + MATCH_SHIFT
}
