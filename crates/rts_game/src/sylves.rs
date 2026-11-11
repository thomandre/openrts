//! Les Sylves : palette, ambiance et unités (faction en exploration visuelle).
//!
//! Direction artistique : « écorce striée, feuillage sombre, poches de sève
//! lumineuse ; une forêt consciente qui avance racine après racine ».
//! Palette : écorce sombre, écorce, feuillage, sève ambrée, spores turquoise.
//!
//! Là où la Coalition tient le terrain avec des fantassins tireurs et des
//! blindés, les Sylves sont lentes et organiques : mêlée en nombre (Ronciers),
//! artillerie de zone enracinée (Lanceur de spores), soutien qui soigne
//! (Sourcier), géant qui encaisse (Gardien), graines volantes d'éclaireurs
//! (Samares) et petites Pousses qui récoltent. Aucune infanterie rapide à
//! distance : c'est leur faiblesse.
//!
//! Les unités sont décrites pièce par pièce avec les formes organiques du
//! moteur (boules bosselées, tubes courbes, formes de révolution, feuillage).
//! Les coordonnées sont celles de la page de direction artistique, où
//! l'avant des unités est vers +z : chaque modèle est donc dessiné dans un
//! groupe tourné d'un demi-tour, pour que l'avant passe vers −z comme
//! l'attend le moteur.
//!
//! Les huit emplacements de matériau génériques du moteur reçoivent ici des
//! rôles organiques :
//!
//! | Emplacement | Sylves |
//! | --- | --- |
//! | `Paint` | écorce |
//! | `Dark` | écorce sombre |
//! | `Steel` | mousse |
//! | `Bare` | lichen |
//! | `Rubber` | feuillage |
//! | `Team` | couleur du joueur (feuilles, bourgeons, carapaces, ailes) |
//! | `Glow` | spores (émissif turquoise) |
//! | `Engine` | sève (émissif ambré) |

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use rts_engine::graphics::{Ambiance, EnvironmentSettings, FogSettings, ParticleSettings, SceneLight};
use rts_engine::units::{
    HoverMotion, MaterialSlot, ModelBuilder, Palette, PartShape, Pulse, SurfaceDetail, SurfaceStyle, UnitType,
};

use crate::map::GroundStyle;

// Noms des rôles des emplacements, pour que les modèles se lisent bien.
const BARK: MaterialSlot = MaterialSlot::Paint;
const DARK_BARK: MaterialSlot = MaterialSlot::Dark;
const MOSS: MaterialSlot = MaterialSlot::Steel;
const LICHEN: MaterialSlot = MaterialSlot::Bare;
const LEAVES: MaterialSlot = MaterialSlot::Rubber;
const TEAM: MaterialSlot = MaterialSlot::Team;
const SPORE: MaterialSlot = MaterialSlot::Glow;
const SAP: MaterialSlot = MaterialSlot::Engine;

/// Battement lent des poches de sève (un peu plus d'une pulsation toutes les
/// 5 secondes), comme une respiration.
const SAP_PULSE: Pulse = Pulse { min: 0.55, max: 1.35, frequency: 0.22 };
/// Les spores du Lanceur gonflent un peu plus vite.
const SPORE_PULSE: Pulse = Pulse { min: 0.6, max: 1.3, frequency: 0.29 };

/// Couleurs de la page de direction artistique.
const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}
pub const DARK_BARK_COLOR: Color = hex(0x2A, 0x1E, 0x15);
pub const BARK_COLOR: Color = hex(0x5A, 0x40, 0x30);
pub const FOLIAGE_COLOR: Color = hex(0x1B, 0x42, 0x21);
pub const SAP_COLOR: Color = hex(0xE0, 0xA3, 0x3A);
pub const SPORE_COLOR: Color = hex(0x2F, 0xE0, 0xC4);
const MOSS_COLOR: Color = hex(0x2A, 0x55, 0x2C);
const LICHEN_COLOR: Color = hex(0xB5, 0xC9, 0x8A);

/// Matériaux des Sylves. `team_color` est la couleur du joueur.
pub fn palette(team_color: Color) -> Palette {
    Palette {
        paint: SurfaceStyle { detail_size: 0.9, ..SurfaceStyle::new(BARK_COLOR, 0.0, 0.88, SurfaceDetail::Bark) },
        dark: SurfaceStyle { detail_size: 0.7, ..SurfaceStyle::new(DARK_BARK_COLOR, 0.05, 0.72, SurfaceDetail::Bark) },
        steel: SurfaceStyle::new(MOSS_COLOR, 0.0, 0.85, SurfaceDetail::Smooth),
        bare: SurfaceStyle::new(LICHEN_COLOR, 0.0, 0.7, SurfaceDetail::Smooth),
        rubber: SurfaceStyle::new(FOLIAGE_COLOR, 0.0, 0.6, SurfaceDetail::Smooth),
        team: SurfaceStyle::new(team_color, 0.0, 0.55, SurfaceDetail::Smooth),
        glow: SurfaceStyle::glowing(SPORE_COLOR, 3.0),
        engine: SurfaceStyle::glowing(SAP_COLOR, 2.6),
        // Anneau de sélection turquoise, couleur des spores.
        selection: LinearRgba::rgb(0.3, 2.0, 1.7),
    }
}

/// Ambiance des cartes des Sylves : sous-bois nocturne vert sombre, lumière
/// principale pâle, contre-jours turquoise (spores) et ambré (sève),
/// spores lumineuses qui montent du sol.
pub fn ambiance() -> Ambiance {
    let background = Color::srgb(0.022, 0.048, 0.042); // #06100B environ
    Ambiance {
        background,
        exposure: 9.6,
        sun: SceneLight { color: hex(0xDF, 0xF3, 0xEA), illuminance: 4200.0, from: Vec3::new(-0.5, 1.0, 0.55) },
        fill_lights: vec![
            // Contre-jours : turquoise à droite, ambré à gauche, comme sur la page de référence.
            SceneLight { color: SPORE_COLOR, illuminance: 2600.0, from: Vec3::new(0.9, 0.35, -0.5) },
            SceneLight { color: SAP_COLOR, illuminance: 2200.0, from: Vec3::new(-0.8, 0.45, -0.7) },
        ],
        environment: EnvironmentSettings {
            sky: Color::srgb(0.45, 0.62, 0.56),
            ground: Color::srgb(0.22, 0.15, 0.08),
            background: Color::srgb(0.02, 0.04, 0.035),
            // Écorce et mousse ne sont pas métalliques : reflets discrets.
            light_reflections: 0.6,
            intensity: 480.0,
        },
        fog: Some(FogSettings { start: 1.0, end: 2.6 }),
        bloom: 0.2,
        particles: Some(ParticleSettings { color: hex(0x5F, 0xF0, 0xD4), count: 150, glow: 3.0 }),
    }
}

/// Sol forestier : humus brun et plaques de mousse, sans grille.
pub fn ground() -> GroundStyle {
    GroundStyle {
        base: Color::srgb(0.075, 0.095, 0.055),
        dust: Color::srgb(0.17, 0.125, 0.08),
        grid_lines: false,
        roughness: 0.95,
        border: SPORE_COLOR.to_linear() * 0.8,
    }
}

/// Indices des types dans [`unit_types`], pour s'y référer par leur nom.
pub mod kind {
    pub const POUSSE: usize = 0;
    pub const RONCIER: usize = 1;
    pub const SOURCIER: usize = 2;
    pub const LANCEUR: usize = 3;
    pub const GARDIEN: usize = 4;
    pub const SAMARE: usize = 5;
}

/// Les six unités des Sylves (stats provisoires, voir GAME_DESIGN.md).
pub fn unit_types() -> Vec<UnitType> {
    vec![
        unit("Pousse", pousse(), 0.45, 0.7, 3.4),
        unit("Roncier", roncier(), 0.75, 1.5, 4.4),
        unit("Sourcier", sourcier(), 0.6, 1.5, 3.2),
        unit("Lanceur de spores", lanceur(), 1.0, 4.0, 2.2),
        unit("Gardien", gardien(), 1.9, 14.0, 2.0),
        UnitType {
            hover: Some(HoverMotion { amplitude: 0.18, frequency: 0.35 }),
            ..unit("Samare", samare(), 0.6, 0.8, 6.0)
        },
    ]
}

/// `mass` : masse relative pour les collisions, 1 pour un fantassin.
fn unit(name: &str, model: ModelBuilder, footprint_radius: f32, mass: f32, speed: f32) -> UnitType {
    UnitType { name: name.into(), parts: model.build(), footprint_radius, mass, speed, hover: None }
}

/// Raccourci pour écrire les positions.
fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// Placement « à la page de référence » : position, rotation (x, y, z en
/// radians) et échelle par axe.
fn placed(at: Vec3, rotation: Vec3, scale: Vec3) -> Transform {
    Transform::from_translation(at)
        .with_rotation(Quat::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z))
        .with_scale(scale)
}

/// Groupe tourné d'un demi-tour (avant vers −z) et mis à l'échelle `scale`.
fn facing_forward(scale: f32) -> Transform {
    Transform::from_rotation(Quat::from_rotation_y(PI)).with_scale(Vec3::splat(scale))
}

/// Formes organiques ajoutées au [`ModelBuilder`] du moteur.
///
/// Un **trait d'extension** : en Rust, on peut déclarer un trait (un
/// ensemble de méthodes) dans sa propre crate et l'implémenter pour un type
/// d'une autre crate. On ajoute ainsi des raccourcis à `ModelBuilder` sans
/// toucher au moteur ; ils passent tous par `ModelBuilder::add`.
trait OrganicParts {
    /// Boule bosselée de rayon `radius`, déformée de ± `amplitude` × rayon,
    /// étirée par `scale`.
    fn blob(&mut self, slot: MaterialSlot, radius: f32, amplitude: f32, frequency: f32, at: Vec3, scale: Vec3) -> &mut Self;
    /// Branche qui passe par `points` en s'affinant de `radius` à 70 %.
    fn branch(&mut self, slot: MaterialSlot, points: &[[f32; 3]], radius: f32) -> &mut Self;
    /// Forme de révolution d'axe y, profil (rayon, hauteur) du bas vers le haut.
    fn bulb(&mut self, slot: MaterialSlot, profile: &[[f32; 2]], transform: Transform) -> &mut Self;
    /// Touffe de `count` feuilles autour de `center`.
    fn leaves(&mut self, slot: MaterialSlot, center: Vec3, radius: f32, count: u32) -> &mut Self;
}

impl OrganicParts for ModelBuilder {
    fn blob(&mut self, slot: MaterialSlot, radius: f32, amplitude: f32, frequency: f32, at: Vec3, scale: Vec3) -> &mut Self {
        // La graine vient de la position : deux boules voisines n'ont pas les mêmes bosses.
        let seed = at.x * 3.1 + at.y * 1.7 + at.z * 2.3 + radius * 7.0;
        let shape = PartShape::Blob { radius, amplitude, frequency, seed };
        self.add(slot, shape, Transform::from_translation(at).with_scale(scale))
    }

    fn branch(&mut self, slot: MaterialSlot, points: &[[f32; 3]], radius: f32) -> &mut Self {
        let points = points.iter().map(|&p| Vec3::from_array(p)).collect();
        self.add(slot, PartShape::Tube { points, radius, radius_end: radius * 0.7 }, Transform::IDENTITY)
    }

    fn bulb(&mut self, slot: MaterialSlot, profile: &[[f32; 2]], transform: Transform) -> &mut Self {
        let profile = profile.iter().map(|&p| Vec2::from_array(p)).collect();
        self.add(slot, PartShape::Lathe { profile, segments: 16 }, transform)
    }

    fn leaves(&mut self, slot: MaterialSlot, center: Vec3, radius: f32, count: u32) -> &mut Self {
        // `as i32 as u32` : conversion d'un nombre à virgule en entier (on
        // garde juste un motif de bits stable pour la graine).
        let seed = (center.x * 131.0 + center.y * 71.0 + center.z * 37.0) as i32 as u32 ^ count;
        let shape = PartShape::Foliage { radius, count, leaf_size: 0.62, seed };
        self.add(slot, shape, Transform::from_translation(center))
    }
}

/// Gardien : arbre géant qui marche. Grand tronc bosselé, cage de côtes sur
/// une poche de sève, longs bras-branches, couronne de feuillage où pointent
/// les bourgeons et une touffe aux couleurs du joueur (lisibles de dessus).
fn gardien() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(facing_forward(0.72), |b| {
        let one = Vec3::ONE;
        b.blob(BARK, 1.0, 0.16, 1.6, v(0.0, 3.7, 0.0), v(1.15, 1.9, 1.0)) // tronc
            .blob(DARK_BARK, 0.9, 0.2, 1.8, v(0.0, 4.9, -0.25), v(1.5, 0.9, 1.0)) // épaules
            .pulsing(SAP_PULSE, |b| {
                b.blob(SAP, 0.5, 0.06, 2.0, v(0.0, 3.95, 0.78), v(1.0, 1.2, 0.7)); // poche de sève
            });
        // Côtes d'écorce par-dessus la poche de sève.
        for i in 0..4 {
            let y = 3.45 + i as f32 * 0.3;
            b.branch(
                DARK_BARK,
                &[[-0.95, y + 0.1, 0.35], [-0.5, y, 0.98], [0.0, y - 0.05, 1.16], [0.5, y, 0.98], [0.95, y + 0.1, 0.35]],
                0.07,
            );
        }
        b.mirrored(|b, s| {
            b.blob(BARK, 0.75, 0.2, 1.7, v(s * 1.45, 5.0, 0.0), v(1.1, 0.85, 1.0)); // épaule
            for k in 0..3 {
                let k = k as f32;
                // Épines d'épaule.
                b.cone(DARK_BARK, 0.13, 0.0, 1.0 - k * 0.15, v(s * (1.3 + k * 0.35), 5.75 - k * 0.12, -0.1 + k * 0.1), v(0.1 * k, 0.0, -s * (0.25 + k * 0.3)));
            }
            b.branch(BARK, &[[s * 1.5, 4.9, 0.0], [s * 2.3, 3.9, 0.3], [s * 2.4, 2.6, 0.7], [s * 2.1, 1.6, 1.1]], 0.28) // bras
                .blob(DARK_BARK, 0.42, 0.15, 2.0, v(s * 2.1, 1.5, 1.12), one); // main
            for c in 0..3 {
                let c = c as f32 - 1.0;
                // Griffes de lichen.
                b.cone(LICHEN, 0.08, 0.0, 0.7, v(s * 2.1 + c * 0.22, 1.0, 1.25), v(PI - 0.3, 0.0, c * 0.3));
            }
            b.blob(MOSS, 0.4, 0.3, 2.2, v(s * 2.45, 3.2, 0.45), one) // mousse sur le bras
                .branch(BARK, &[[s * 0.6, 2.3, 0.0], [s * 1.05, 1.3, 0.35], [s * 1.1, 0.2, 0.1]], 0.38) // jambe
                .blob(DARK_BARK, 0.5, 0.15, 2.0, v(s * 1.1, 0.25, 0.15), v(1.0, 0.6, 1.3)); // pied
            // Racines-orteils.
            for (dx, dz) in [(0.9, 0.9), (1.2, -0.2), (0.1, 1.3)] {
                b.branch(
                    DARK_BARK,
                    &[[s * 1.1, 0.25, 0.15], [s * (1.1 + dx * 0.5), 0.2, 0.15 + dz * 0.5], [s * (1.1 + dx), 0.04, 0.15 + dz]],
                    0.12,
                );
            }
            b.sphere(SAP, 0.08, v(s * 0.17, 5.72, 0.95)) // œil
                .branch(DARK_BARK, &[[s * 0.3, 5.9, 0.2], [s * 0.8, 6.6, 0.0], [s * 1.5, 7.0, -0.2], [s * 1.8, 7.6, -0.1]], 0.12)
                .branch(DARK_BARK, &[[s * 0.8, 6.6, 0.0], [s * 0.7, 7.2, 0.3], [s * 0.9, 7.7, 0.4]], 0.08)
                .blob(MOSS, 0.6, 0.28, 2.2, v(s * 1.85, 7.75, -0.1), one) // mousse de la couronne
                .blob(TEAM, 0.42, 0.28, 2.4, v(s * 0.9, 7.85, 0.4), one) // bourgeon d'équipe
                // Veine de sève qui court sur le tronc.
                .branch(SAP, &[[s * 0.35, 2.6, 0.92], [s * 0.5, 3.3, 1.02], [s * 0.8, 4.4, 0.82], [s * 0.6, 5.2, 0.75]], 0.04);
        });
        b.blob(DARK_BARK, 0.48, 0.14, 2.0, v(0.0, 5.65, 0.5), v(1.0, 1.1, 1.1)) // tête
            .blob(MOSS, 1.0, 0.28, 2.0, v(0.0, 5.7, -0.9), v(1.4, 0.8, 1.0)); // dos moussu
        // Le feuillage : environ 280 feuilles (2 200 sommets), une touffe aux
        // couleurs du joueur au sommet.
        b.leaves(LEAVES, v(1.85, 7.75, -0.1), 0.95, 40)
            .leaves(LEAVES, v(-1.85, 7.75, -0.1), 0.95, 40)
            .leaves(TEAM, v(0.0, 7.4, -0.3), 0.9, 36)
            .leaves(LEAVES, v(0.9, 7.85, 0.4), 0.65, 20)
            .leaves(LEAVES, v(-0.9, 7.85, 0.4), 0.65, 20)
            .leaves(LEAVES, v(0.0, 5.9, -1.0), 1.35, 56)
            .leaves(LEAVES, v(1.5, 5.5, -0.4), 0.7, 20)
            .leaves(LEAVES, v(-1.5, 5.5, -0.4), 0.7, 20)
            .leaves(LEAVES, v(2.45, 3.2, 0.45), 0.5, 13)
            .leaves(LEAVES, v(-2.45, 3.2, 0.45), 0.5, 13);
    });
    b
}

/// Roncier : fantassin de mêlée, bas et trapu sur quatre pattes en racines,
/// dos couvert de carapaces d'écorce (celle du milieu aux couleurs du
/// joueur), bras en crochets de lichen, abdomen gorgé de sève.
fn roncier() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(facing_forward(0.8), |b| {
        b.blob(BARK, 0.5, 0.16, 1.8, v(0.0, 1.15, 0.0), v(1.0, 1.1, 1.5)); // corps
        // Trois carapaces en coupole, de l'avant vers l'arrière.
        for i in 0..3 {
            let i = i as f32;
            let r = 0.62 - i * 0.06;
            // Calotte de sphère : un quart de cercle (un peu plus) qui tourne autour de y.
            let profile: Vec<[f32; 2]> = (0..=5)
                .map(|k| {
                    let theta = 0.45 * PI * (1.0 - k as f32 / 5.0);
                    [r * theta.sin(), r * theta.cos()]
                })
                .collect();
            let slot = if i == 1.0 { TEAM } else { DARK_BARK };
            b.bulb(slot, &profile, placed(v(0.0, 1.3 - i * 0.02, 0.35 - i * 0.42), v(-0.5 - i * 0.25, 0.0, 0.0), v(1.0, 0.8, 1.1)));
        }
        b.mirrored(|b, s| {
            for k in 0..2 {
                // Pattes en racines arquées.
                let z = 0.35 - k as f32 * 0.7;
                let (mid, foot) = if k == 0 { (0.3, 0.5) } else { (-0.3, -0.5) };
                b.branch(DARK_BARK, &[[s * 0.35, 1.0, z], [s * 1.0, 1.5, z + mid], [s * 1.25, 0.05, z + foot]], 0.08);
            }
            b.branch(BARK, &[[s * 0.3, 1.3, 0.6], [s * 0.6, 1.9, 1.0], [s * 0.45, 1.7, 1.6]], 0.09) // bras
                .cone(LICHEN, 0.1, 0.0, 0.9, v(s * 0.42, 1.5, 1.95), v(2.0, 0.0, 0.0)) // crochet
                .sphere(SAP, 0.07, v(s * 0.16, 1.35, 0.7)); // œil
            for j in 0..3 {
                let j = j as f32;
                // Épines dorsales.
                b.cone(DARK_BARK, 0.08, 0.0, 0.6, v(s * (0.25 + j * 0.08), 1.78 - j * 0.05, 0.3 - j * 0.45), v(-0.4 - j * 0.2, 0.0, -s * 0.5));
            }
        });
        b.pulsing(SAP_PULSE, |b| {
            b.blob(SAP, 0.26, 0.05, 2.0, v(0.0, 0.95, -0.75), v(1.0, 0.9, 1.2)); // abdomen de sève
        });
    });
    b
}

/// Lanceur de spores : artillerie lente. Cinq racines en étoile, tronc
/// tordu, énorme cosse de mousse piquée de spores lumineuses et couronnée
/// d'épines aux couleurs du joueur (vue de dessus : une étoile et un disque).
fn lanceur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(facing_forward(0.85), |b| {
        for i in 0..5 {
            let a = i as f32 * TAU / 5.0;
            let (sin, cos) = a.sin_cos();
            b.branch(DARK_BARK, &[[0.0, 0.5, 0.0], [sin * 0.6, 0.25, cos * 0.6], [sin * 1.3, 0.03, cos * 1.3]], 0.13);
        }
        b.branch(BARK, &[[0.0, 0.3, 0.0], [-0.15, 1.2, 0.05], [0.1, 2.1, 0.1], [0.45, 2.8, 0.3]], 0.24); // tronc
        b.group(placed(v(0.55, 3.45, 0.4), v(0.25, 0.0, -0.25), Vec3::ONE), |pod| {
            // La cosse, fermée en haut pour qu'on ne voie pas au travers.
            pod.bulb(
                MOSS,
                &[[0.0, -0.7], [0.45, -0.55], [0.78, -0.1], [0.72, 0.4], [0.42, 0.75], [0.3, 0.85], [0.0, 0.9]],
                Transform::IDENTITY,
            );
            for i in 0..5 {
                let a = i as f32 * TAU / 5.0;
                let (sin, cos) = a.sin_cos();
                // Épines de la couronne, aux couleurs du joueur.
                pod.cone(TEAM, 0.2, 0.0, 0.7, v(sin * 0.36, 1.05, cos * 0.36), v(cos * 0.6, 0.0, -sin * 0.6));
            }
            pod.pulsing(SPORE_PULSE, |pod| {
                for i in 0..7 {
                    let a = i as f32 * 0.9;
                    pod.sphere(SPORE, 0.14, v(a.sin() * 0.74, -0.3 + (i % 3) as f32 * 0.3, a.cos() * 0.74));
                }
                pod.sphere(SPORE, 0.26, v(0.0, 0.8, 0.0));
            });
        });
        b.leaves(LEAVES, v(0.5, 2.75, 0.35), 0.55, 18)
            .leaves(LEAVES, v(0.0, 0.35, 0.0), 0.75, 16)
            .leaves(LEAVES, v(-0.1, 1.5, 0.05), 0.35, 8);
    });
    b
}

/// Sourcier : soutien qui soigne. Petit tronc bossu sur quatre racines,
/// portant sur le dos une grosse lanterne de sève tenue par des côtes
/// d'écorce et entourée de pétales aux couleurs du joueur (vue de dessus :
/// un disque ambré cerclé de pétales).
fn sourcier() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(facing_forward(1.0), |b| {
        b.mirrored(|b, s| {
            for z in [0.25, -0.25] {
                b.branch(DARK_BARK, &[[s * 0.2, 0.75, z], [s * 0.55, 0.6, z * 1.6], [s * 0.65, 0.03, z * 2.2]], 0.08);
            }
            b.sphere(SAP, 0.05, v(s * 0.12, 1.32, 0.62)) // œil
                // Bras qui tient un bâton de lichen.
                .branch(BARK, &[[s * 0.35, 1.15, 0.2], [s * 0.55, 0.95, 0.45], [s * 0.5, 0.7, 0.65]], 0.07);
        });
        b.blob(BARK, 0.45, 0.16, 1.8, v(0.0, 0.95, 0.05), v(1.0, 1.15, 1.0)) // corps
            .blob(DARK_BARK, 0.26, 0.14, 2.0, v(0.0, 1.3, 0.45), Vec3::ONE) // tête
            .pulsing(SAP_PULSE, |b| {
                b.blob(SAP, 0.42, 0.05, 2.0, v(0.0, 1.62, -0.3), Vec3::ONE); // lanterne de sève
            });
        // Trois côtes d'écorce qui tiennent la lanterne.
        for dz in [-0.2, 0.0, 0.2] {
            let r = (0.48f32 * 0.48 - dz * dz).sqrt();
            let z = -0.3 + dz;
            b.branch(
                DARK_BARK,
                &[[-r, 1.45, z], [-r * 0.75, 1.62 + r * 0.6, z], [0.0, 1.64 + r, z], [r * 0.75, 1.62 + r * 0.6, z], [r, 1.45, z]],
                0.045,
            );
        }
        // Collerette de pétales autour de la lanterne.
        for i in 0..7 {
            let a = i as f32 * TAU / 7.0;
            let (sin, cos) = a.sin_cos();
            b.cone(TEAM, 0.17, 0.0, 0.7, v(sin * 0.58, 1.42, -0.3 + cos * 0.58), v(cos * 1.25, 0.0, -sin * 1.25));
        }
        b.leaves(LEAVES, v(0.0, 1.1, -0.1), 0.5, 14);
    });
    b
}

/// Pousse : l'ouvrière. Petit bulbe de mousse sur trois radicelles, deux
/// feuilles dressées aux couleurs du joueur, un œil de sève.
fn pousse() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(facing_forward(1.6), |b| {
        b.bulb(MOSS, &[[0.0, 0.0], [0.2, 0.05], [0.27, 0.25], [0.18, 0.48], [0.0, 0.55]], Transform::from_xyz(0.0, 0.04, 0.0));
        b.mirrored(|b, s| {
            b.cone(TEAM, 0.08, 0.0, 0.45, v(s * 0.12, 0.68, 0.0), v(0.0, 0.0, -s * 0.6));
        });
        for i in 0..3 {
            let a = i as f32 * TAU / 3.0 + 0.5;
            let (sin, cos) = a.sin_cos();
            b.branch(DARK_BARK, &[[sin * 0.15, 0.12, cos * 0.15], [sin * 0.3, 0.06, cos * 0.3], [sin * 0.4, 0.01, cos * 0.4]], 0.04);
        }
        b.sphere(SAP, 0.06, v(0.0, 0.3, 0.25));
    });
    b
}

/// Samare : graine ailée qui plane, éclaireur et harceleur rapide. Deux
/// grandes ailes aux couleurs du joueur nervurées d'écorce, une graine
/// sombre au cœur de spore, des radicelles qui pendent. Elle vole à
/// 1,4 m du sol.
fn samare() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(Transform::from_xyz(0.0, 1.4, 0.0), |b| {
        b.blob(DARK_BARK, 0.28, 0.14, 2.0, Vec3::ZERO, v(1.0, 0.8, 1.3)) // graine
            .sphere(SPORE, 0.11, v(0.0, -0.2, 0.0)); // cœur de spore
        // L'aile : un profil convexe (z, y) en forme de lame, extrudé sur
        // 4 cm puis couché à plat et légèrement relevé (dièdre).
        let wing = [
            Vec2::new(-0.06, 0.0),
            Vec2::new(0.06, 0.0),
            Vec2::new(0.38, 0.7),
            Vec2::new(0.4, 1.05),
            Vec2::new(0.25, 1.3),
            Vec2::new(0.0, 1.25),
            Vec2::new(-0.12, 0.9),
        ];
        b.mirrored(|b, s| {
            let lay_flat = Transform::from_xyz(s * 0.12, 0.02, 0.0)
                .with_rotation(Quat::from_rotation_z(-s * (PI / 2.0 - 0.12)) * Quat::from_rotation_y(-s * 0.25));
            b.group(lay_flat, |b| {
                b.add(TEAM, PartShape::Prism { profile: wing.to_vec(), width: 0.04 }, Transform::IDENTITY);
                // Nervure principale, un peu au-dessus de l'aile.
                b.branch(DARK_BARK, &[[0.03, 0.05, -0.1], [0.03, 0.6, 0.12], [0.03, 1.2, 0.3]], 0.035);
            });
        });
        for i in 0..3 {
            let a = i as f32 * TAU / 3.0;
            let (sin, cos) = a.sin_cos();
            b.branch(LICHEN, &[[sin * 0.08, -0.2, cos * 0.08], [sin * 0.15, -0.5, cos * 0.15], [sin * 0.1, -0.8, cos * 0.12]], 0.025);
        }
    });
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Garde un œil sur le coût des modèles : nombre de sommets par type.
    #[test]
    fn units_stay_light() {
        for unit in unit_types() {
            let vertices: usize = unit.parts.iter().map(|p| p.shape.mesh().count_vertices()).sum();
            println!("{} : {} sommets", unit.name, vertices);
            assert!(vertices < 14_000, "{} est trop lourd : {vertices} sommets", unit.name);
        }
    }
}
