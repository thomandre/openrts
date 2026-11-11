//! Joueurs : qui possède quoi, et l'état propre à chaque joueur.
//!
//! Chaque unité et chaque bâtiment porte un [`Owner`] : le joueur qui le
//! commande. Tout ce qui se compte (stocks de ressources, population,
//! bâtiments terminés pour les prérequis) se compte **par joueur**, dans la
//! ressource [`Players`]. Il n'y a pour l'instant qu'un joueur humain, mais
//! rien dans le moteur ne suppose qu'il est seul : une IA ou un adversaire en
//! réseau sera simplement un autre `PlayerId`.

use bevy::prelude::*;

use crate::economy::Stock;

/// Numéro d'un joueur (0 pour le premier).
///
/// `#[derive(...)]` demande au compilateur d'écrire tout seul du code
/// courant : copier (`Clone`, `Copy`), comparer (`PartialEq`, `Eq`, `Ord`),
/// afficher pour le débogage (`Debug`), valeur par défaut (`Default` : 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PlayerId(pub u8);

/// Le joueur propriétaire d'une unité ou d'un bâtiment.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Owner(pub PlayerId);

/// Le joueur qui joue sur cette machine : l'interface montre ses stocks, et
/// ses ordres partent sous son nom.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct LocalPlayer(pub PlayerId);

/// L'état d'un joueur, tenu par la simulation.
#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    /// Ressources en stock.
    pub stock: Stock,
    /// Population utilisée : unités vivantes et unités en file de production
    /// (recalculée à chaque pas, voir `economy::production`).
    pub population: u32,
    /// Population disponible : somme de ce que fournissent ses bâtiments
    /// terminés, plafonnée par [`crate::economy::PopulationSettings::max`].
    pub population_cap: u32,
}

/// Tous les joueurs de la partie, rangés par numéro : `players[PlayerId(2)]`
/// est le troisième. La liste grandit toute seule si l'on demande un joueur
/// qui n'existe pas encore (voir [`Players::get_mut`]).
#[derive(Resource, Debug, Clone, Default)]
pub struct Players(pub Vec<PlayerState>);

impl Players {
    /// L'état du joueur `id` (un état vide s'il n'existe pas).
    pub fn get(&self, id: PlayerId) -> PlayerState {
        self.0.get(usize::from(id.0)).cloned().unwrap_or_default()
    }

    /// L'état du joueur `id`, modifiable. `&mut` : un emprunt **exclusif**.
    /// Tant qu'on le tient, personne d'autre ne peut lire ni modifier la liste :
    /// c'est le « borrow checker » de Rust qui le garantit à la compilation.
    pub fn get_mut(&mut self, id: PlayerId) -> &mut PlayerState {
        let index = usize::from(id.0);
        if self.0.len() <= index {
            self.0.resize_with(index + 1, PlayerState::default);
        }
        &mut self.0[index]
    }
}

pub struct PlayersPlugin;

impl Plugin for PlayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalPlayer>().init_resource::<Players>();
    }
}
