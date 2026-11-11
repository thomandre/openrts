//! Couche d'abstraction des entrées.
//!
//! Le reste du moteur ne lit jamais la souris ni le clavier directement : il
//! ne connaît que des [`GameAction`] (sélectionner, ordonner…) et l'état
//! [`CameraControl`]. Chaque périphérique a son « backend » qui produit ces
//! actions. Ici : souris + clavier. Le tactile (iPad) ajoutera son propre
//! backend sans toucher au reste.

use bevy::input::keyboard::Key;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;

use crate::camera::screen_to_ground;
use crate::control_groups::GROUP_COUNT;
use crate::cursor::Pointer;
use crate::minimap::MinimapSurface;
use crate::RtsSystems;

/// Une action de joueur, indépendante du périphérique.
///
/// Les positions sont en pixels dans la fenêtre (origine en haut à gauche) :
/// un clic de souris et un tap au doigt produisent exactement la même chose.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub enum GameAction {
    /// Sélectionner ce qui se trouve sous ce point.
    SelectAt { screen_pos: Vec2, additive: bool },
    /// Sélectionner tout ce qui est dans ce rectangle.
    SelectBox { corner_a: Vec2, corner_b: Vec2, additive: bool },
    /// Ordre contextuel au point visé (pour l'instant : se déplacer).
    CommandAt { screen_pos: Vec2 },
    /// Outil de test : faire apparaître beaucoup d'unités d'un coup.
    SpawnStressTest,
    /// Choisir un bâtiment à poser (indice dans le catalogue des bâtiments) :
    /// ouvre le mode pose, si un ouvrier est sélectionné.
    ChooseBuilding { type_index: usize },
    /// Poser le bâtiment choisi au point visé. `keep_placing` : rester en
    /// mode pose pour en poser un autre (Maj + clic à la souris).
    PlaceBuildingAt { screen_pos: Vec2, keep_placing: bool },
    /// Quitter le mode pose sans rien poser.
    CancelPlacement,
    /// Produire une unité de ce type (indice du catalogue d'unités) dans un
    /// des bâtiments producteurs sélectionnés (celui dont la file est la plus courte).
    Train { unit_type: usize },
    /// Annuler l'élément `slot` de la file du bâtiment producteur affiché.
    CancelQueued { slot: usize },
    /// Annuler les chantiers sélectionnés (remboursés).
    CancelConstruction,
    /// Préparer un ordre qui vise un point (touche A ou M, ou bouton de la
    /// barre d'ordres) : le prochain clic, ou tap, le donne.
    ChooseOrder { order: TargetedOrder },
    /// Donner l'ordre préparé au point visé (sur un ennemi : l'attaquer).
    /// `keep` : rester en mode visée pour en donner un autre (Maj + clic).
    OrderAt { order: TargetedOrder, screen_pos: Vec2, keep: bool },
    /// Quitter le mode visée sans rien ordonner.
    CancelOrder,
    /// Tout arrêter (touche S ou bouton).
    Stop,
    /// Tenir la position (touche H ou bouton).
    HoldPosition,
    /// Centrer la caméra sur ce point du sol (x, z) : clic ou glissé sur la minicarte.
    LookAt { point: Vec2 },
    /// Ordre contextuel vers ce point du sol (x, z), sans rien viser dessus :
    /// clic droit sur la minicarte. Même effet qu'un clic droit au sol.
    CommandAtPoint { point: Vec2 },
    /// Donner l'ordre préparé (A ou M) vers ce point du sol : clic sur la
    /// minicarte en mode visée. `keep` : rester en mode visée.
    OrderAtPoint { order: TargetedOrder, point: Vec2, keep: bool },
    /// Mémoriser la sélection dans le groupe de contrôle `group` (0 à 9 ;
    /// Ctrl ou Cmd + chiffre). Voir le module `control_groups`.
    AssignGroup { group: usize },
    /// Ajouter la sélection au groupe `group` (Maj + chiffre).
    AddToGroup { group: usize },
    /// Rappeler le groupe `group` (chiffre seul, ou bouton de la barre de
    /// groupes) ; deux fois de suite rapidement : centrer la caméra dessus.
    RecallGroup { group: usize },
}

/// Un ordre qui attend qu'on vise un point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetedOrder {
    /// Attaque-déplacement : aller au point en combattant ce qu'on croise.
    AttackMove,
    /// Déplacement simple : aller au point en ignorant les ennemis (retraite).
    Move,
}

/// Raccourci clavier d'un [`ActionButton`] : une touche, éventuellement
/// précédée d'une touche de menu (« B puis Q », comme une grille de
/// raccourcis). Ce sont des caractères tapés (pas des positions de touches) :
/// le raccourci affiché sur le bouton est celui inscrit sur le clavier, en
/// AZERTY comme en QWERTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub menu: Option<char>,
    pub key: char,
}

/// Un bouton d'interface qui déclenche une action : un clic, un tap ou son
/// raccourci clavier produisent tous la même [`GameAction`]. Le moteur sait
/// ainsi gérer les boutons sans savoir d'où vient l'entrée.
///
/// Seuls les boutons visibles réagissent ; un bouton désactivé (`enabled`
/// faux, par exemple un bâtiment dont il manque un prérequis) avale le clic
/// sans rien faire.
#[derive(Component, Debug, Clone)]
pub struct ActionButton {
    pub action: GameAction,
    pub enabled: bool,
    pub hotkey: Option<Hotkey>,
}

/// Marqueur : un panneau d'interface qui arrête les clics (ils ne
/// sélectionnent pas les unités qui se trouvent dessous).
#[derive(Component, Debug, Default)]
pub struct BlocksPointer;

/// État des entrées partagé entre les backends et le reste du moteur.
#[derive(Resource, Debug, Default)]
pub struct InputContext {
    /// Mode pose de bâtiment actif (tenu à jour par le module `buildings`) :
    /// les clics posent ou annulent au lieu de sélectionner et d'ordonner.
    pub placing: bool,
    /// Touche de menu enfoncée juste avant (« B » de « B puis Q »), en attente
    /// de la seconde touche.
    pub key_menu: Option<char>,
    /// Ordre en attente de visée (tenu à jour par le module `combat`) : le
    /// clic gauche le donne, le clic droit l'annule.
    pub targeting: Option<TargetedOrder>,
}

impl InputContext {
    /// Échap a-t-il un rôle dans le jeu en ce moment (annuler la pose, fermer
    /// un menu de raccourcis) ? Sinon, il rend la souris au système (voir `cursor`).
    pub fn wants_escape(&self) -> bool {
        self.placing || self.key_menu.is_some() || self.targeting.is_some()
    }
}

/// Ce que le joueur demande à la caméra pendant cette image.
/// Remis à zéro au début de chaque image, puis rempli par les backends.
#[derive(Resource, Default, Debug)]
pub struct CameraControl {
    /// Direction de défilement, chaque axe entre -1 et 1 (x : droite, y : haut de l'écran).
    pub pan: Vec2,
    /// Zoom demandé : positif pour se rapprocher.
    pub zoom: f32,
}

/// Rectangle de sélection en cours de tracé, pour que l'interface l'affiche.
#[derive(Resource, Default, Debug)]
pub struct SelectionDrag(pub Option<(Vec2, Vec2)>);

/// Réglages du backend souris/clavier.
#[derive(Resource, Debug)]
pub struct MouseKeyboardSettings {
    /// Distance au bord de la fenêtre (en pixels) qui déclenche le défilement.
    pub edge_scroll_margin: f32,
    /// Déplacement minimal (en pixels) pour qu'un clic devienne un rectangle.
    pub drag_threshold: f32,
}

impl Default for MouseKeyboardSettings {
    fn default() -> Self {
        Self {
            edge_scroll_margin: 8.0,
            drag_threshold: 6.0,
        }
    }
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<GameAction>()
            .init_resource::<InputContext>()
            .init_resource::<CameraControl>()
            .init_resource::<SelectionDrag>()
            .init_resource::<MouseKeyboardSettings>()
            .add_systems(
                Update,
                (reset_camera_control, mouse_keyboard_backend)
                    .chain()
                    .in_set(RtsSystems::Input),
            );
    }
}

fn reset_camera_control(mut control: ResMut<CameraControl>) {
    *control = CameraControl::default();
}

/// Point de départ d'un clic gauche en cours (état propre à ce backend).
///
/// On mémorise le point *du sol* sous le curseur, pas sa position à l'écran :
/// si la caméra défile pendant qu'on trace le rectangle, le coin de départ
/// reste accroché aux mêmes unités, comme dans tout RTS.
#[derive(Default)]
struct LeftPress {
    start_screen: Option<Vec2>,
    start_ground: Option<Vec3>,
    /// Le clic a commencé sur la minicarte : tant qu'il est tenu, glisser
    /// déplace la caméra (même si le pointeur sort de la minicarte).
    on_minimap: bool,
}

/// Les touches des groupes de contrôle, dans l'ordre des groupes : 1 à 9, puis 0.
///
/// Ce sont des **positions** de touches (`KeyCode`), pas des caractères tapés
/// comme les autres raccourcis : sur un clavier AZERTY, la rangée du haut
/// tape « & é " ' ( … » sans Maj, mais ce sont bien les touches 1 à 0.
const GROUP_KEYS: [KeyCode; GROUP_COUNT] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
    KeyCode::Digit0,
];

/// Backend souris + clavier : lit le matériel et produit des actions.
fn mouse_keyboard_backend(
    window: Single<&Window, With<PrimaryWindow>>,
    pointer: Res<Pointer>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    typed: Res<ButtonInput<Key>>,
    mut wheel: MessageReader<MouseWheel>,
    settings: Res<MouseKeyboardSettings>,
    mut control: ResMut<CameraControl>,
    mut drag: ResMut<SelectionDrag>,
    mut context: ResMut<InputContext>,
    mut actions: MessageWriter<GameAction>,
    buttons: Query<(&ComputedNode, &UiGlobalTransform, &InheritedVisibility, Option<&ActionButton>), Or<(With<ActionButton>, With<BlocksPointer>)>>,
    minimap: Query<(&ComputedNode, &UiGlobalTransform, &InheritedVisibility, &MinimapSurface)>,
    // `Local` : une variable qui survit d'une image à l'autre, privée à ce système.
    mut press: Local<LeftPress>,
) {
    let cursor = pointer.position;
    let (camera, camera_transform) = camera.into_inner();
    let additive = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    // --- Interface : le pointeur est-il sur un bouton ou un panneau ? ---
    // Les nœuds d'interface sont mesurés en pixels physiques, le pointeur en
    // pixels logiques (un écran Retina a 2 pixels physiques par pixel logique).
    let ui_hit = cursor.and_then(|pos| {
        buttons
            .iter()
            .filter(|(node, transform, visible, _)| {
                visible.get() && node.contains_point(**transform, pos / node.inverse_scale_factor())
            })
            // Un bouton l'emporte sur le panneau qui le contient.
            .max_by_key(|(.., button)| button.is_some())
            .map(|(.., button)| button.cloned())
    });

    // --- Raccourcis clavier des boutons (« B puis Q ») ---
    for key in typed.get_just_pressed() {
        let Key::Character(text) = key else { continue };
        // Minuscule : Maj enfoncée ne change pas le raccourci.
        let Some(c) = text.chars().next().map(|c| c.to_ascii_lowercase()) else { continue };
        let visible = buttons.iter().filter(|(_, _, v, _)| v.get()).filter_map(|(.., b)| b);
        let mut opens_menu = false;
        let mut pressed = None;
        for button in visible {
            let Some(hotkey) = button.hotkey else { continue };
            if context.key_menu.is_none() && hotkey.menu == Some(c) {
                opens_menu = true;
            } else if hotkey.menu == context.key_menu && hotkey.key == c {
                pressed = Some(button);
            }
        }
        context.key_menu = None;
        if let Some(button) = pressed {
            if button.enabled {
                actions.write(button.action);
            }
        } else if opens_menu {
            context.key_menu = Some(c);
        }
    }

    // --- Groupes de contrôle : Ctrl (ou Cmd) + chiffre mémorise, Maj +
    // chiffre ajoute, le chiffre seul rappelle ---
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]);
    for (group, key) in GROUP_KEYS.into_iter().enumerate() {
        if keys.just_pressed(key) {
            actions.write(if ctrl {
                GameAction::AssignGroup { group }
            } else if additive {
                GameAction::AddToGroup { group }
            } else {
                GameAction::RecallGroup { group }
            });
        }
    }

    // --- Outil de test de charge ---
    if keys.just_pressed(KeyCode::F3) {
        actions.write(GameAction::SpawnStressTest);
    }

    // --- Échap : annule la pose ou ferme le menu de raccourcis ---
    // (sinon, il rend la souris au système : voir `cursor`).
    if keys.just_pressed(KeyCode::Escape) {
        if context.placing {
            actions.write(GameAction::CancelPlacement);
        }
        if context.targeting.is_some() {
            actions.write(GameAction::CancelOrder);
        }
        context.key_menu = None;
    }

    // --- Caméra : flèches du clavier ---
    let mut pan = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowLeft) {
        pan.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        pan.x += 1.0;
    }
    if keys.pressed(KeyCode::ArrowUp) {
        pan.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        pan.y -= 1.0;
    }

    // --- Caméra : bords de l'écran (seulement si le curseur est dans la fenêtre) ---
    if let Some(pos) = cursor {
        let margin = settings.edge_scroll_margin;
        let size = window.size();
        if pos.x <= margin {
            pan.x -= 1.0;
        }
        if pos.x >= size.x - margin {
            pan.x += 1.0;
        }
        if pos.y <= margin {
            pan.y += 1.0;
        }
        if pos.y >= size.y - margin {
            pan.y -= 1.0;
        }
    }
    control.pan = pan.clamp(Vec2::NEG_ONE, Vec2::ONE);

    // --- Caméra : molette ---
    for event in wheel.read() {
        control.zoom += match event.unit {
            MouseScrollUnit::Line => event.y,
            // Trackpads : défilement en pixels, bien plus fin qu'un cran de molette.
            MouseScrollUnit::Pixel => event.y / 40.0,
        };
    }

    // --- Minicarte : le point du sol sous le pointeur ---
    // `normalize_point` donne la position dans le nœud, de -0,5 à 0,5 sur
    // chaque axe ; `clamp_to_map` : un glissé qui sort de la minicarte vise
    // son bord.
    let on_minimap = |pos: Vec2, clamp_to_map: bool| {
        minimap.iter().find_map(|(node, transform, visible, surface)| {
            let local = node.normalize_point(*transform, pos / node.inverse_scale_factor())? + 0.5;
            let inside = local.cmpge(Vec2::ZERO).all() && local.cmple(Vec2::ONE).all();
            (visible.get() && (inside || clamp_to_map)).then(|| surface.to_world(local))
        })
    };
    if press.on_minimap {
        if mouse.pressed(MouseButton::Left)
            && let Some(point) = cursor.and_then(|pos| on_minimap(pos, true))
        {
            // La minicarte mène la caméra : pas de défilement par les bords en même temps.
            control.pan = Vec2::ZERO;
            actions.write(GameAction::LookAt { point });
        } else {
            press.on_minimap = false;
        }
        return;
    }
    if let Some(point) = cursor.and_then(|pos| on_minimap(pos, false)) {
        if mouse.just_pressed(MouseButton::Left) {
            if let Some(order) = context.targeting {
                // En mode visée, le clic sur la minicarte donne l'ordre à ce point.
                actions.write(GameAction::OrderAtPoint { order, point, keep: additive });
            } else {
                *press = LeftPress { on_minimap: true, ..default() };
                drag.0 = None;
                actions.write(GameAction::LookAt { point });
            }
        }
        if mouse.just_pressed(MouseButton::Right) {
            // Comme au sol : le clic droit annule la pose ou la visée, sinon il ordonne.
            if context.placing {
                actions.write(GameAction::CancelPlacement);
            } else if context.targeting.is_some() {
                actions.write(GameAction::CancelOrder);
            } else {
                actions.write(GameAction::CommandAtPoint { point });
            }
        }
        if mouse.any_just_pressed([MouseButton::Left, MouseButton::Right]) {
            return;
        }
    }

    // --- Clic sur l'interface : le bouton agit, rien d'autre ---
    if let Some(button) = &ui_hit {
        if mouse.just_pressed(MouseButton::Left)
            && let Some(button) = button
            && button.enabled
        {
            actions.write(button.action);
        }
        if mouse.any_just_pressed([MouseButton::Left, MouseButton::Right]) {
            return;
        }
    }

    // --- Mode pose : clic gauche pose, clic droit annule ---
    if context.placing {
        *press = LeftPress::default();
        drag.0 = None;
        if let Some(pos) = cursor
            && mouse.just_pressed(MouseButton::Left)
        {
            actions.write(GameAction::PlaceBuildingAt { screen_pos: pos, keep_placing: additive });
        }
        if mouse.just_pressed(MouseButton::Right) {
            actions.write(GameAction::CancelPlacement);
        }
        return;
    }

    // --- Mode visée (attaque-déplacement…) : clic gauche ordonne, clic droit annule ---
    if let Some(order) = context.targeting {
        *press = LeftPress::default();
        drag.0 = None;
        if let Some(pos) = cursor
            && mouse.just_pressed(MouseButton::Left)
        {
            actions.write(GameAction::OrderAt { order, screen_pos: pos, keep: additive });
        }
        if mouse.just_pressed(MouseButton::Right) {
            actions.write(GameAction::CancelOrder);
        }
        return;
    }

    // --- Sélection : clic gauche ou rectangle ---
    if mouse.just_pressed(MouseButton::Left) {
        press.start_screen = cursor;
        press.start_ground = cursor.and_then(|pos| screen_to_ground(camera, camera_transform, pos));
    }
    // Où se trouve aujourd'hui, à l'écran, le point du sol cliqué au départ.
    let start = press
        .start_ground
        .and_then(|ground| camera.world_to_viewport(camera_transform, ground).ok())
        .or(press.start_screen);
    if let (Some(start), Some(pos)) = (start, cursor) {
        let dragging = start.distance(pos) > settings.drag_threshold;
        drag.0 = dragging.then_some((start, pos));

        if mouse.just_released(MouseButton::Left) {
            actions.write(if dragging {
                GameAction::SelectBox { corner_a: start, corner_b: pos, additive }
            } else {
                GameAction::SelectAt { screen_pos: pos, additive }
            });
        }
    }
    if !mouse.pressed(MouseButton::Left) {
        *press = LeftPress::default();
        drag.0 = None;
    }

    // --- Ordre contextuel : clic droit ---
    if mouse.just_pressed(MouseButton::Right)
        && let Some(pos) = cursor
    {
        actions.write(GameAction::CommandAt { screen_pos: pos });
    }
}
