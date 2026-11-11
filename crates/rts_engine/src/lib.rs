//! Moteur RTS générique, construit sur Bevy.
//!
//! Cette crate ne connaît aucune faction ni aucune unité précise : elle fournit
//! la caméra, la couche d'entrées par actions, la sélection, la file des
//! ordres des joueurs (`orders`), les joueurs (`players`), la navigation
//! (grille, A*), les bâtiments (pose, chantiers), l'économie, le combat, une IA
//! adverse basique (`ai`) et le rendu
//! (module `graphics`). Le contenu (unités, bâtiments, cartes…) vit
//! dans la crate `rts_game`.

pub mod ai;
pub mod buildings;
pub mod camera;
pub mod collision;
pub mod combat;
pub mod cursor;
pub mod devtools;
pub mod economy;
pub mod graphics;
pub mod input;
pub mod navigation;
pub mod orders;
pub mod players;
pub mod selection;
pub mod simulation;
pub mod units;

use bevy::prelude::*;

/// Ordre d'exécution des systèmes à chaque image :
/// d'abord on lit le matériel (souris, clavier, tactile) et on le traduit
/// en actions, puis la logique de jeu consomme ces actions.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RtsSystems {
    /// Position du pointeur (curseur virtuel ou curseur du système).
    Pointer,
    /// Traduction des entrées brutes en actions de jeu.
    Input,
    /// Consommation des actions (caméra, sélection, ordres).
    Actions,
}

/// Plugin principal : l'ajouter à l'`App` active tout le moteur.
pub struct RtsEnginePlugin;

impl Plugin for RtsEnginePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, (RtsSystems::Pointer, RtsSystems::Input, RtsSystems::Actions).chain())
            .add_plugins((
                cursor::CursorPlugin,
                input::InputPlugin,
                camera::RtsCameraPlugin,
                selection::SelectionPlugin,
                simulation::SimulationPlugin,
                players::PlayersPlugin,
                orders::OrdersPlugin,
                economy::EconomyPlugin,
                economy::EconomyUiPlugin,
                combat::CombatPlugin,
                combat::ui::CombatUiPlugin,
                combat::effects::CombatEffectsPlugin,
                buildings::BuildingsPlugin,
                graphics::GraphicsPlugin,
                devtools::DevToolsPlugin,
            ))
            // Un `n`-uplet de plugins en compte au plus 15 : celui-ci à part.
            .add_plugins(ai::AiPlugin);
    }
}
