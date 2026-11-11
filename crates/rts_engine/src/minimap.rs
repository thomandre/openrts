//! Minicarte, en bas à droite de l'écran.
//!
//! Elle montre le sol de la carte ([`MapBounds`]), les gisements, les
//! bâtiments (rectangles) et les unités (points) aux couleurs d'équipe, et
//! le trapèze de ce que voit la caméra. Le nord de la carte (-z) est en
//! haut, comme dans la vue 3D.
//!
//! - Clic gauche (ou glissé) : la caméra y saute (`GameAction::LookAt`).
//! - Clic droit : ordre contextuel vers ce point, comme un clic droit au sol
//!   (`GameAction::CommandAtPoint`) ; en mode visée (A ou M), le clic gauche
//!   donne l'ordre préparé (`GameAction::OrderAtPoint`).
//!
//! La couche d'entrées traduit le pointeur en point du sol grâce à
//! [`MinimapSurface`] ; ce module ne fait que dessiner.
//!
//! **Performances** : la carte est une petite image (un pixel d'image par
//! pixel d'écran) redessinée par le processeur dix fois par seconde
//! seulement, en recopiant un fond préparé une fois pour toutes. Même à
//! 300 contre 300, c'est quelques milliers de pixels écrits. Le trapèze de
//! la caméra, lui, suit à chaque image : ce sont quatre traits d'interface
//! déplacés, sans redessiner l'image.
//!
//! **Brouillard de guerre** : tout ce qui est dessiné passe par
//! [`LocalVision::sees`], le seul point qui décide de ce que voit le joueur
//! local (pour l'instant : tout).

use bevy::asset::RenderAssetUsages;
use bevy::color::ColorToPacked;
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::buildings::ui::{BORDER, PANEL};
use crate::camera::{ray_at_height, screen_ray};
use crate::collision::{MapBounds, StaticObstacle};
use crate::economy::{EconomyRules, ResourceNode};
use crate::input::BlocksPointer;
use crate::players::{LocalPlayer, Owner, PlayerId};
use crate::selection::Selectable;
use crate::units::UnitCatalog;
use crate::RtsSystems;

/// Côté le plus long de la minicarte, en pixels (logiques) d'écran.
pub const MINIMAP_SIDE: f32 = 224.0;

/// Intervalle entre deux mises à jour de l'image, en secondes.
const REFRESH_SECS: f32 = 0.1;

/// Épaisseur des traits du trapèze de la caméra, en pixels.
const VIEW_LINE: f32 = 1.5;

/// Couleurs du fond : sol, quadrillage tous les 10 m, bord de la carte.
const GROUND: [u8; 4] = [22, 28, 32, 255];
const GRID: [u8; 4] = [30, 38, 43, 255];
const EDGE: [u8; 4] = [46, 120, 132, 255];
/// Couleur d'une entité dont on ne connaît pas l'équipe.
const NEUTRAL: Color = Color::srgb(0.7, 0.7, 0.7);

/// Le joueur local voit-il cette entité ? **Le** point d'accroche du
/// brouillard de guerre pour l'interface : la minicarte (et plus tard le
/// reste) ne montre que ce qu'il accepte.
///
/// `#[derive(SystemParam)]` : un paramètre de système fait maison, qui
/// regroupe d'autres paramètres (ici le joueur local ; demain la carte du
/// brouillard). Les systèmes l'écrivent `vision: LocalVision`.
#[derive(SystemParam)]
pub struct LocalVision<'w> {
    local: Res<'w, LocalPlayer>,
}

impl LocalVision<'_> {
    /// Vrai si le joueur local voit l'entité `entity`, de propriétaire
    /// `owner` (`None` : un gisement, un élément de décor), placée en
    /// `position` (x, z).
    ///
    /// Pour l'instant, sans brouillard de guerre, tout est visible.
    pub fn sees(&self, entity: Entity, owner: Option<PlayerId>, position: Vec2) -> bool {
        // Ses propres entités sont toujours visibles.
        if owner == Some(self.local.0) {
            return true;
        }
        // `let _ = …` : ces valeurs serviront au brouillard ; on les « utilise »
        // pour que le compilateur ne signale pas de paramètre inutilisé.
        let _ = (entity, position);
        true
    }
}

/// Le nœud d'interface qui représente le sol de la carte : la couche
/// d'entrées s'en sert pour traduire un clic en point du sol.
#[derive(Component, Debug, Clone, Copy)]
pub struct MinimapSurface {
    /// La zone (x, z) de la carte représentée.
    pub bounds: Rect,
}

impl MinimapSurface {
    /// Point du sol (x, z) sous la position `local` dans la minicarte, de
    /// (0, 0) en haut à gauche à (1, 1) en bas à droite (bornée à la carte).
    pub fn to_world(&self, local: Vec2) -> Vec2 {
        self.bounds.min + local.clamp(Vec2::ZERO, Vec2::ONE) * self.bounds.size()
    }

    /// L'inverse : position dans la minicarte (de 0 à 1 sur la carte, hors de
    /// cet intervalle au-delà) d'un point du sol (x, z).
    pub fn to_local(&self, world: Vec2) -> Vec2 {
        (world - self.bounds.min) / self.bounds.size()
    }
}

/// Taille de la minicarte (en pixels d'écran, et donc en pixels d'image)
/// pour une carte de cette taille : `MINIMAP_SIDE` sur le côté le plus long.
pub fn minimap_size(bounds: Rect) -> UVec2 {
    let size = bounds.size().max(Vec2::splat(1e-3));
    let scale = MINIMAP_SIDE / size.max_element();
    (size * scale).round().as_uvec2().max(UVec2::ONE)
}

/// L'image de la minicarte et son fond préparé.
#[derive(Resource, Debug)]
struct MinimapCanvas {
    image: Handle<Image>,
    size: UVec2,
    surface: MinimapSurface,
    /// Sol et quadrillage, recopiés au début de chaque mise à jour.
    background: Vec<u8>,
}

/// Un des quatre traits du trapèze de la caméra (dans l'ordre des coins).
#[derive(Component, Debug)]
struct ViewLine(usize);

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_minimap)
            .add_systems(Update, (draw_minimap, update_view_lines).after(RtsSystems::Actions));
    }
}

/// Dessine le fond : sol, quadrillage tous les 10 m, bord de la carte.
fn background(size: UVec2, surface: &MinimapSurface) -> Vec<u8> {
    let (w, h) = (size.x as usize, size.y as usize);
    let mut data = Vec::with_capacity(w * h * 4);
    // Mètres par pixel d'image, sur chaque axe.
    let step = surface.bounds.size() / size.as_vec2();
    for y in 0..h {
        for x in 0..w {
            let world = surface.to_world((Vec2::new(x as f32, y as f32) + 0.5) / size.as_vec2());
            // Une ligne du quadrillage passe-t-elle dans ce pixel ?
            // (à moins d'un demi-pixel d'un multiple de 10 m)
            let on_grid = |v: f32, s: f32| (v - (v / 10.0).round() * 10.0).abs() < s / 2.0;
            let pixel = if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                EDGE
            } else if on_grid(world.x, step.x) || on_grid(world.y, step.y) {
                GRID
            } else {
                GROUND
            };
            data.extend_from_slice(&pixel);
        }
    }
    data
}

/// Remplit le rectangle de pixels [`min`, `max`[ (bornés à l'image).
fn fill_rect(data: &mut [u8], size: UVec2, min: IVec2, max: IVec2, color: [u8; 4]) {
    let min = min.max(IVec2::ZERO);
    let max = max.min(size.as_ivec2());
    for y in min.y..max.y {
        let row = y as usize * size.x as usize;
        for x in min.x..max.x {
            let i = (row + x as usize) * 4;
            data[i..i + 4].copy_from_slice(&color);
        }
    }
}

/// Pixels couverts par un rectangle du sol de centre `center` et de
/// demi-côtés `half` (en mètres), au moins `min_px` pixels de côté.
fn rect_pixels(surface: &MinimapSurface, size: UVec2, center: Vec2, half: Vec2, min_px: f32) -> (IVec2, IVec2) {
    let px = size.as_vec2();
    let c = surface.to_local(center) * px;
    let half_px = (half / surface.bounds.size() * px).max(Vec2::splat(min_px / 2.0));
    ((c - half_px).round().as_ivec2(), (c + half_px).round().as_ivec2())
}

fn spawn_minimap(mut commands: Commands, bounds: Res<MapBounds>, mut images: ResMut<Assets<Image>>) {
    let surface = MinimapSurface { bounds: bounds.0 };
    let size = minimap_size(bounds.0);
    let background = background(size, &surface);
    let mut image = Image::new_fill(
        Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &GROUND,
        TextureFormat::Rgba8UnormSrgb,
        // L'image reste aussi côté processeur (`MAIN_WORLD`) pour qu'on la redessine.
        RenderAssetUsages::default(),
    );
    image.data = Some(background.clone());
    // Pixels nets plutôt que lissés.
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);

    commands
        .spawn((
            Name::new("Minicarte"),
            BlocksPointer,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(14.0),
                right: Val::Px(14.0),
                padding: UiRect::all(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(BORDER),
        ))
        .with_children(|panel| {
            panel
                .spawn((
                    surface,
                    ImageNode::new(handle.clone()),
                    Node {
                        width: Val::Px(size.x as f32),
                        height: Val::Px(size.y as f32),
                        // Le trapèze de la caméra déborde souvent : on le coupe au bord.
                        overflow: Overflow::clip(),
                        ..default()
                    },
                ))
                .with_children(|map| {
                    for i in 0..4 {
                        map.spawn((
                            ViewLine(i),
                            Node { position_type: PositionType::Absolute, height: Val::Px(VIEW_LINE), ..default() },
                            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                        ));
                    }
                });
        });
    commands.insert_resource(MinimapCanvas { image: handle, size, surface, background });
}

/// Redessine l'image dix fois par seconde : gisements, bâtiments, unités.
fn draw_minimap(
    time: Res<Time<Real>>,
    mut since: Local<Option<f32>>,
    canvas: Option<Res<MinimapCanvas>>,
    mut images: ResMut<Assets<Image>>,
    vision: LocalVision,
    units: Option<Res<UnitCatalog>>,
    rules: Option<Res<EconomyRules>>,
    nodes: Query<(Entity, &ResourceNode, &StaticObstacle)>,
    entities: Query<(Entity, &GlobalTransform, &Owner, &Selectable)>,
) {
    let Some(canvas) = canvas else { return };
    let now = time.elapsed_secs();
    if since.is_some_and(|last| now - last < REFRESH_SECS) {
        return;
    }
    *since = Some(now);
    let Some(mut image) = images.get_mut(&canvas.image) else { return };
    let Some(data) = image.data.as_mut() else { return };
    let (surface, size) = (&canvas.surface, canvas.size);
    data.copy_from_slice(&canvas.background);

    // Les gisements, à la couleur de leur ressource.
    for (entity, node, obstacle) in &nodes {
        let center = obstacle.0.center();
        if !vision.sees(entity, None, center) {
            continue;
        }
        let color = rules.as_ref().map_or(NEUTRAL, |r| r.resource(node.kind).color);
        let (min, max) = rect_pixels(surface, size, center, obstacle.0.half_size(), 2.0);
        fill_rect(data, size, min, max, color.to_srgba().to_u8_array());
    }

    // Les bâtiments (rectangles cerclés de sombre), puis les unités (points)
    // par-dessus. Couleur : celle de l'équipe du propriétaire.
    let team = |owner: PlayerId| units.as_ref().and_then(|u| u.team_color(owner)).unwrap_or(NEUTRAL);
    for pass_buildings in [true, false] {
        for (entity, transform, owner, selectable) in &entities {
            if selectable.half_extents.is_some() != pass_buildings {
                continue;
            }
            let center = transform.translation().xz();
            if !vision.sees(entity, Some(owner.0), center) {
                continue;
            }
            let color = team(owner.0).to_srgba();
            match selectable.half_extents {
                Some(half) => {
                    let (min, max) = rect_pixels(surface, size, center, half, 3.0);
                    fill_rect(data, size, min, max, (color * 0.45).with_alpha(1.0).to_u8_array());
                    fill_rect(data, size, min + 1, max - 1, color.to_u8_array());
                }
                None => {
                    let (min, max) = rect_pixels(surface, size, center, Vec2::splat(selectable.radius), 2.0);
                    fill_rect(data, size, min, max, color.to_u8_array());
                }
            }
        }
    }
}

/// Place les quatre traits du trapèze : les coins de l'écran projetés au sol.
fn update_view_lines(
    canvas: Option<Res<MinimapCanvas>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut lines: Query<(&ViewLine, &mut Node, &mut UiTransform)>,
) {
    let Some(canvas) = canvas else { return };
    let (camera, camera_transform) = camera.into_inner();
    let Some(view) = camera.logical_viewport_rect() else { return };
    let corners = [view.min, Vec2::new(view.max.x, view.min.y), view.max, Vec2::new(view.min.x, view.max.y)].map(|corner| {
        let ray = screen_ray(camera, camera_transform, corner)?;
        // Un coin au-dessus de l'horizon : un point lointain dans sa direction.
        let ground = ray_at_height(ray, 0.0).unwrap_or_else(|| ray.get_point(1000.0));
        Some(canvas.surface.to_local(ground.xz()) * canvas.size.as_vec2())
    });
    for (line, mut node, mut transform) in &mut lines {
        let (Some(a), Some(b)) = (corners[line.0], corners[(line.0 + 1) % 4]) else { continue };
        // Un trait : un rectangle fin, centré au milieu du segment et tourné
        // de son angle (dans le sens des aiguilles d'une montre, y vers le bas).
        let length = a.distance(b);
        let middle = (a + b) / 2.0;
        node.left = Val::Px(middle.x - length / 2.0);
        node.top = Val::Px(middle.y - VIEW_LINE / 2.0);
        node.width = Val::Px(length);
        let rotation = Rot2::radians((b - a).to_angle());
        if transform.rotation != rotation {
            transform.rotation = rotation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimap_and_world_points_convert_both_ways() {
        let surface = MinimapSurface { bounds: Rect::new(-50.0, -50.0, 50.0, 50.0) };
        // Le coin en haut à gauche de la minicarte est le coin nord-ouest (-x, -z).
        assert_eq!(surface.to_world(Vec2::ZERO), Vec2::new(-50.0, -50.0));
        assert_eq!(surface.to_world(Vec2::ONE), Vec2::new(50.0, 50.0));
        assert_eq!(surface.to_world(Vec2::new(0.5, 0.25)), Vec2::new(0.0, -25.0));
        // Hors de la minicarte : borné au bord de la carte.
        assert_eq!(surface.to_world(Vec2::new(-0.3, 1.4)), Vec2::new(-50.0, 50.0));
        // Aller-retour.
        for point in [Vec2::new(12.5, -7.0), Vec2::new(-49.0, 33.0)] {
            assert!(surface.to_world(surface.to_local(point)).distance(point) < 1e-4);
        }
        // Une carte qui n'est pas centrée sur l'origine.
        let shifted = MinimapSurface { bounds: Rect::new(0.0, 10.0, 200.0, 110.0) };
        assert_eq!(shifted.to_local(Vec2::new(100.0, 35.0)), Vec2::new(0.5, 0.25));
    }

    #[test]
    fn minimap_keeps_the_map_proportions() {
        assert_eq!(minimap_size(Rect::new(-50.0, -50.0, 50.0, 50.0)), UVec2::splat(MINIMAP_SIDE as u32));
        let wide = minimap_size(Rect::new(0.0, 0.0, 200.0, 100.0));
        assert_eq!(wide, UVec2::new(MINIMAP_SIDE as u32, MINIMAP_SIDE as u32 / 2));
    }

    #[test]
    fn rectangles_are_clipped_to_the_image() {
        let size = UVec2::new(4, 3);
        let mut data = vec![0; 4 * 3 * 4];
        fill_rect(&mut data, size, IVec2::new(-2, 1), IVec2::new(2, 9), [9, 9, 9, 9]);
        let painted: Vec<bool> = data.chunks(4).map(|p| p[0] == 9).collect();
        assert_eq!(painted, [false, false, false, false, true, true, false, false, true, true, false, false]);
    }

    #[test]
    fn small_units_still_get_a_visible_dot() {
        let surface = MinimapSurface { bounds: Rect::new(-50.0, -50.0, 50.0, 50.0) };
        let (min, max) = rect_pixels(&surface, UVec2::splat(100), Vec2::ZERO, Vec2::splat(0.1), 2.0);
        assert_eq!(max - min, IVec2::splat(2));
        assert_eq!(min, IVec2::splat(49));
    }
}
