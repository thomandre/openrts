//! File des ordres des joueurs : le seul chemin par lequel un joueur agit
//! sur la partie.
//!
//! L'interface (souris, clavier, tactile, boutons) ne modifie jamais l'état
//! du jeu elle-même : elle traduit les actions du joueur en
//! [`PlayerCommand`] (déplacer ces unités, récolter ce gisement, poser ce
//! bâtiment, produire cette unité…) et les range dans la [`CommandQueue`].
//! Au **pas de simulation suivant** (`FixedUpdate`), la file est vidée d'un
//! coup dans [`TickCommands`], et chaque module du moteur applique les ordres
//! qui le concernent, dans l'ordre où ils ont été donnés, après les avoir
//! revérifiés (le joueur possède-t-il ces unités ? a-t-il de quoi payer ?).
//!
//! # Pourquoi cette file : le futur multijoueur en lockstep
//!
//! En lockstep (« pas de l'oie »), les machines ne s'envoient pas l'état du
//! jeu (des milliers d'unités), seulement les **ordres** des joueurs. Chaque
//! machine fait tourner la même simulation déterministe avec les mêmes
//! ordres aux mêmes pas, et obtient donc exactement la même partie. Cette
//! file en est le point de passage unique : plus tard, chaque lot d'ordres
//! sera estampillé d'un numéro de pas futur (« appliqué au pas n + 2 »),
//! envoyé aux autres joueurs, et le pas n + 2 n'avancera qu'une fois les
//! ordres de tous reçus. Il faudra aussi remplacer les `Entity` de Bevy, qui
//! diffèrent d'une machine à l'autre, par des identifiants de simulation
//! identiques partout.

use bevy::prelude::*;

use crate::buildings::ConstructionTask;
use crate::navigation::MoveGroupCounter;
use crate::players::{Owner, PlayerId};
use crate::selection::{formation, Selectable};
use crate::simulation::{MoveSpeed, MoveTarget, SimSet};

/// Un ordre de joueur, indépendant du périphérique et de l'interface.
///
/// `enum` : un type qui vaut **une** des variantes ci-dessous, chacune avec
/// ses propres données. `match` oblige à traiter toutes les variantes.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerCommand {
    /// Aller au point visé, en formation.
    Move { units: Vec<Entity>, target: Vec2 },
    /// Poser un chantier ; l'ouvrier le plus proche parmi `builders` s'en charge.
    Build { type_index: usize, min_cell: IVec2, builders: Vec<Entity> },
}

/// Un ordre et le joueur qui l'a donné.
#[derive(Debug, Clone, PartialEq)]
pub struct IssuedCommand {
    pub player: PlayerId,
    pub command: PlayerCommand,
}

/// Les ordres donnés depuis le dernier pas de simulation (remplie dans `Update`).
#[derive(Resource, Debug, Default)]
pub struct CommandQueue(Vec<IssuedCommand>);

impl CommandQueue {
    /// Ajoute un ordre du joueur `player`, appliqué au prochain pas.
    pub fn push(&mut self, player: PlayerId, command: PlayerCommand) {
        self.0.push(IssuedCommand { player, command });
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Les ordres à appliquer pendant le pas de simulation en cours, dans
/// l'ordre où ils ont été donnés.
#[derive(Resource, Debug, Default)]
pub struct TickCommands(pub Vec<IssuedCommand>);

pub struct OrdersPlugin;

impl Plugin for OrdersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandQueue>()
            .init_resource::<TickCommands>()
            .add_systems(FixedUpdate, collect_commands.in_set(SimSet::CollectOrders))
            .add_systems(FixedUpdate, apply_unit_orders.in_set(SimSet::Orders));
    }
}

/// Début du pas : les ordres en attente deviennent ceux du pas.
fn collect_commands(mut queue: ResMut<CommandQueue>, mut tick: ResMut<TickCommands>) {
    // `std::mem::take` prend le contenu et laisse une liste vide à sa place.
    tick.0 = std::mem::take(&mut queue.0);
}

/// Les entités de `units` qui existent et appartiennent à `player` (d'après
/// la fonction `owner`), sans doublon, triées par identifiant (ordre reproductible).
pub(crate) fn owned_units(units: &[Entity], player: PlayerId, owner: impl Fn(Entity) -> Option<Owner>) -> Vec<Entity> {
    let mut list: Vec<Entity> = units.iter().copied().filter(|&e| owner(e) == Some(Owner(player))).collect();
    list.sort_unstable();
    list.dedup();
    list
}

/// Applique les ordres de déplacement.
///
/// Un ordre donné à la main remplace ce que faisait l'unité : elle quitte son
/// chantier ([`ConstructionTask`]).
pub(crate) fn apply_unit_orders(
    mut commands: Commands,
    tick: Res<TickCommands>,
    mut groups: ResMut<MoveGroupCounter>,
    units: Query<(&Owner, &Selectable), With<MoveSpeed>>,
) {
    for issued in &tick.0 {
        let PlayerCommand::Move { units: ordered, target } = &issued.command else { continue };
        let movers = owned_units(ordered, issued.player, |e| units.get(e).ok().map(|(o, _)| *o));
        if movers.is_empty() {
            continue;
        }
        let radii: Vec<f32> = movers.iter().filter_map(|&e| units.get(e).ok()).map(|(_, s)| s.radius).collect();
        // Un numéro de groupe commun : les unités de l'ordre partagent leur
        // recherche de chemin (voir `navigation`).
        let group = groups.next_group();
        for (&entity, slot) in movers.iter().zip(formation(&radii, *target)) {
            commands
                .entity(entity)
                .insert((MoveTarget(slot), group))
                .remove::<ConstructionTask>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::{SimPosition, SimulationPlugin};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins((SimulationPlugin, OrdersPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 32.0)));
        app.update();
        app
    }

    fn unit(app: &mut App, player: u8) -> Entity {
        app.world_mut()
            .spawn((
                Owner(PlayerId(player)),
                SimPosition::new(Vec2::ZERO),
                MoveSpeed(4.0),
                Selectable { radius: 0.5, half_extents: None },
            ))
            .id()
    }

    #[test]
    fn orders_wait_for_the_next_tick_and_check_the_owner() {
        let mut app = app();
        let mine = unit(&mut app, 0);
        let theirs = unit(&mut app, 1);
        app.world_mut()
            .resource_mut::<CommandQueue>()
            .push(PlayerId(0), PlayerCommand::Move { units: vec![mine, theirs], target: Vec2::new(10.0, 0.0) });
        // Rien n'est appliqué tant que le pas suivant n'a pas eu lieu.
        assert!(app.world().get::<MoveTarget>(mine).is_none());
        app.update();
        assert!(app.world().get::<MoveTarget>(mine).is_some(), "mon unité a reçu l'ordre");
        assert!(app.world().get::<MoveTarget>(theirs).is_none(), "l'unité d'un autre joueur l'ignore");
        assert!(app.world().resource::<CommandQueue>().is_empty());
    }
}
