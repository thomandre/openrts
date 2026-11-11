//! Écran de démarrage : choisir sa faction et celle de l'adversaire, puis
//! lancer la partie.
//!
//! Le jeu a deux **états** (`GameState`) : le menu, puis la partie. Bevy ne
//! lance les systèmes « à l'entrée dans un état » (`OnEnter`) qu'une fois,
//! au moment du changement : c'est là que `main.rs` crée la carte, les bases
//! et les armées, avec les factions choisies ici.
//!
//! Le pointeur est le curseur virtuel du moteur (en plein écran, le curseur
//! du système est bloqué) : les boutons du menu testent donc eux-mêmes la
//! position de [`Pointer`] au clic. Entrée lance aussi la partie.

use bevy::prelude::*;
use bevy::text::{FontSource, GenericFontFamily};
use rts_engine::cursor::Pointer;

use crate::{EnemyFaction, Faction};

/// Les deux temps du jeu.
///
/// `#[derive(States)]` : Bevy sait passer d'une valeur à l'autre
/// (`NextState`) et lancer des systèmes à l'entrée de chacune.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    Menu,
    Playing,
}

/// Les factions proposées, dans l'ordre des boutons.
const CHOICES: [Faction; 3] = [Faction::Garde, Faction::Sylves, Faction::Collectif];

const BACKDROP: Color = Color::srgb(0.02, 0.03, 0.05);
const PANEL: Color = Color::srgba(0.04, 0.06, 0.09, 0.95);
const BORDER: Color = Color::srgb(0.18, 0.55, 0.62);
const TEXT: Color = Color::srgb(0.88, 0.92, 0.95);
const DIM_TEXT: Color = Color::srgb(0.55, 0.6, 0.66);
const ACCENT: Color = Color::srgb(0.31, 0.82, 0.88);

/// Nom, couleur et présentation d'une faction dans le menu.
fn card(faction: Faction) -> (&'static str, Color, &'static str) {
    match faction {
        Faction::Garde => ("La Garde", Color::srgb(0.95, 0.5, 0.18), "Humains en armure : infanterie à distance, blindés, jetpacks. Les Techniciens bâtissent sur place."),
        Faction::Sylves => ("Les Sylves", Color::srgb(0.4, 0.85, 0.45), "Forêt consciente : lente et forte au contact. Les Pousses deviennent les bâtiments."),
        Faction::Collectif | Faction::CollectifOrRose => (
            "Le Collectif",
            Color::srgb(0.89, 0.71, 0.32),
            "Joaillerie mécanique : peu d'unités, chères et solides, beaucoup de volants. Les bâtiments se sertissent à distance.",
        ),
    }
}

/// Nom, couleur et présentation de la carte « Aléatoire ».
const RANDOM_CARD: (&str, Color, &str) = ("Aléatoire", Color::srgb(0.85, 0.85, 0.9), "La faction est tirée au sort au lancement de la partie.");

/// Un bouton du menu et ce qu'il fait. `Option<Faction>` : une faction
/// précise, ou `None` pour « Aléatoire ».
#[derive(Component, Debug, Clone, Copy, PartialEq)]
enum MenuButton {
    /// Choisir sa faction.
    Player(Option<Faction>),
    /// Choisir celle de l'adversaire.
    Enemy(Option<Faction>),
    Start,
}

/// Ce qui est choisi dans le menu (`None` : aléatoire).
#[derive(Resource, Debug, Clone, Copy)]
struct MenuChoice {
    player: Option<Faction>,
    enemy: Option<Faction>,
}

/// Une faction au hasard parmi celles du menu. Le hasard vient de l'horloge
/// (les nanosecondes de l'heure) : il ne touche pas à la simulation, qui
/// reste déterministe une fois la partie lancée. `salt` : un tirage différent
/// pour le joueur et pour l'adversaire.
fn random_faction(salt: u32) -> Faction {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    pick(nanos, salt)
}

/// Le tirage lui-même, à partir d'un nombre quelconque (`nanos`).
fn pick(nanos: u32, salt: u32) -> Faction {
    // Un mélange de bits (multiplication par un grand nombre impair) pour que
    // deux tirages proches ne donnent pas le même résultat.
    let mixed = (nanos ^ salt.wrapping_mul(0x9E37_79B9)).wrapping_mul(0x85EB_CA6B) >> 16;
    CHOICES[mixed as usize % CHOICES.len()]
}

/// Marqueur : la racine du menu (retirée au lancement).
#[derive(Component)]
struct MenuRoot;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Menu), spawn_menu)
            .add_systems(Update, (handle_menu, highlight_choices).chain().run_if(in_state(GameState::Menu)))
            .add_systems(OnExit(GameState::Menu), despawn_menu);
    }
}

fn spawn_menu(mut commands: Commands, faction: Res<Faction>, enemy: Res<EnemyFaction>) {
    commands.insert_resource(MenuChoice { player: Some(*faction), enemy: Some(enemy.0) });
    // Une caméra 2D, le temps du menu : l'interface s'affiche par une caméra
    // (celle du jeu n'existe pas encore).
    commands.spawn((Name::new("Caméra du menu"), MenuRoot, Camera2d));
    // Police sans empattement du système : elle a les lettres accentuées.
    let font = |size: f32| TextFont { font: FontSource::Generic(GenericFontFamily::SansSerif), ..TextFont::from_font_size(size) };
    commands
        .spawn((
            Name::new("Menu"),
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(22.0),
                ..default()
            },
            BackgroundColor(BACKDROP),
            // Au-dessus de toute l'interface du jeu (minicarte, compteurs…).
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            root.spawn((Text::new("openRTS"), font(64.0), TextColor(TEXT)));
            for (title, player) in [("Ta faction", true), ("L'adversaire (IA)", false)] {
                root.spawn((Text::new(title), font(20.0), TextColor(ACCENT)));
                root.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(14.0), ..default() }).with_children(|row| {
                    // Les trois factions, puis « Aléatoire ».
                    for faction in CHOICES.map(Some).into_iter().chain([None]) {
                        let (name, color, blurb) = faction.map_or(RANDOM_CARD, card);
                        let button = if player { MenuButton::Player(faction) } else { MenuButton::Enemy(faction) };
                        row.spawn((
                            button,
                            Node {
                                width: Val::Px(220.0),
                                flex_direction: FlexDirection::Column,
                                padding: UiRect::all(Val::Px(14.0)),
                                row_gap: Val::Px(6.0),
                                border: UiRect::all(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(PANEL),
                            BorderColor::all(BORDER),
                        ))
                        .with_children(|card| {
                            card.spawn((Text::new(name), font(22.0), TextColor(color)));
                            card.spawn((Text::new(blurb), font(13.0), TextColor(DIM_TEXT)));
                        });
                    }
                });
            }
            root.spawn((
                MenuButton::Start,
                Node { padding: UiRect::axes(Val::Px(36.0), Val::Px(14.0)), border: UiRect::all(Val::Px(2.0)), margin: UiRect::top(Val::Px(12.0)), ..default() },
                BackgroundColor(PANEL),
                BorderColor::all(ACCENT),
            ))
            .with_children(|button| {
                button.spawn((Text::new("Lancer la partie (Entrée)"), font(22.0), TextColor(TEXT)));
            });
        });
}

/// Clics (au pointeur virtuel) et Entrée.
fn handle_menu(
    pointer: Res<Pointer>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<(&MenuButton, &ComputedNode, &UiGlobalTransform)>,
    mut choice: ResMut<MenuChoice>,
    mut faction: ResMut<Faction>,
    mut enemy: ResMut<EnemyFaction>,
    mut next: ResMut<NextState<GameState>>,
    time: Res<Time<Real>>,
) {
    let mut start = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    // Outil de développement : `OPENRTS_MENU=start` lance la partie au bout
    // de 2 s, comme un clic sur « Lancer » (pour tester le passage en jeu).
    if std::env::var("OPENRTS_MENU").as_deref() == Ok("start") && time.elapsed_secs() > 2.0 {
        start = true;
    }
    if mouse.just_pressed(MouseButton::Left)
        && let Some(pos) = pointer.position
    {
        // Les nœuds sont mesurés en pixels physiques, le pointeur en pixels
        // logiques (voir la couche d'entrées du moteur).
        let hit = buttons.iter().find(|(_, node, transform)| node.contains_point(**transform, pos / node.inverse_scale_factor()));
        match hit.map(|(button, ..)| *button) {
            Some(MenuButton::Player(picked)) => choice.player = picked,
            Some(MenuButton::Enemy(picked)) => choice.enemy = picked,
            Some(MenuButton::Start) => start = true,
            None => {}
        }
    }
    if start {
        // « Aléatoire » : le tirage a lieu maintenant, au lancement.
        *faction = choice.player.unwrap_or_else(|| random_faction(1));
        enemy.0 = choice.enemy.unwrap_or_else(|| random_faction(2));
        info!("Partie : {:?} contre {:?}", *faction, enemy.0);
        next.set(GameState::Playing);
    }
}

/// Le cadre des factions choisies s'allume à leur couleur.
fn highlight_choices(choice: Option<Res<MenuChoice>>, mut buttons: Query<(&MenuButton, &mut BorderColor, &mut BackgroundColor)>) {
    let Some(choice) = choice else { return };
    for (button, mut border, mut background) in &mut buttons {
        let chosen = match *button {
            MenuButton::Player(f) => Some((f == choice.player, f)),
            MenuButton::Enemy(f) => Some((f == choice.enemy, f)),
            MenuButton::Start => None,
        };
        let Some((selected, f)) = chosen else { continue };
        let color = if selected { f.map_or(RANDOM_CARD, card).1 } else { BORDER.with_alpha(0.35) };
        border.set_if_neq(BorderColor::all(color));
        background.set_if_neq(BackgroundColor(if selected { Color::srgba(0.08, 0.12, 0.16, 0.98) } else { PANEL }));
    }
}

fn despawn_menu(mut commands: Commands, roots: Query<Entity, With<MenuRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draw_can_give_every_faction() {
        for salt in [1, 2] {
            let drawn: Vec<Faction> = (0..1000u32).map(|n| pick(n * 7919, salt)).collect();
            for faction in CHOICES {
                assert!(drawn.contains(&faction), "{faction:?} n'est jamais tirée");
            }
        }
    }
}
