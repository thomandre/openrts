//! Brouillard de guerre, côté affichage, pour le joueur local
//! ([`LocalPlayer`]). **Affichage seulement** : ces systèmes tournent dans
//! `Update`, lisent [`FogOfWar`] et ne modifient jamais l'état du jeu.
//!
//! - Les unités ennemies hors de vue sont cachées et marquées [`FogHidden`] :
//!   on ne peut ni les sélectionner ni les viser, et leurs barres de vie,
//!   anneaux et tirs ne sont pas dessinés.
//! - Un bâtiment ennemi hors de vue est caché lui aussi ; s'il a déjà été
//!   aperçu, un **souvenir** ([`BuildingMemory`]) le remplace : son modèle
//!   figé tel qu'au dernier aperçu, qui reste même s'il a été détruit
//!   depuis, jusqu'à ce qu'on revoie l'emplacement.
//! - Le sol est assombri là où l'on ne voit pas, davantage là où l'on n'est
//!   jamais allé : une texture d'une case par mètre (plus un liseré sombre)
//!   posée sur un grand plan semi-transparent juste au-dessus du sol. Les
//!   bords sont adoucis trois fois : la teinte de chaque case glisse en
//!   douceur vers sa valeur (en un quart de seconde environ), un petit flou
//!   lisse les marches d'escalier, et la carte graphique interpole entre les
//!   cases (filtrage linéaire).

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::{CellVision, FogOfWar};
use crate::buildings::{Building, BuildingCatalog};
use crate::collision::{MapBounds, StaticObstacle};
use crate::players::{LocalPlayer, Owner, PlayerId};
use crate::selection::Selected;
use crate::simulation::SimPosition;
use crate::units::Unit;
use crate::RtsSystems;

/// Marqueur : cette unité ou ce bâtiment ennemi est hors de la vue du
/// joueur local (il n'est pas dessiné). La minicarte peut s'en servir.
#[derive(Component, Debug, Default)]
pub struct FogHidden;

/// Le souvenir d'un bâtiment ennemi hors de vue : un modèle figé, sans
/// simulation (affichage seulement). La minicarte peut s'en servir.
#[derive(Component, Debug, Clone, Copy)]
pub struct BuildingMemory {
    /// Le bâtiment dont c'est le souvenir (il n'existe peut-être plus).
    pub of: Entity,
    pub owner: PlayerId,
    pub type_index: usize,
    /// Son empreinte au sol.
    pub rect: Rect,
}

/// Marqueur : le plan semi-transparent du brouillard, au-dessus du sol.
#[derive(Component, Debug)]
struct FogPlane;

/// Le calque du brouillard : sa texture et la teinte affichée de chaque
/// case (qui suit en douceur l'état de la case).
#[derive(Resource)]
struct FogLayer {
    image: Handle<Image>,
    /// Taille de la texture : la grille de vision plus une case de liseré
    /// tout autour.
    width: usize,
    height: usize,
    /// Opacité affichée de chaque texel, de 0 (rien) à 1 (noir complet).
    shade: Vec<f32>,
    /// Tableau de travail du flou.
    blurred: Vec<f32>,
    /// Les teintes ont fini de glisser vers leur valeur.
    settled: bool,
    /// Version de la vision au dernier calcul des teintes.
    version: u64,
}

/// Opacité du brouillard sur une case déjà vue, et sur une case jamais vue.
const EXPLORED_SHADE: f32 = 0.45;
const UNEXPLORED_SHADE: f32 = 0.72;
/// Couleur du brouillard (sRGB) : un bleu nuit presque noir, qui assombrit
/// sans salir les couleurs de l'ambiance.
const FOG_TINT: [u8; 3] = [5, 8, 16];
/// Vitesse à laquelle une case change de teinte (par seconde).
const SHADE_SPEED: f32 = 5.0;
/// Hauteur du plan au-dessus du sol : au-dessus des anneaux et marques au
/// sol (4 à 6 cm), sous les pieds des unités.
const PLANE_HEIGHT: f32 = 0.07;
/// Le plan déborde de la carte de cette distance (mètres), pour assombrir
/// aussi les abords de la carte qu'on aperçoit en dézoomant.
const PLANE_MARGIN: f32 = 200.0;

pub struct FogDisplayPlugin;

impl Plugin for FogDisplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            // Avant les actions : un clic ne doit pas trouver une entité cachée.
            (hide_unseen, update_fog_layer).chain().before(RtsSystems::Actions),
        );
    }
}

/// Cache ce que le joueur local ne voit pas, et tient à jour les souvenirs
/// de bâtiments.
fn hide_unseen(
    mut commands: Commands,
    fog: Option<Res<FogOfWar>>,
    local: Res<LocalPlayer>,
    catalog: Option<Res<BuildingCatalog>>,
    mut units: Query<(Entity, &Owner, &SimPosition, &mut Visibility, Has<FogHidden>), With<Unit>>,
    mut buildings: Query<(Entity, &Owner, &StaticObstacle, &mut Visibility, Has<FogHidden>), (With<Building>, Without<Unit>)>,
    memories: Query<(Entity, &BuildingMemory)>,
) {
    let Some(fog) = fog else { return };
    let viewer = local.0;
    // Montre ou cache une entité, si c'est un changement.
    let set = |commands: &mut Commands, entity: Entity, visibility: &mut Visibility, was_hidden: bool, seen: bool| {
        if seen == !was_hidden {
            return;
        }
        if seen {
            *visibility = Visibility::Inherited;
            commands.entity(entity).remove::<FogHidden>();
        } else {
            *visibility = Visibility::Hidden;
            // Une entité qui disparaît dans le brouillard quitte la sélection.
            commands.entity(entity).insert(FogHidden).remove::<Selected>();
        }
    };
    for (entity, owner, position, mut visibility, hidden) in &mut units {
        let seen = fog.sees_entity(viewer, owner.0, position.current, None);
        set(&mut commands, entity, &mut visibility, hidden, seen);
    }
    // Souvenirs déjà affichés, triés pour les retrouver par dichotomie.
    let mut shown: Vec<(Entity, Entity)> = memories.iter().map(|(memory, m)| (m.of, memory)).collect();
    shown.sort_unstable();
    let memory_of = |building: Entity| shown.binary_search_by_key(&building, |(of, _)| *of).ok().map(|i| shown[i].1);
    for (entity, owner, obstacle, mut visibility, hidden) in &mut buildings {
        let seen = fog.sees_entity(viewer, owner.0, obstacle.0.center(), Some(obstacle.0));
        set(&mut commands, entity, &mut visibility, hidden, seen);
        match (seen, memory_of(entity), fog.remembers(viewer, entity)) {
            // Revu : le vrai bâtiment remplace son souvenir.
            (true, Some(memory), _) => commands.entity(memory).despawn(),
            // Perdu de vue après avoir été aperçu : son souvenir apparaît.
            (false, None, Some(remembered)) => {
                let spawned = catalog.as_ref().and_then(|c| {
                    c.spawn_memory(&mut commands, remembered.owner, remembered.type_index, remembered.rect.center(), remembered.progress)
                });
                if let Some(memory) = spawned {
                    commands.entity(memory).insert(BuildingMemory {
                        of: entity,
                        owner: remembered.owner,
                        type_index: remembered.type_index,
                        rect: remembered.rect,
                    });
                }
            }
            _ => {}
        }
    }
    // Souvenirs oubliés (emplacement revu vide) ou brouillard coupé.
    for (memory, m) in &memories {
        if !fog.is_enabled() || fog.remembers(viewer, m.of).is_none() {
            commands.entity(memory).despawn();
        }
    }
}

/// Crée le calque du brouillard, puis met à jour sa texture quand la
/// vision change ou que les teintes n'ont pas fini de glisser.
fn update_fog_layer(
    mut commands: Commands,
    time: Res<Time>,
    fog: Option<Res<FogOfWar>>,
    local: Res<LocalPlayer>,
    bounds: Res<MapBounds>,
    layer: Option<ResMut<FogLayer>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut planes: Query<&mut Visibility, With<FogPlane>>,
) {
    let Some(fog) = fog else { return };
    for mut visibility in &mut planes {
        visibility.set_if_neq(if fog.is_enabled() { Visibility::Inherited } else { Visibility::Hidden });
    }
    let cells = fog.cells(local.0);
    if !fog.is_enabled() || cells.is_empty() {
        return;
    }
    let size = fog.size();
    let (columns, rows) = (size.x as usize, size.y as usize);
    let Some(mut layer) = layer else {
        let layer = spawn_layer(&mut commands, &mut images, &mut meshes, &mut materials, &fog, bounds.0);
        commands.insert_resource(layer);
        return;
    };
    if layer.width != columns + 2 || layer.height != rows + 2 {
        return;
    }
    if layer.settled && layer.version == fog.version() {
        return;
    }
    layer.version = fog.version();

    // 1. Chaque teinte glisse vers celle de l'état de sa case.
    let step = (time.delta_secs() * SHADE_SPEED).min(1.0);
    let width = layer.width;
    let mut settled = true;
    for z in 0..rows {
        for x in 0..columns {
            let target = match cells[z * columns + x] {
                CellVision::Visible => 0.0,
                CellVision::Explored => EXPLORED_SHADE,
                CellVision::Unexplored => UNEXPLORED_SHADE,
            };
            let shade = &mut layer.shade[(z + 1) * width + x + 1];
            let gap = target - *shade;
            if gap.abs() > 0.004 {
                *shade += gap * step;
                settled = false;
            } else {
                *shade = target;
            }
        }
    }
    layer.settled = settled;

    // 2. Petit flou (noyau 1-2-1 sur chaque axe), puis écriture des opacités.
    let height = layer.height;
    // `std::mem::take` : on sort les tableaux de `layer` le temps du calcul,
    // pour pouvoir les emprunter en même temps sans conflit.
    let shade = std::mem::take(&mut layer.shade);
    let mut blurred = std::mem::take(&mut layer.blurred);
    blur(&shade, &mut blurred, width, height);
    if let Some(mut image) = images.get_mut(&layer.image)
        && let Some(data) = image.data.as_mut()
    {
        // `as_chunks_mut::<4>` découpe les octets en texels de 4 (rouge, vert, bleu, opacité).
        for (texel, alpha) in data.as_chunks_mut::<4>().0.iter_mut().zip(&blurred) {
            texel[3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    layer.shade = shade;
    layer.blurred = blurred;
}

/// Flou séparable : moyenne pondérée 1-2-1 de chaque texel et de ses
/// voisins, en ligne puis en colonne (les bords se répètent).
fn blur(source: &[f32], out: &mut Vec<f32>, width: usize, height: usize) {
    let mut rows = vec![0.0; source.len()];
    for z in 0..height {
        for x in 0..width {
            let at = |x: usize| source[z * width + x];
            rows[z * width + x] = (at(x.saturating_sub(1)) + 2.0 * at(x) + at((x + 1).min(width - 1))) / 4.0;
        }
    }
    out.resize(source.len(), 0.0);
    for z in 0..height {
        for x in 0..width {
            let at = |z: usize| rows[z * width + x];
            out[z * width + x] = (at(z.saturating_sub(1)) + 2.0 * at(z) + at((z + 1).min(height - 1))) / 4.0;
        }
    }
}

/// Crée la texture du brouillard et le plan qui la porte.
fn spawn_layer(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    fog: &FogOfWar,
    bounds: Rect,
) -> FogLayer {
    let size = fog.size();
    let (width, height) = (size.x as usize + 2, size.y as usize + 2);
    let [r, g, b] = FOG_TINT;
    let opaque = (UNEXPLORED_SHADE * 255.0) as u8;
    // `RenderAssetUsages::default()` : l'image reste aussi dans le monde
    // principal, pour qu'on puisse la modifier à chaque image.
    let mut image = Image::new_fill(
        Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[r, g, b, opaque],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // Au-delà de la texture, on répète son bord (le liseré sombre) ;
    // entre deux texels, on interpole (bords doux).
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    let image = images.add(image);

    // La texture couvre la grille plus une case de liseré ; le plan, bien
    // plus grand, la reçoit par une transformation de ses coordonnées de
    // texture : `uv_texture = uv_plan × échelle + décalage`.
    let frame = fog.frame();
    let texture_rect = Rect::from_corners(frame.origin - frame.cell_size, frame.origin + (size.as_vec2() + 1.0) * frame.cell_size);
    let plane_rect = bounds.inflate(PLANE_MARGIN);
    let scale = plane_rect.size() / texture_rect.size();
    let offset = (plane_rect.min - texture_rect.min) / texture_rect.size();
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(image.clone()),
        uv_transform: Affine2::from_scale_angle_translation(scale, 0.0, offset),
        alpha_mode: AlphaMode::Blend,
        // Pas d'éclairage : la couleur de la texture telle quelle.
        unlit: true,
        ..default()
    });
    let center = plane_rect.center();
    commands.spawn((
        Name::new("Brouillard de guerre"),
        FogPlane,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(plane_rect.width(), plane_rect.height()))),
        MeshMaterial3d(material),
        Transform::from_xyz(center.x, PLANE_HEIGHT, center.y),
        NotShadowCaster,
        NotShadowReceiver,
    ));
    FogLayer {
        image,
        width,
        height,
        shade: vec![UNEXPLORED_SHADE; width * height],
        blurred: Vec::new(),
        settled: false,
        version: u64::MAX,
    }
}
