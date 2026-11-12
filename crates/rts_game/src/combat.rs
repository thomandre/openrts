//! Données de combat des trois factions : PV, armure, catégories et armes de
//! chaque unité et de chaque bâtiment (voir `rts_engine::combat`).
//!
//! Garde : PV, dégâts et portées du tableau de GAME_DESIGN.md ; armures,
//! cadences et cibles possibles sont **proposés**. Sylves et Collectif : tout
//! est **proposé**, pensé pour leurs rôles provisoires (à ajuster en test).
//! Les fiches suivent l'ordre des catalogues (`kind` et `building` de
//! chaque faction).
//!
//! Lecture d'une arme : `Weapon::new(dégâts, portée en m, secondes entre deux
//! coups, (touche le sol, touche l'air))`, puis éventuellement
//! `.with_bonus(catégorie, dégâts en plus)` et `.with_splash(rayon de zone)`.
//!
//! `.with_sight(m)` : rayon de vision (brouillard de guerre), mesuré depuis le
//! bord comme les portées, **proposé** et toujours au moins égal à la portée
//! d'acquisition (portée de l'arme + 3 m, au moins 6 m) : on ne vise que ce
//! qu'on voit. Ouvriers 7 m ; combattants 8 à 10 m ; tireurs à longue portée
//! (Longue-Vue, Lanceur de spores, Essaimeur, Titan) et éclaireurs volants
//! (Faucon, Samare) 10 à 12 m ; bâtiments principaux et défenses 10 m,
//! productions 8 m, population et extraction 6 m (le Résonateur, relais du
//! Collectif, 8 m).

use rts_engine::combat::Category::*;
use rts_engine::combat::{CombatRules, CombatStats, Weapon};

/// Sol seulement, air seulement, les deux.
const GROUND: (bool, bool) = (true, false);
const BOTH: (bool, bool) = (true, true);

/// Portée d'une arme de contact (mêlée, outil).
const CONTACT: f32 = 0.3;

/// La Garde : humains militaires, armes à distance, un blindé lourd.
pub fn garde() -> CombatRules {
    CombatRules {
        // Ordre de `garde::kind` : Technicien, Sentinelle, Longue-Vue, Lancier, Bastion, Faucon.
        units: vec![
            CombatStats::unit(40.0, 0.0, &[Infantry, Worker, Melee], vec![Weapon::new(5.0, CONTACT, 1.5, GROUND)]).with_sight(7.0),
            CombatStats::unit(50.0, 0.0, &[Infantry, Ranged], vec![Weapon::new(6.0, 5.0, 0.6, BOTH)]).with_sight(9.0),
            CombatStats::unit(40.0, 0.0, &[Infantry, Ranged], vec![Weapon::new(18.0, 9.0, 2.0, GROUND)]).with_sight(12.0),
            CombatStats::unit(70.0, 1.0, &[Infantry, Ranged], vec![Weapon::new(10.0, 6.0, 1.5, BOTH).with_bonus(Armored, 25.0)]).with_sight(9.0),
            CombatStats::unit(300.0, 2.0, &[Armored, Ranged], vec![Weapon::new(30.0, 6.0, 2.2, GROUND).with_splash(1.6)]).with_sight(10.0),
            // Faucon : il vole pour se déplacer (à 3 m au moins, au-dessus des
            // unités, bâtiments et falaises) et se pose pour combattre (voir
            // `HoverMotion::lands`) ; il compte comme une unité au sol au
            // combat (pas de catégorie `Air`) : toutes les armes le touchent.
            CombatStats::unit(45.0, 0.0, &[Infantry, Ranged], vec![Weapon::new(8.0, 4.0, 0.8, GROUND)]).with_sight(10.0),
        ],
        // Ordre de `garde::building` : Quartier général, Module d'habitat,
        // Extracteur, Caserne, Tourelle (défense fixe : sol et air), puis le
        // palier 2 : Armurerie, Centre de propulsion, Usine.
        buildings: vec![
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(10.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(6.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(6.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], vec![Weapon::new(12.0, 7.0, 0.9, BOTH)]).with_sight(10.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(6.0),
        ],
    }
}

/// Les Sylves : lentes, fortes au contact, pas d'infanterie rapide à distance.
pub fn sylves() -> CombatRules {
    CombatRules {
        // Ordre de `sylves::kind` : Pousse, Roncier, Sourcier, Lanceur de spores, Gardien, Samare.
        units: vec![
            CombatStats::unit(40.0, 0.0, &[Infantry, Worker, Melee], vec![Weapon::new(5.0, CONTACT, 1.5, GROUND)]).with_sight(7.0),
            CombatStats::unit(60.0, 1.0, &[Infantry, Melee], vec![Weapon::new(7.0, CONTACT, 0.8, GROUND)]).with_sight(8.0),
            // Soutien (le soin viendra plus tard) : le seul tir anti-aérien de l'infanterie.
            CombatStats::unit(70.0, 0.0, &[Infantry, Ranged], vec![Weapon::new(5.0, 4.0, 1.2, BOTH)]).with_sight(9.0),
            CombatStats::unit(110.0, 0.0, &[Armored, Ranged], vec![Weapon::new(20.0, 8.0, 3.0, GROUND).with_splash(2.0)]).with_sight(11.0),
            CombatStats::unit(500.0, 3.0, &[Armored, Massive, Melee], vec![Weapon::new(35.0, 1.0, 2.0, GROUND).with_bonus(Structure, 20.0)]).with_sight(9.0),
            CombatStats::unit(50.0, 0.0, &[Air, Ranged], vec![Weapon::new(6.0, 3.0, 1.0, BOTH).with_bonus(Worker, 6.0)]).with_sight(11.0),
        ],
        // Ordre de `sylves::building` : Arbre-mère, Souche nourricière,
        // Calice, Ronceraie, Épineux (défense fixe : sol et air), puis le
        // palier 2 : Fontaine de sève, Champignonnière, Érablière.
        buildings: vec![
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(10.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(6.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(6.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], vec![Weapon::new(15.0, 7.0, 1.2, BOTH)]).with_sight(10.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
        ],
    }
}

/// Le Collectif : beaucoup d'unités volantes, peu d'unités au sol mais
/// chères et solides.
pub fn collectif() -> CombatRules {
    CombatRules {
        // Ordre de `collectif::kind` : Collecteur, Automate, Perceur, Essaim, Essaimeur, Titan.
        units: vec![
            CombatStats::unit(45.0, 0.0, &[Air, Worker], vec![Weapon::new(5.0, 0.5, 1.5, GROUND)]).with_sight(7.0),
            CombatStats::unit(90.0, 1.0, &[Infantry, Ranged], vec![Weapon::new(9.0, 6.0, 0.9, BOTH)]).with_sight(9.0),
            CombatStats::unit(130.0, 1.0, &[Armored, Melee], vec![Weapon::new(12.0, 1.5, 1.2, GROUND).with_bonus(Armored, 20.0)]).with_sight(8.0),
            CombatStats::unit(60.0, 0.0, &[Air, Melee], vec![Weapon::new(4.0, 1.5, 0.4, BOTH)]).with_sight(9.0),
            // Ses drones ne sont pas encore simulés : une arme provisoire les représente.
            CombatStats::unit(350.0, 2.0, &[Air, Armored, Massive], vec![Weapon::new(8.0, 8.0, 0.5, BOTH)]).with_sight(11.0),
            CombatStats::unit(
                600.0,
                3.0,
                &[Armored, Massive, Ranged],
                vec![Weapon::new(40.0, 9.0, 2.8, GROUND).with_splash(1.8).with_bonus(Structure, 10.0)],
            )
            .with_sight(12.0),
        ],
        // Ordre de `collectif::building` : Écrin, Résonateur, Creuset,
        // Orfèvrerie, Aiguillon (défense fixe : sol et air, cadence rapide
        // mais dégâts faibles, valeurs proposées), puis le palier 2 :
        // Lapidaire, Volière, Fonderie.
        buildings: vec![
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(10.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(6.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], vec![Weapon::new(9.0, 7.0, 0.7, BOTH)]).with_sight(10.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
            CombatStats::structure(1.0, &[Armored], Vec::new()).with_sight(8.0),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On ne vise que ce qu'on voit : la vision de chaque fiche doit couvrir
    /// sa portée d'acquisition.
    #[test]
    fn sight_covers_acquisition_range() {
        for (name, rules) in [("Garde", garde()), ("Sylves", sylves()), ("Collectif", collectif())] {
            for (i, stats) in rules.units.iter().chain(&rules.buildings).enumerate() {
                assert!(stats.sight >= stats.acquire(), "{name}, fiche {i} : vision {} < acquisition {}", stats.sight, stats.acquire());
            }
        }
    }
}
