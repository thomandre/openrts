//! Interface de l'économie : traduction des boutons de production en
//! ordres, compteurs de ressources, panneau de production, messages.
//!
//! Comme pour la construction, rien ici ne change l'état du jeu : les
//! actions ([`GameAction`]) deviennent des ordres ([`PlayerCommand`]) dans
//! la file, que la simulation vérifie et applique au pas suivant.

use bevy::prelude::*;

use super::production::ProductionQueue;
use crate::buildings::{Building, BuildingCatalog, UnderConstruction};
use crate::input::GameAction;
use crate::orders::{CommandQueue, PlayerCommand};
use crate::players::{LocalPlayer, Owner};
use crate::selection::Selected;

/// Les bâtiments producteurs terminés du joueur local dans la sélection,
/// triés par identifiant. Le premier est celui dont le panneau est affiché.
pub(crate) fn selected_producers<'a>(
    local: LocalPlayer,
    producers: impl Iterator<Item = (Entity, &'a Owner, &'a ProductionQueue)>,
) -> Vec<Entity> {
    let mut list: Vec<Entity> = producers.filter(|(_, owner, _)| owner.0 == local.0).map(|(e, ..)| e).collect();
    list.sort_unstable();
    list
}

/// Consomme les actions de production et d'annulation.
pub(crate) fn handle_production_actions(
    mut actions: MessageReader<GameAction>,
    local: Res<LocalPlayer>,
    catalog: Option<Res<BuildingCatalog>>,
    producers: Query<(Entity, &Owner, &ProductionQueue, &Building), (With<Selected>, Without<UnderConstruction>)>,
    sites: Query<(Entity, &Owner), (With<Selected>, With<UnderConstruction>)>,
    mut orders: ResMut<CommandQueue>,
) {
    let Some(catalog) = catalog else {
        actions.clear();
        return;
    };
    for action in actions.read() {
        match *action {
            GameAction::Train { unit_type } => {
                // Parmi les bâtiments sélectionnés qui produisent cette unité :
                // la file la plus courte, puis le plus petit identifiant.
                let best = producers
                    .iter()
                    .filter(|(_, owner, _, building)| {
                        owner.0 == local.0 && catalog.types[building.type_index].production.iter().any(|o| o.unit_type == unit_type)
                    })
                    .min_by_key(|(entity, _, queue, _)| (queue.items.len(), *entity));
                if let Some((building, ..)) = best {
                    orders.push(local.0, PlayerCommand::Train { building, unit_type });
                }
            }
            GameAction::CancelQueued { slot } => {
                let shown = selected_producers(*local, producers.iter().map(|(e, o, q, _)| (e, o, q)));
                if let Some(&building) = shown.first() {
                    orders.push(local.0, PlayerCommand::CancelTrain { building, slot });
                }
            }
            GameAction::CancelConstruction => {
                let mut list: Vec<Entity> = sites.iter().filter(|(_, o)| o.0 == local.0).map(|(e, _)| e).collect();
                list.sort_unstable();
                for building in list {
                    orders.push(local.0, PlayerCommand::CancelConstruction { building });
                }
            }
            _ => {}
        }
    }
}
