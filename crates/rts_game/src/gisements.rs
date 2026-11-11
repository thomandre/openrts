//! Les gisements de ressources : noms (provisoires), disposition autour d'une
//! base de départ (données de carte) et visuels.
//!
//! Les deux ressources n'ont pas encore de nom définitif (question ouverte de
//! GAME_DESIGN.md). Noms **provisoires**, faciles à changer ici :
//! - ressource principale : le **Cristal**, en grappes de cristaux bleu
//!   glacier sur des socles de roche veinés de lumière ;
//! - ressource rare : l'**Éther**, une lueur violette qui monte d'une
//!   faille dans la roche (la « source »), qu'on ne récolte qu'en la
//!   couvrant d'un Extracteur.
//!
//! Comme dans les RTS du genre, une base de départ a une ligne de gisements
//! de Cristal à quelques mètres derrière le Quartier général et deux sources
//! d'Éther sur ses flancs. La disposition est décrite en données, par
//! rapport au centre du Quartier général ([`BaseLayout`]).

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use rts_engine::economy::{spawn_resource_node, DepletingVisual, ResourceInfo, ResourceKind};
use rts_engine::graphics::shapes::merge_parts;
use rts_engine::navigation::GridFrame;
use rts_engine::units::PartShape;

/// Nom provisoire de la ressource principale.
pub const PRIMARY_NAME: &str = "Cristal";
/// Nom provisoire de la ressource rare.
pub const RARE_NAME: &str = "Éther";

/// Couleur du Cristal : bleu glacier, distinct du cyan des visières.
pub const PRIMARY_COLOR: Color = Color::srgb(0.42, 0.68, 1.0);
/// Couleur de l'Éther : violet lumineux, loin de l'orange des équipes.
pub const RARE_COLOR: Color = Color::srgb(0.78, 0.4, 1.0);

/// Noms et couleurs des deux ressources, pour le moteur (dans l'ordre :
/// principale, rare).
pub fn resource_infos() -> [ResourceInfo; 2] {
    [
        ResourceInfo { name: PRIMARY_NAME.into(), color: PRIMARY_COLOR },
        ResourceInfo { name: RARE_NAME.into(), color: RARE_COLOR },
    ]
}

/// Empreinte d'un gisement de Cristal, en cases : 2 × 1 m.
const PRIMARY_SIZE: UVec2 = UVec2::new(2, 1);
/// Empreinte d'une source d'Éther : 4 × 4 m, celle de l'Extracteur.
const RARE_SIZE: UVec2 = UVec2::new(4, 4);

/// Quantités (valeurs proposées, à ajuster en test) : les gisements proches
/// du Quartier général sont plus petits que ceux du fond de la ligne.
const PRIMARY_NEAR: u32 = 900;
const PRIMARY_FAR: u32 = 1500;
const RARE_AMOUNT: u32 = 2250;

/// Disposition des gisements d'une base, autour du centre de son Quartier
/// général (`hq`) : centres des gisements et quantités.
#[derive(Debug, Clone)]
pub struct BaseLayout {
    pub primary: Vec<(Vec2, u32)>,
    pub rare: Vec<(Vec2, u32)>,
}

/// La base de départ : une ligne de huit gisements de Cristal en arc, de 2 à
/// 4 m derrière le Quartier général (vers −z, le haut de l'écran), et deux
/// sources d'Éther à 4,5 m de ses flancs.
///
/// Les centres tombent sur la grille de 1 m : x entier et z en « ,5 » pour
/// un gisement de 2 × 1, x et z entiers pour une source de 4 × 4 (le
/// Quartier général étant lui-même centré sur un point entier).
pub fn starting_base(hq: Vec2) -> BaseLayout {
    let line = [
        (-7.0, -5.5, PRIMARY_NEAR),
        (-5.0, -6.5, PRIMARY_FAR),
        (-3.0, -7.5, PRIMARY_FAR),
        (-1.0, -6.5, PRIMARY_NEAR),
        (1.0, -7.5, PRIMARY_FAR),
        (3.0, -6.5, PRIMARY_NEAR),
        (5.0, -7.5, PRIMARY_FAR),
        (7.0, -5.5, PRIMARY_NEAR),
    ];
    BaseLayout {
        primary: line.iter().map(|&(x, z, amount)| (hq + Vec2::new(x, z), amount)).collect(),
        rare: vec![(hq + Vec2::new(-10.0, -2.0), RARE_AMOUNT), (hq + Vec2::new(10.0, -2.0), RARE_AMOUNT)],
    }
}

/// Maillages et matériaux des gisements, préparés une fois et partagés par
/// tous (dessin en lot).
pub struct DepositVisuals {
    rock: Handle<Mesh>,
    rock_material: Handle<StandardMaterial>,
    veins: Handle<Mesh>,
    vein_material: Handle<StandardMaterial>,
    /// Deux grappes de cristaux différentes, en alternance le long de la ligne.
    crystals: [Handle<Mesh>; 2],
    crystal_material: Handle<StandardMaterial>,
    vent: Handle<Mesh>,
    pool: Handle<Mesh>,
    shards: Handle<Mesh>,
    ether_material: Handle<StandardMaterial>,
}

impl DepositVisuals {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        // Un cristal : prisme hexagonal effilé (forme de révolution taillée à
        // 6 faces), profil (rayon, hauteur) du pied à la pointe.
        let crystal = |radius: f32, height: f32| {
            let (radius, height) = (radius * 1.25, height * 1.15);
            PartShape::Faceted {
            profile: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(radius * 0.85, 0.02),
                Vec2::new(radius, height * 0.65),
                Vec2::new(0.0, height),
            ],
            sides: 6,
            }
        };
        // Une grappe : (rayon, hauteur, position x/z, inclinaison autour de z puis de x).
        let cluster = |spec: &[(f32, f32, f32, f32, f32, f32)]| {
            let shapes: Vec<(PartShape, Transform)> = spec
                .iter()
                .map(|&(r, h, x, z, tilt_z, tilt_x)| {
                    let transform = Transform::from_xyz(x, 0.2, z)
                        .with_rotation(Quat::from_euler(EulerRot::XYZ, tilt_x, x * 2.1, tilt_z));
                    (crystal(r, h), transform)
                })
                .collect();
            merge_parts(shapes.iter().map(|(s, t)| (s, *t)))
        };
        let cluster_a = cluster(&[
            (0.2, 1.35, -0.15, 0.0, 0.12, 0.05),
            (0.15, 0.95, 0.35, 0.1, -0.35, 0.1),
            (0.14, 0.8, -0.6, -0.05, 0.45, -0.1),
            (0.12, 0.6, 0.72, -0.12, -0.55, 0.15),
            (0.1, 0.5, 0.1, 0.28, 0.1, 0.5),
        ]);
        let cluster_b = cluster(&[
            (0.18, 1.1, 0.2, 0.05, -0.1, -0.08),
            (0.16, 1.2, -0.3, -0.05, 0.25, 0.1),
            (0.12, 0.7, -0.72, 0.1, 0.5, 0.1),
            (0.13, 0.75, 0.66, 0.0, -0.45, -0.1),
            (0.09, 0.45, -0.05, -0.3, 0.05, -0.55),
        ]);

        // Socle de roche biseauté, et veines lumineuses en travers du dessus.
        let rock = PartShape::Plate { size: Vec3::new(1.95, 0.3, 0.95), bevel: 0.12 };
        let veins = [(-0.55, 0.4), (0.05, -0.5), (0.55, 0.3)].map(|(x, angle)| {
            (PartShape::Plate { size: Vec3::new(0.05, 0.03, 0.75), bevel: 0.01 }, Transform::from_xyz(x, 0.3, 0.0).with_rotation(Quat::from_rotation_y(angle)))
        });

        // Source d'Éther : margelle de roche, nappe lumineuse, éclats autour.
        let vent = PartShape::Lathe {
            profile: vec![
                Vec2::new(1.95, 0.0),
                Vec2::new(1.9, 0.25),
                Vec2::new(1.6, 0.4),
                Vec2::new(1.3, 0.3),
                Vec2::new(1.25, 0.12),
            ],
            segments: 10,
        };
        let pool = PartShape::Cylinder { radius: 1.3, height: 0.1 };
        let shards: Vec<(PartShape, Transform)> = (0..7)
            .map(|i| {
                let angle = i as f32 / 7.0 * std::f32::consts::TAU + 0.3;
                let distance = 1.45 + 0.15 * (i % 2) as f32;
                let height = 0.45 + 0.25 * ((i * 5) % 3) as f32;
                let at = Vec3::new(angle.cos() * distance, 0.25, angle.sin() * distance);
                let lean = Quat::from_rotation_arc(Vec3::Y, (Vec3::Y * 2.0 + at.with_y(0.0).normalize()).normalize());
                (crystal(0.12, height), Transform::from_translation(at).with_rotation(lean))
            })
            .collect();

        let glow = |color: Color, strength: f32| StandardMaterial {
            base_color: Color::BLACK,
            emissive: color.to_linear() * strength,
            ..default()
        };
        Self {
            rock: meshes.add(merge_parts([(&rock, Transform::from_xyz(0.0, 0.15, 0.0))])),
            rock_material: materials.add(StandardMaterial {
                base_color: Color::srgb(0.1, 0.11, 0.13),
                perceptual_roughness: 0.85,
                ..default()
            }),
            veins: meshes.add(merge_parts(veins.iter().map(|(s, t)| (s, *t)))),
            vein_material: materials.add(glow(PRIMARY_COLOR, 1.4)),
            crystals: [meshes.add(cluster_a), meshes.add(cluster_b)],
            // Cristal : éclairé (reflets nets sur les facettes) et lumineux de l'intérieur.
            crystal_material: materials.add(StandardMaterial {
                base_color: PRIMARY_COLOR.darker(0.25),
                emissive: PRIMARY_COLOR.to_linear() * 0.9,
                metallic: 0.3,
                perceptual_roughness: 0.1,
                ..default()
            }),
            vent: meshes.add(merge_parts([(&vent, Transform::IDENTITY)])),
            pool: meshes.add(merge_parts([(&pool, Transform::from_xyz(0.0, 0.12, 0.0))])),
            shards: meshes.add(merge_parts(shards.iter().map(|(s, t)| (s, *t)))),
            ether_material: materials.add(StandardMaterial {
                base_color: RARE_COLOR.darker(0.3),
                emissive: RARE_COLOR.to_linear() * 1.1,
                metallic: 0.2,
                perceptual_roughness: 0.15,
                ..default()
            }),
        }
    }
}

/// Fait apparaître les gisements d'une base.
pub fn spawn_base(commands: &mut Commands, visuals: &DepositVisuals, frame: GridFrame, layout: &BaseLayout) {
    for (i, &(center, amount)) in layout.primary.iter().enumerate() {
        let min_cell = frame.min_cell_centered(center, PRIMARY_SIZE);
        let node = spawn_resource_node(commands, frame, ResourceKind::Primary, min_cell, PRIMARY_SIZE, amount, false);
        commands.entity(node).insert(Name::new("Gisement de Cristal")).with_children(|parent| {
            parent.spawn((Mesh3d(visuals.rock.clone()), MeshMaterial3d(visuals.rock_material.clone())));
            parent.spawn((Mesh3d(visuals.veins.clone()), MeshMaterial3d(visuals.vein_material.clone()), NotShadowCaster));
            // La grappe rétrécit à mesure que le gisement se vide.
            parent.spawn((
                DepletingVisual::default(),
                Mesh3d(visuals.crystals[i % 2].clone()),
                MeshMaterial3d(visuals.crystal_material.clone()),
                Transform::default(),
            ));
        });
    }
    for &(center, amount) in &layout.rare {
        let min_cell = frame.min_cell_centered(center, RARE_SIZE);
        let node = spawn_resource_node(commands, frame, ResourceKind::Rare, min_cell, RARE_SIZE, amount, true);
        commands.entity(node).insert(Name::new("Source d'Éther")).with_children(|parent| {
            parent.spawn((Mesh3d(visuals.vent.clone()), MeshMaterial3d(visuals.rock_material.clone())));
            parent.spawn((
                DepletingVisual { min_scale: Vec2::new(0.4, 1.0) },
                Mesh3d(visuals.pool.clone()),
                MeshMaterial3d(visuals.ether_material.clone()),
                Transform::default(),
                NotShadowCaster,
            ));
            parent.spawn((Mesh3d(visuals.shards.clone()), MeshMaterial3d(visuals.ether_material.clone()), NotShadowCaster));
        });
    }
}
