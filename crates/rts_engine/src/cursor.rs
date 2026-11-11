//! Capture du pointeur : curseur virtuel dessiné par le jeu.
//!
//! Dans un RTS, on pousse sans arrêt la souris contre les bords de l'écran
//! pour faire défiler la caméra. Sur macOS, cela déclenche les coins actifs,
//! la barre de menus ou le Dock. Pour l'éviter, on verrouille le curseur du
//! système (il ne bouge plus et devient invisible) et on déplace à la place
//! un curseur virtuel, à partir des mouvements de la souris, sans jamais le
//! laisser sortir de la fenêtre.
//!
//! Échap rend le curseur au système ; un clic dans la fenêtre le reprend.
//! Sauf quand Échap sert déjà au jeu (annuler la pose d'un bâtiment, fermer
//! un menu de raccourcis : voir [`InputContext::wants_escape`]) : il faut
//! alors un second appui pour libérer la souris.

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::input::InputContext;
use crate::RtsSystems;

/// Position du pointeur utilisée par la couche d'entrées, qu'il soit
/// capturé (curseur virtuel) ou libre (curseur du système).
#[derive(Resource, Default, Debug)]
pub struct Pointer {
    /// En pixels logiques, origine en haut à gauche ; `None` hors de la fenêtre.
    pub position: Option<Vec2>,
    /// Vrai quand le curseur virtuel est actif.
    pub captured: bool,
}

/// Réglages de la capture du pointeur.
#[derive(Resource, Debug)]
pub struct CursorCaptureSettings {
    /// Capturer le pointeur quand la fenêtre a le focus (activé par défaut).
    pub enabled: bool,
    /// Multiplicateur appliqué aux mouvements de la souris.
    pub sensitivity: f32,
}

impl Default for CursorCaptureSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: 1.0,
        }
    }
}

/// Le joueur a appuyé sur Échap : on ne recapture pas tant qu'il n'a pas recliqué.
#[derive(Default)]
struct ReleasedByPlayer(bool);

/// Marqueur : l'image du curseur virtuel.
#[derive(Component)]
struct VirtualCursor;

pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pointer>()
            .init_resource::<CursorCaptureSettings>()
            .add_systems(Startup, spawn_virtual_cursor)
            .add_systems(
                Update,
                (
                    (update_capture, update_pointer).chain().in_set(RtsSystems::Pointer),
                    draw_virtual_cursor.in_set(RtsSystems::Actions),
                ),
            );
    }
}

/// Décide s'il faut capturer ou relâcher le pointeur.
fn update_capture(
    window: Single<(&mut Window, &mut CursorOptions), With<PrimaryWindow>>,
    settings: Res<CursorCaptureSettings>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    context: Res<InputContext>,
    mut pointer: ResMut<Pointer>,
    mut released: Local<ReleasedByPlayer>,
) {
    let (mut window, mut cursor) = window.into_inner();

    if pointer.captured && keys.just_pressed(KeyCode::Escape) && !context.wants_escape() {
        released.0 = true;
    }
    // Un clic dans la fenêtre annule le « relâché par le joueur ».
    if !pointer.captured && mouse.just_pressed(MouseButton::Left) && window.cursor_position().is_some() {
        released.0 = false;
    }

    let want_capture = settings.enabled && window.focused && !released.0;
    if want_capture == pointer.captured {
        return;
    }

    if want_capture {
        // Le curseur virtuel part de là où était le curseur système.
        pointer.position = window
            .cursor_position()
            .or(Some(window.size() / 2.0));
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    } else {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
        // On replace le curseur système là où était le curseur virtuel.
        if window.focused {
            window.set_cursor_position(pointer.position);
        }
    }
    pointer.captured = want_capture;
}

/// Met à jour la position du pointeur pour cette image.
fn update_pointer(
    window: Single<&Window, With<PrimaryWindow>>,
    motion: Res<AccumulatedMouseMotion>,
    settings: Res<CursorCaptureSettings>,
    mut pointer: ResMut<Pointer>,
) {
    pointer.position = if pointer.captured {
        let previous = pointer.position.unwrap_or(window.size() / 2.0);
        // `clamp` : le curseur virtuel ne peut jamais sortir de la fenêtre.
        Some((previous + motion.delta * settings.sensitivity).clamp(Vec2::ZERO, window.size()))
    } else {
        window.cursor_position()
    };
}

fn spawn_virtual_cursor(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.spawn((
        VirtualCursor,
        ImageNode::new(images.add(arrow_image())),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(ARROW_SIZE as f32),
            height: Val::Px(ARROW_SIZE as f32),
            ..default()
        },
        // Toujours dessiné au-dessus du reste de l'interface.
        GlobalZIndex(i32::MAX),
        Visibility::Hidden,
    ));
}

fn draw_virtual_cursor(
    pointer: Res<Pointer>,
    cursor: Single<(&mut Node, &mut Visibility), With<VirtualCursor>>,
) {
    let (mut node, mut visibility) = cursor.into_inner();
    match (pointer.captured, pointer.position) {
        (true, Some(pos)) => {
            // La pointe de la flèche est le coin haut-gauche de l'image.
            node.left = Val::Px(pos.x);
            node.top = Val::Px(pos.y);
            visibility.set_if_neq(Visibility::Inherited);
        }
        _ => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Côté de l'image du curseur, en pixels.
const ARROW_SIZE: u32 = 24;

/// Dessine une flèche blanche bordée de noir, pixel par pixel.
fn arrow_image() -> Image {
    // Sommets de la flèche : la pointe, le bas du bord gauche, l'extrémité droite.
    let corners = [Vec2::new(1.0, 1.0), Vec2::new(1.0, 19.0), Vec2::new(14.0, 14.0)];
    let edges = [(corners[0], corners[1]), (corners[1], corners[2]), (corners[2], corners[0])];

    let mut data = Vec::with_capacity((ARROW_SIZE * ARROW_SIZE * 4) as usize);
    for y in 0..ARROW_SIZE {
        for x in 0..ARROW_SIZE {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            // Distance signée à chaque bord : positive à l'intérieur du triangle.
            let inside = edges
                .iter()
                .map(|&(a, b)| (p - a).perp_dot(b - a) / (b - a).length())
                .fold(f32::INFINITY, f32::min);
            let pixel: [u8; 4] = if inside > 1.5 {
                [255, 255, 255, 255]
            } else if inside > -0.5 {
                [0, 0, 0, 255]
            } else {
                [0, 0, 0, 0]
            };
            data.extend_from_slice(&pixel);
        }
    }

    Image::new(
        Extent3d { width: ARROW_SIZE, height: ARROW_SIZE, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}
