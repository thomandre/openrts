//! Caméra RTS : perspective inclinée, défilement et zoom.
//!
//! La caméra regarde un point du sol (`focus`) depuis une certaine distance,
//! avec un angle fixe. Défiler déplace le point visé ; zoomer change la
//! distance. On recalcule la `Transform` à partir de ces trois valeurs.

use bevy::prelude::*;

use crate::input::{CameraControl, GameAction};
use crate::RtsSystems;

/// Composant à placer sur l'entité caméra pour la piloter comme dans un RTS.
#[derive(Component, Debug, Clone)]
pub struct RtsCamera {
    /// Point du sol au centre de l'écran.
    pub focus: Vec3,
    /// Distance entre la caméra et `focus`.
    pub distance: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    /// Inclinaison sous l'horizontale, en radians (90° = vue de dessus).
    pub pitch: f32,
    /// Vitesse de défilement, en mètres par seconde pour une distance de 1.
    /// Multipliée par la distance : on défile plus vite quand on est loin.
    pub pan_speed: f32,
    /// Fraction de distance gagnée par cran de molette.
    pub zoom_step: f32,
    /// Zone (x, z) dans laquelle `focus` doit rester.
    pub bounds: Rect,
}

impl Default for RtsCamera {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            distance: 30.0,
            min_distance: 10.0,
            max_distance: 70.0,
            pitch: 56f32.to_radians(),
            pan_speed: 1.2,
            zoom_step: 0.12,
            bounds: Rect::new(-50.0, -50.0, 50.0, 50.0),
        }
    }
}

impl RtsCamera {
    /// Position et orientation de la caméra qui correspondent à ces réglages.
    pub fn transform(&self) -> Transform {
        // La caméra recule vers +Z et monte : le « haut » de l'écran regarde vers -Z.
        let offset = Vec3::new(0.0, self.pitch.sin(), self.pitch.cos()) * self.distance;
        Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y)
    }

    /// Vise ce point du sol (x, z), sans sortir de `bounds`. La `Transform`
    /// suit à la prochaine mise à jour de la caméra.
    pub fn look_at(&mut self, point: Vec2) {
        let point = point.clamp(self.bounds.min, self.bounds.max);
        self.focus = Vec3::new(point.x, 0.0, point.y);
    }
}

/// Le rayon qui part de la caméra et passe par ce point de l'écran.
pub fn screen_ray(camera: &Camera, camera_transform: &GlobalTransform, screen_pos: Vec2) -> Option<Ray3d> {
    // `.ok()` transforme un `Result` (succès ou erreur) en `Option` (valeur ou rien).
    camera.viewport_to_world(camera_transform, screen_pos).ok()
}

/// Point où le rayon traverse le plan horizontal y = `height`.
pub fn ray_at_height(ray: Ray3d, height: f32) -> Option<Vec3> {
    // `?` : si l'intersection n'existe pas (`None`), la fonction renvoie `None` tout de suite.
    let distance = ray.intersect_plane(Vec3::Y * height, InfinitePlane3d::new(Vec3::Y))?;
    Some(ray.get_point(distance))
}

/// Point du sol (y = 0) qui se trouve sous ce point de l'écran.
pub fn screen_to_ground(camera: &Camera, camera_transform: &GlobalTransform, screen_pos: Vec2) -> Option<Vec3> {
    ray_at_height(screen_ray(camera, camera_transform, screen_pos)?, 0.0)
}

pub struct RtsCameraPlugin;

impl Plugin for RtsCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (handle_look_at, apply_camera_control).chain().in_set(RtsSystems::Actions));
    }
}

/// « Regarder ce point » (minicarte) : la caméra y saute.
fn handle_look_at(mut actions: MessageReader<GameAction>, mut cameras: Query<&mut RtsCamera>) {
    for action in actions.read() {
        if let GameAction::LookAt { point } = *action {
            for mut cam in &mut cameras {
                cam.look_at(point);
            }
        }
    }
}

fn apply_camera_control(
    control: Res<CameraControl>,
    time: Res<Time>,
    mut cameras: Query<(&mut RtsCamera, &mut Transform)>,
) {
    // `iter_mut` parcourt toutes les entités qui ont *à la fois* RtsCamera et Transform.
    for (mut cam, mut transform) in &mut cameras {
        // Zoom multiplicatif : chaque cran rapproche d'un même pourcentage.
        let factor = (1.0 - cam.zoom_step).powf(control.zoom);
        cam.distance = (cam.distance * factor).clamp(cam.min_distance, cam.max_distance);

        // Défilement : x de l'écran → x du monde, haut de l'écran → -z du monde.
        let speed = cam.pan_speed * cam.distance * time.delta_secs();
        let delta = Vec3::new(control.pan.x, 0.0, -control.pan.y) * speed;
        let bounds = cam.bounds;
        cam.focus = (cam.focus + delta).clamp(
            Vec3::new(bounds.min.x, 0.0, bounds.min.y),
            Vec3::new(bounds.max.x, 0.0, bounds.max.y),
        );

        *transform = cam.transform();
    }
}
