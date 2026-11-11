//! Outils de développement : capture d'écran automatique et mesure des FPS.
//!
//! Utile pour vérifier le rendu sans intervention : le jeu attend quelques
//! secondes, enregistre une capture de sa propre image, affiche la moyenne
//! d'images par seconde (FPS) mesurée pendant l'attente, puis se ferme.
//!
//! La caméra dessine alors dans une image en mémoire plutôt que dans la
//! fenêtre : macOS n'affiche pas les fenêtres quand l'écran est verrouillé ou
//! en veille, et une capture de la fenêtre serait toute noire.

use std::path::PathBuf;

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::ui::IsDefaultUiCamera;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};

/// Taille de l'image dans laquelle la caméra dessine en mode capture.
const CAPTURE_SIZE: UVec2 = UVec2::new(2560, 1440);

/// L'image en mémoire qui remplace la fenêtre comme cible de la caméra.
#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

/// Si cette ressource est présente, une capture est prise après `delay_secs`
/// secondes, puis l'application se ferme.
#[derive(Resource, Debug, Clone)]
pub struct AutoScreenshot {
    pub path: PathBuf,
    pub delay_secs: f32,
}

pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_offscreen_target.run_if(resource_exists::<AutoScreenshot>))
            .add_systems(
                Update,
                (render_cameras_offscreen, auto_screenshot)
                    .chain()
                    .run_if(resource_exists::<AutoScreenshot>),
            );
    }
}

fn create_offscreen_target(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut image = Image::new_target_texture(
        CAPTURE_SIZE.x,
        CAPTURE_SIZE.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    );
    // `|=` ajoute un drapeau : la capture doit pouvoir recopier l'image (COPY_SRC).
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    commands.insert_resource(OffscreenTarget(images.add(image)));
}

/// Fait dessiner chaque nouvelle caméra 3D dans l'image en mémoire.
fn render_cameras_offscreen(
    mut commands: Commands,
    target: Res<OffscreenTarget>,
    // La caméra 3D du jeu, ou la caméra 2D du menu de départ.
    cameras: Query<Entity, Or<(Added<Camera3d>, Added<Camera2d>)>>,
) {
    for camera in &cameras {
        // `IsDefaultUiCamera` : l'interface (barres, boutons) est dessinée par
        // défaut par la caméra qui vise la fenêtre ; on la redirige ici aussi.
        commands.entity(camera).insert((RenderTarget::from(target.0.clone()), IsDefaultUiCamera));
    }
}

/// Compteur d'images, pour la moyenne des FPS.
#[derive(Default)]
struct FrameCounter {
    frames: u32,
    started_at: Option<f32>,
    requested: bool,
}

fn auto_screenshot(
    mut commands: Commands,
    settings: Res<AutoScreenshot>,
    time: Res<Time<Real>>,
    target: Res<OffscreenTarget>,
    mut counter: Local<FrameCounter>,
) {
    let now = time.elapsed_secs();
    // On ne compte que les 6 dernières secondes : le démarrage (compilation
    // des shaders, chargement) fausserait la moyenne.
    if now < settings.delay_secs - 6.0 || counter.requested {
        return;
    }
    let started_at = *counter.started_at.get_or_insert(now);
    counter.frames += 1;
    if now < settings.delay_secs {
        return;
    }

    let fps = counter.frames as f32 / (now - started_at).max(1e-3);
    info!("FPS moyens sur {:.1} s : {fps:.1}", now - started_at);
    counter.requested = true;
    let path = settings.path.clone();
    let mut save = save_to_disk(path.clone());
    // Un observateur : une fonction appelée quand l'évènement « capture prête » arrive.
    commands.spawn(Screenshot::image(target.0.clone())).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            save(captured);
            info!("Capture enregistrée : {}", path.display());
            exit.write(AppExit::Success);
        },
    );
}
