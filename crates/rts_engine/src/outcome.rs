//! Fin de partie : défaite, victoire et écran de fin.
//!
//! Règle (façon SC2) : un joueur est **vaincu** quand il n'a plus aucun
//! bâtiment, chantiers et emplacements promis compris. Seuls comptent les
//! joueurs qui ont eu au moins un bâtiment : une scène sans base adverse ne
//! se termine jamais. Quand il ne reste qu'un joueur en lice, il **gagne**.
//!
//! La règle est appliquée par la simulation (pas fixe, ordre fixe des
//! joueurs) : toutes les machines d'une partie en réseau verront la même
//! fin au même pas. L'écran de fin, lui, n'est que de l'affichage : on peut
//! le fermer pour continuer à regarder la carte, ou quitter le jeu.

use bevy::app::AppExit;
use bevy::prelude::*;

use crate::buildings::ui::{ui_font, BORDER, BUTTON, KEY_TEXT, PANEL, TEXT};
use crate::buildings::{Building, PlannedSite};
use crate::input::{ActionButton, BlocksPointer, GameAction};
use crate::players::{LocalPlayer, Owner, PlayerId, Players};
use crate::simulation::SimSet;
use crate::RtsSystems;

/// Un joueur vient d'être vaincu (pour l'affichage, un message, un son…).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerDefeated(pub PlayerId);

/// L'issue de la partie, une fois décidée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// Le dernier joueur en lice (`None` : plus personne, défaite de tous).
    pub winner: Option<PlayerId>,
    /// Durée de la partie à ce moment, en secondes de simulation.
    pub duration_secs: f32,
}

/// L'issue de la partie : `None` tant qu'elle n'est pas finie.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct GameOutcome(pub Option<Outcome>);

pub struct OutcomePlugin;

impl Plugin for OutcomePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayerDefeated>()
            .init_resource::<GameOutcome>()
            .init_resource::<OutcomeScreen>()
            .add_systems(FixedUpdate, check_defeats.after(SimSet::Combat))
            .add_systems(Update, (handle_outcome_actions, show_outcome_screen).chain().in_set(RtsSystems::Actions));
    }
}

/// Fin du pas : qui n'a plus de bâtiments ? Qui reste en lice ?
fn check_defeats(
    time: Res<Time>,
    mut players: ResMut<Players>,
    mut outcome: ResMut<GameOutcome>,
    mut defeated: MessageWriter<PlayerDefeated>,
    // Instant où un premier joueur a eu une base : début de la partie
    // (le temps passé dans un menu ne compte pas).
    mut started: Local<Option<f32>>,
    buildings: Query<&Owner, With<Building>>,
    plans: Query<&Owner, With<PlannedSite>>,
) {
    let mut owned = vec![0usize; players.0.len()];
    for owner in buildings.iter().chain(plans.iter()) {
        let index = usize::from(owner.0 .0);
        if owned.len() <= index {
            owned.resize(index + 1, 0);
        }
        owned[index] += 1;
    }
    // Dans l'ordre des numéros de joueur (ordre fixe).
    for (index, &count) in owned.iter().enumerate() {
        let player = PlayerId(index as u8);
        let state = players.get_mut(player);
        if count > 0 {
            state.has_built = true;
            started.get_or_insert(time.elapsed_secs());
        } else if state.has_built && !state.defeated {
            state.defeated = true;
            info!("Joueur {index} vaincu : plus aucun bâtiment");
            defeated.write(PlayerDefeated(player));
        }
    }
    if outcome.0.is_some() {
        return;
    }
    let contenders: Vec<PlayerId> =
        (0..players.0.len()).map(|i| PlayerId(i as u8)).filter(|&p| players.get(p).has_built).collect();
    let alive: Vec<PlayerId> = contenders.iter().copied().filter(|&p| !players.get(p).defeated).collect();
    // Il faut avoir été au moins deux à lutter, et qu'il en reste au plus un.
    if contenders.len() >= 2 && alive.len() <= 1 {
        outcome.0 = Some(Outcome { winner: alive.first().copied(), duration_secs: time.elapsed_secs() - started.unwrap_or(0.0) });
    }
}

// ---------------------------------------------------------------------------
// Écran de fin (affichage seulement).
// ---------------------------------------------------------------------------

/// L'écran de fin : déjà créé, fermé par le joueur ?
#[derive(Resource, Debug, Default)]
struct OutcomeScreen {
    spawned: bool,
    dismissed: bool,
}

/// Marqueur : la racine de l'écran de fin.
#[derive(Component)]
struct OutcomeRoot;

/// « Continuer à regarder » ferme l'écran ; « Quitter » ferme le jeu.
fn handle_outcome_actions(mut actions: MessageReader<GameAction>, mut screen: ResMut<OutcomeScreen>, mut exit: MessageWriter<AppExit>) {
    for action in actions.read() {
        match action {
            GameAction::DismissOutcome => screen.dismissed = true,
            GameAction::Quit => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

/// « 12 min 05 s » (ou « 45 s »).
fn format_duration(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u32;
    let (minutes, seconds) = (total / 60, total % 60);
    if minutes == 0 { format!("{seconds} s") } else { format!("{minutes} min {seconds:02} s") }
}

/// Le titre et la phrase de l'écran de fin, vus par le joueur `local`.
fn verdict(outcome: Outcome, local: PlayerId) -> (&'static str, &'static str, Color) {
    if outcome.winner == Some(local) {
        ("VICTOIRE", "Tous les bâtiments adverses sont détruits.", Color::srgb(0.45, 1.0, 0.6))
    } else {
        ("DÉFAITE", "Tous vos bâtiments ont été détruits.", Color::srgb(1.0, 0.35, 0.28))
    }
}

fn show_outcome_screen(
    mut commands: Commands,
    outcome: Res<GameOutcome>,
    local: Res<LocalPlayer>,
    mut screen: ResMut<OutcomeScreen>,
    mut roots: Query<&mut Visibility, With<OutcomeRoot>>,
) {
    let Some(outcome) = outcome.0 else { return };
    if screen.spawned {
        if screen.dismissed {
            for mut visibility in &mut roots {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
        return;
    }
    screen.spawned = true;
    let (title, sentence, color) = verdict(outcome, local.0);
    // Un voile sur tout l'écran (il arrête les clics), et au centre le panneau.
    commands
        .spawn((
            Name::new("Écran de fin"),
            OutcomeRoot,
            BlocksPointer,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            // Au-dessus de tous les autres panneaux.
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(48.0), Val::Px(28.0)),
                    row_gap: Val::Px(12.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(BORDER),
            ))
            .with_children(|panel| {
                panel.spawn((Text::new(title), ui_font(56.0), TextColor(color)));
                panel.spawn((Text::new(sentence), ui_font(18.0), TextColor(TEXT)));
                panel.spawn((Text::new(format!("Durée de la partie : {}", format_duration(outcome.duration_secs))), ui_font(15.0), TextColor(KEY_TEXT)));
                panel
                    .spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(10.0)), ..default() })
                    .with_children(|row| {
                        for (label, action) in [("Continuer à regarder (Entrée)", GameAction::DismissOutcome), ("Quitter", GameAction::Quit)] {
                            row.spawn((
                                ActionButton { action, enabled: true, hotkey: None },
                                Node { padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
                                BackgroundColor(BUTTON),
                                BorderColor::all(BORDER),
                            ))
                            .with_children(|button| {
                                button.spawn((Text::new(label), ui_font(16.0), TextColor(TEXT)));
                            });
                        }
                    });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::PlayersPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PlayersPlugin)
            .add_message::<PlayerDefeated>()
            .init_resource::<GameOutcome>()
            .add_systems(FixedUpdate, check_defeats)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 32.0)));
        app
    }

    fn building(app: &mut App, player: u8) -> Entity {
        app.world_mut().spawn((Building { type_index: 0 }, Owner(PlayerId(player)))).id()
    }

    #[test]
    fn last_player_with_buildings_wins() {
        let mut app = app();
        let mine = building(&mut app, 0);
        let theirs = [building(&mut app, 1), building(&mut app, 1)];
        app.update();
        app.update();
        assert_eq!(app.world().resource::<GameOutcome>().0, None);
        // Un seul bâtiment adverse détruit : la partie continue.
        app.world_mut().despawn(theirs[0]);
        app.update();
        assert_eq!(app.world().resource::<GameOutcome>().0, None);
        app.world_mut().despawn(theirs[1]);
        app.update();
        let outcome = app.world().resource::<GameOutcome>().0.expect("la partie aurait dû finir");
        assert_eq!(outcome.winner, Some(PlayerId(0)));
        assert!(app.world().resource::<Players>().get(PlayerId(1)).defeated);
        assert!(app.world().get_entity(mine).is_ok());
    }

    #[test]
    fn a_scene_without_opponent_never_ends() {
        let mut app = app();
        let mine = building(&mut app, 0);
        app.update();
        app.update();
        app.world_mut().despawn(mine);
        app.update();
        // Le joueur est vaincu, mais il n'a jamais eu d'adversaire : pas d'issue.
        assert!(app.world().resource::<Players>().get(PlayerId(0)).defeated);
        assert_eq!(app.world().resource::<GameOutcome>().0, None);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(format_duration(45.2), "45 s");
        assert_eq!(format_duration(725.0), "12 min 05 s");
    }
}
