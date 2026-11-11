//! La carte : un sol texturé, une grille discrète au sol et un liseré
//! lumineux qui marque les limites de la zone de jeu.
//!
//! L'aspect du sol dépend de la faction affichée ([`GroundStyle`] et
//! [`GroundPattern`]) : roche sombre pour la Garde, terre forestière pour
//! les Sylves, dalles hexagonales sombres pour le Collectif.

use bevy::light::NotShadowCaster;
use bevy::math::Affine2;
use bevy::prelude::*;
use rts_engine::graphics::textures::{hex_floor_images, terrain_images, velvet_images};

/// Une répétition de la texture du sol mesure 8 m (la grille tombe tous les 4 m).
const TERRAIN_TILE: f32 = 8.0;
/// Le sol continue au-delà de la carte, jusqu'à se fondre dans le brouillard.
const OUTER_SIZE: f32 = 500.0;

/// Aspect du sol et du liseré de la carte.
#[derive(Debug, Clone, Copy)]
pub struct GroundStyle {
    /// Teinte principale du terrain (roche, terre).
    pub base: Color,
    /// Seconde teinte, mélangée par plaques (poussière, humus).
    pub dust: Color,
    /// Grille discrète au sol, pour juger les distances.
    pub grid_lines: bool,
    /// 0 : sol mouillé et brillant ; 1 : complètement mat.
    pub roughness: f32,
    /// Couleur lumineuse du liseré qui marque les limites de la carte.
    pub border: LinearRgba,
}

impl GroundStyle {
    /// Roche sombre et grille, liseré cyan (Garde).
    pub fn rock() -> Self {
        Self {
            base: Color::srgb(0.085, 0.1, 0.12),
            dust: Color::srgb(0.15, 0.155, 0.16),
            grid_lines: true,
            roughness: 0.92,
            border: LinearRgba::rgb(0.31, 0.82, 0.88) * 0.8,
        }
    }
}

/// Motif du sol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundPattern {
    /// Terrain naturel : roche ou terre, cailloux et fissures.
    Natural,
    /// Velours d'écrin : grandes moirures douces entre les deux teintes du
    /// [`GroundStyle`], sans cailloux ni grille (`grid_lines` est ignoré).
    Velvet,
    /// Dalles hexagonales sombres, d'une teinte tirée entre les deux du
    /// [`GroundStyle`] (le sol de la page de direction artistique, retenu
    /// pour le Collectif) ; `grid_lines` est ignoré, les joints en tiennent lieu.
    Hexagons,
}

pub fn spawn_ground(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    map_size: f32,
    style: &GroundStyle,
    pattern: GroundPattern,
) {
    let terrain = match pattern {
        GroundPattern::Natural => terrain_images(style.base, style.dust, style.grid_lines),
        GroundPattern::Velvet => velvet_images(style.base, style.dust),
        GroundPattern::Hexagons => hex_floor_images(style.base, style.dust),
    };
    let albedo = images.add(terrain.albedo);
    let normal = images.add(terrain.normal);

    // Un seul grand plan : la carte et ses abords partagent la même texture,
    // répétée tous les `TERRAIN_TILE` mètres.
    commands.spawn((
        Name::new("Sol"),
        Mesh3d(meshes.add(ground_mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(albedo),
            normal_map_texture: Some(normal),
            perceptual_roughness: style.roughness,
            uv_transform: Affine2::from_scale(Vec2::splat(OUTER_SIZE / TERRAIN_TILE)),
            ..default()
        })),
    ));

    // Liseré lumineux autour de la zone jouable.
    let border = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: style.border,
        ..default()
    });
    let half = map_size / 2.0;
    let line = meshes.add(Cuboid::new(map_size + 0.2, 0.02, 0.2));
    for (pos, rotation) in [
        (Vec3::new(0.0, 0.01, -half), 0.0),
        (Vec3::new(0.0, 0.01, half), 0.0),
        (Vec3::new(-half, 0.01, 0.0), std::f32::consts::FRAC_PI_2),
        (Vec3::new(half, 0.01, 0.0), std::f32::consts::FRAC_PI_2),
    ] {
        commands.spawn((
            Name::new("Limite de carte"),
            Mesh3d(line.clone()),
            MeshMaterial3d(border.clone()),
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(rotation)),
            NotShadowCaster,
        ));
    }
}

/// Le plan du sol, avec ses tangentes (nécessaires à la normal map).
fn ground_mesh() -> Mesh {
    let mut mesh = Plane3d::default().mesh().size(OUTER_SIZE, OUTER_SIZE).build();
    if let Err(error) = mesh.generate_tangents() {
        warn!("Tangentes du sol impossibles à calculer : {error}");
    }
    mesh
}
