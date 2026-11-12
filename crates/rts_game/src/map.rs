//! La carte : un sol texturé (avec son relief), une grille discrète au sol
//! et un liseré lumineux qui marque les limites de la zone de jeu.
//!
//! [`match_terrain`] décrit le relief de la partie normale : deux bases sur
//! des plateaux, une rampe chacune, et deux buttes au centre.
//!
//! L'aspect du sol dépend de la faction affichée ([`GroundStyle`] et
//! [`GroundPattern`]) : roche sombre pour la Garde, terre forestière pour
//! les Sylves, dalles hexagonales sombres pour le Collectif.

use bevy::light::NotShadowCaster;
use bevy::math::Affine2;
use bevy::prelude::*;
use rts_engine::graphics::textures::{hex_floor_images, terrain_images, velvet_images};
use rts_engine::terrain::{ring_mesh, Plateau, Ramp, Terrain, DEFAULT_LEVEL_HEIGHT};

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
    terrain: &Terrain,
) {
    let textures = match pattern {
        GroundPattern::Natural => terrain_images(style.base, style.dust, style.grid_lines),
        GroundPattern::Velvet => velvet_images(style.base, style.dust),
        GroundPattern::Hexagons => hex_floor_images(style.base, style.dust),
    };
    let albedo = images.add(textures.albedo);
    let normal = images.add(textures.normal);

    // Un grand carré de sol : la carte et ses abords partagent la même
    // texture, répétée tous les `TERRAIN_TILE` mètres. Avec du relief, le
    // centre est le maillage du relief et le reste un anneau plat.
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(albedo),
        normal_map_texture: Some(normal),
        perceptual_roughness: style.roughness,
        uv_transform: Affine2::from_scale(Vec2::splat(OUTER_SIZE / TERRAIN_TILE)),
        ..default()
    });
    let outer = Rect::from_center_size(Vec2::ZERO, Vec2::splat(OUTER_SIZE));
    let meshes_to_draw = if terrain.is_flat() {
        vec![ground_mesh()]
    } else {
        vec![terrain.mesh(0.0, outer, true), ring_mesh(terrain.mesh_rect(), outer, 0.0, outer)]
    };
    for mut mesh in meshes_to_draw {
        if let Err(error) = mesh.generate_tangents() {
            warn!("Tangentes du sol impossibles à calculer : {error}");
        }
        commands.spawn((Name::new("Sol"), Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone())));
    }

    // Liseré lumineux autour de la zone jouable.
    let border = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: style.border,
        ..default()
    });
    commands.spawn((
        Name::new("Limite de carte"),
        Mesh3d(meshes.add(border_mesh(map_size / 2.0, terrain))),
        MeshMaterial3d(border),
        NotShadowCaster,
    ));
}

/// Le plan du sol (ses tangentes, nécessaires à la normal map, sont
/// calculées ensuite).
fn ground_mesh() -> Mesh {
    Plane3d::default().mesh().size(OUTER_SIZE, OUTER_SIZE).build()
}

/// Le liseré : un ruban de 20 cm qui fait le tour de la carte en épousant
/// le relief (un morceau tous les 50 cm).
fn border_mesh(half: f32, terrain: &Terrain) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let (mut positions, mut indices) = (Vec::new(), Vec::new());
    let corners = [Vec2::new(-half, -half), Vec2::new(half, -half), Vec2::new(half, half), Vec2::new(-half, half)];
    for side in 0..4 {
        let (a, b) = (corners[side], corners[(side + 1) % 4]);
        // Le ruban déborde de 10 cm de part et d'autre de la limite.
        let across = (b - a).normalize().perp() * 0.1;
        let pieces = ((b - a).length() / 0.5).ceil() as u32;
        for i in 0..=pieces {
            let p = a.lerp(b, i as f32 / pieces as f32);
            let y = terrain.height_at(p) + 0.03;
            positions.push([p.x - across.x, y, p.y - across.y]);
            positions.push([p.x + across.x, y, p.y + across.y]);
            if i > 0 {
                let n = positions.len() as u32;
                // Deux triangles, dans les deux sens (visibles de partout).
                indices.extend_from_slice(&[n - 4, n - 3, n - 2, n - 3, n - 1, n - 2, n - 2, n - 3, n - 4, n - 2, n - 1, n - 3]);
            }
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_indices(Indices::U32(indices))
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
}

/// Le relief de la partie normale, symétrique par rapport au centre (le
/// camp sud est le miroir du camp nord, comme les bases, voir `side`).
///
/// - Chaque base est sur un plateau de niveau 1 qui occupe son bord de
///   carte, et descend vers le centre par une rampe (côté est pour le
///   joueur, ouest pour l'adversaire).
/// - Deux buttes de niveau 1 au centre, chacune avec sa rampe tournée vers
///   le milieu : de là-haut, on voit venir sans être vu.
/// - Entre les deux, la plaine (niveau 0), et des couloirs sur les côtés.
pub fn match_terrain(bounds: Rect) -> Terrain {
    let v = |x: f32, z: f32| Vec2::new(x, z);
    // Le plateau de la base nord (joueur), débordant de la carte.
    let base = vec![
        v(-60.0, -60.0),
        v(60.0, -60.0),
        v(60.0, -29.0),
        v(35.0, -27.0),
        v(28.0, -18.0),
        v(20.0, -13.0),
        v(6.0, -12.0),
        v(-8.0, -13.0),
        v(-20.0, -15.0),
        v(-30.0, -22.0),
        v(-36.0, -30.0),
        v(-60.0, -32.0),
    ];
    let base_ramp = Ramp { bottom: v(14.0, -8.6), top: v(14.0, -16.4), width: 8.0 };
    // La butte ouest, et sa rampe vers le centre.
    let mesa = vec![v(-35.0, 3.0), v(-33.0, -2.0), v(-28.0, -4.0), v(-20.0, -3.0), v(-17.0, 2.0), v(-21.0, 7.0), v(-30.0, 7.0)];
    let mesa_ramp = Ramp { bottom: v(-13.1, 2.0), top: v(-20.9, 2.0), width: 8.0 };

    let mirror = |points: &[Vec2]| points.iter().map(|p| -*p).collect::<Vec<_>>();
    let flip = |ramp: Ramp| Ramp { bottom: -ramp.bottom, top: -ramp.top, ..ramp };
    let plateaus = vec![
        Plateau::new(1, base.clone()),
        Plateau::new(1, mirror(&base)),
        Plateau::new(1, mesa.clone()),
        Plateau::new(1, mirror(&mesa)),
    ];
    let ramps = vec![base_ramp, flip(base_ramp), mesa_ramp, flip(mesa_ramp)];
    Terrain::new(bounds, DEFAULT_LEVEL_HEIGHT, plateaus, ramps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::navigation::{plan_path, NavGrid, PathScratch};

    /// Les plus grosses unités (le Bastion : 1,8 m de rayon) descendent de
    /// leur base et montent sur les buttes.
    #[test]
    fn big_units_can_cross_the_map() {
        let bounds = Rect::new(-50.0, -50.0, 50.0, 50.0);
        let terrain = match_terrain(bounds);
        let mut grid = NavGrid::new(bounds, 1.0);
        grid.set_terrain(&terrain);
        grid.set_obstacles(terrain.cliff_rects().into_iter());
        let mut scratch = PathScratch::default();
        for radius in [0.5, 1.2, 1.8] {
            for goal in [Vec2::new(0.0, 22.0), Vec2::new(-26.0, 2.0), Vec2::new(26.0, -2.0)] {
                let path = plan_path(&grid, Vec2::new(2.5, -23.5), goal, radius, &mut scratch).expect("un chemin");
                assert!(path.goal.distance(goal) < 1.0, "rayon {radius} : vers {goal}, arrêt en {}", path.goal);
            }
        }
    }
}
