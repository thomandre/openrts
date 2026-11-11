//! Les Sylves : palette, ambiance, unités, bâtiments et économie (faction en
//! exploration).
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
//!
//! Construction : **la Pousse devient le bâtiment**. Elle se rend à
//! l'emplacement, s'y enracine et disparaît ; le chantier pousse ensuite seul
//! (style `WorkerConsumed` du moteur, voir [`CONSTRUCTION`]).

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use rts_engine::buildings::{BuildingType, ConstructionStyle, Cost, PlacementRule};
use rts_engine::economy::production::{ProductionOption, Requirement, UnitEconomy};
use rts_engine::economy::{EconomyRules, ResourceInfo, ResourceKind};
use rts_engine::graphics::{Ambiance, EnvironmentSettings, FogSettings, ParticleSettings, SceneLight};
use rts_engine::units::{
    HoverMotion, MaterialSlot, ModelBuilder, Motion, Palette, PartShape, Pulse, SurfaceDetail, SurfaceStyle, UnitType,
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

// ---------------------------------------------------------------------------
// Les bâtiments (palier 1, propositions : voir GAME_DESIGN.md).
// ---------------------------------------------------------------------------

/// Façon de construire des Sylves : la Pousse se transforme en chantier, qui
/// pousse seul ; annulé, il rend 75 % de son coût et la Pousse.
pub const CONSTRUCTION: ConstructionStyle = ConstructionStyle::WorkerConsumed { refund_percent: 75 };

/// Indices des bâtiments dans [`building_types`].
pub mod building {
    pub const ARBRE_MERE: usize = 0;
    pub const SOUCHE: usize = 1;
    pub const CALICE: usize = 2;
    pub const RONCERAIE: usize = 3;
    pub const EPINEUX: usize = 4;
}

/// Les bâtiments du palier 1, mêmes rôles que ceux de la Coalition :
///
/// - **Arbre-mère** (7 × 7 m) : le cœur de la base, dépôt des récoltes,
///   produit les Pousses, 15 de population ;
/// - **Souche nourricière** (3 × 3 m) : 8 de population ;
/// - **Calice** (4 × 4 m) : fleur géante posée sur une source d'Éther, qui
///   le boit ; les Pousses le récoltent à son pied ;
/// - **Ronceraie** (5 × 5 m) : roncier en voûte où mûrissent les Ronciers ;
/// - **Épineux** (2 × 2 m) : défense fixe, débloquée par la Ronceraie.
///
/// Chiffres proposés, à ajuster en test. Chaque bâtiment coûte aussi une
/// Pousse (50 de Cristal et 12 s de production) : ses prix en Cristal sont
/// donc un peu plus bas que ceux de la Coalition, dont l'ouvrier survit.
pub fn building_types() -> Vec<BuildingType> {
    use building::*;
    vec![
        BuildingType {
            dropoff: true,
            supply: 15,
            production: vec![option(kind::POUSSE, 'p', &[])],
            ..building_type("Arbre-mère", (7, 7), 1500.0, 75.0, (350, 0), &[], 'a', arbre_mere())
        },
        BuildingType { supply: 8, ..building_type("Souche nourricière", (3, 3), 350.0, 20.0, (75, 0), &[], 's', souche()) },
        BuildingType {
            placement: PlacementRule::OnResource(ResourceKind::Rare),
            ..building_type("Calice", (4, 4), 450.0, 22.0, (50, 0), &[], 'c', calice())
        },
        // La Ronceraie fait mûrir les Ronciers ; Sourcier et Lanceur de
        // spores attendent des bâtiments du palier 2 pas encore dans le jeu.
        BuildingType {
            production: vec![
                option(kind::RONCIER, 'r', &[]),
                option(kind::SOURCIER, 's', &["Fontaine de sève"]),
                option(kind::LANCEUR, 'l', &["Champignonnière"]),
            ],
            ..building_type("Ronceraie", (5, 5), 1000.0, 45.0, (125, 0), &[ARBRE_MERE], 'r', ronceraie())
        },
        building_type("Épineux", (2, 2), 300.0, 22.0, (100, 0), &[RONCERAIE], 'e', epineux()),
    ]
}

/// Règles économiques des Sylves : les Pousses récoltent, avec les mêmes
/// cadences que les Techniciens pour l'instant (5 de Cristal en 2,8 s, 4
/// d'Éther en 2 s, au pied du Calice). Coûts, temps de production (en
/// secondes) et population proposés, à ajuster en test.
pub fn economy_rules(resources: [ResourceInfo; 2]) -> EconomyRules {
    // Dans l'ordre de `unit_types` : (Cristal, Éther), temps, population.
    let units = [
        ((50, 0), 12.0, 1), // Pousse
        ((50, 0), 16.0, 1), // Roncier
        ((75, 25), 22.0, 2), // Sourcier
        ((125, 50), 30.0, 3), // Lanceur de spores
        ((250, 150), 55.0, 6), // Gardien
        ((75, 25), 20.0, 2), // Samare
    ];
    EconomyRules {
        resources,
        harvesters: vec![kind::POUSSE],
        carry: [5, 4],
        gather_time: [2.8, 2.0],
        units: units
            .iter()
            .map(|&((primary, rare), train_time, population)| UnitEconomy { cost: Cost::new(primary, rare), train_time, population })
            .collect(),
    }
}

/// Une unité produite par un bâtiment ; `requires` : noms de bâtiments qui
/// n'existent pas encore dans le jeu (l'option reste grisée).
fn option(unit_type: usize, hotkey: char, requires: &[&str]) -> ProductionOption {
    ProductionOption {
        unit_type,
        hotkey,
        requires: requires.iter().map(|name| Requirement::NotYetAvailable((*name).into())).collect(),
    }
}

/// `cost` : (Cristal, Éther).
fn building_type(
    name: &str,
    footprint: (u32, u32),
    max_health: f32,
    build_time: f32,
    cost: (u32, u32),
    requires: &[usize],
    hotkey: char,
    model: ModelBuilder,
) -> BuildingType {
    BuildingType {
        name: name.into(),
        footprint: UVec2::new(footprint.0, footprint.1),
        max_health,
        build_time,
        cost: Cost::new(cost.0, cost.1),
        requires: requires.to_vec(),
        hotkey,
        parts: model.build(),
        ..Default::default()
    }
}

/// Épine ou pétale : un cône dont la base est en `base` et la pointe dans la
/// direction `dir`, long de `length`.
fn spike(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, length: f32, base: Vec3, dir: Vec3) {
    let dir = dir.normalize();
    // Le cône du moteur est centré sur son origine, pointe vers +y : on le
    // tourne vers `dir` et on le recule d'une demi-longueur.
    let transform = Transform::from_translation(base + dir * length / 2.0).with_rotation(Quat::from_rotation_arc(Vec3::Y, dir));
    b.add(slot, PartShape::Frustum { radius_bottom: radius, radius_top: 0.0, height: length }, transform);
}

/// Racines qui rayonnent autour d'un pied : `count` racines de `from` (rayon
/// au départ, hauteur) à `to` (rayon à l'arrivée, sous le sol).
fn roots(b: &mut ModelBuilder, count: u32, from: (f32, f32), to: f32, thickness: f32, turn: f32) {
    for i in 0..count {
        let a = i as f32 * TAU / count as f32 + turn;
        let (sin, cos) = a.sin_cos();
        let at = |r: f32, y: f32| [cos * r, y, sin * r];
        let mid = (from.0 + to) / 2.0;
        b.branch(DARK_BARK, &[at(from.0, from.1), at(mid, from.1 * 0.55), at(to, -0.06)], thickness);
    }
}

/// Champignon : pied de lichen et chapeau aux couleurs du joueur, posé en
/// `at`, de taille `scale` (1 : chapeau de 60 cm de rayon).
fn mushroom(b: &mut ModelBuilder, at: Vec3, scale: f32) {
    b.group(Transform::from_translation(at).with_scale(Vec3::splat(scale)), |b| {
        b.bulb(LICHEN, &[[0.0, 0.0], [0.16, 0.0], [0.12, 0.3], [0.13, 0.6], [0.0, 0.62]], Transform::IDENTITY);
        // Chapeau : bord retroussé, dessus bombé.
        b.bulb(TEAM, &[[0.0, 0.52], [0.5, 0.55], [0.62, 0.64], [0.5, 0.8], [0.25, 0.9], [0.0, 0.93]], Transform::IDENTITY);
    });
}

/// Arbre-mère : le cœur de la base, le plus grand bâtiment (6,5 m de haut,
/// assez bas pour ne pas cacher la ligne de Cristal derrière lui).
/// Une butte d'humus tachée de mousse et sept grosses racines qui tiennent toute
/// l'empreinte ; un tronc évasé creusé en façade d'une grande poche de sève
/// (le dépôt, où les Pousses versent leur récolte), qui respire ; cinq
/// branches maîtresses chargées de feuillage, des lanternes de spores
/// pendues dessous, et au sommet une couronne de feuilles et cinq bourgeons
/// aux couleurs du joueur. Vu de dessus : une étoile de racines sous un
/// disque de feuillage cerclé de bourgeons d'équipe.
fn arbre_mere() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.blob(DARK_BARK, 1.9, 0.14, 1.6, Vec3::ZERO, v(1.0, 0.25, 1.0)); // butte d'humus
    for (x, z) in [(1.6, 0.9), (-1.4, 1.3), (0.4, -1.9)] {
        b.blob(MOSS, 0.5, 0.3, 2.2, v(x, 0.15, z), v(1.0, 0.45, 1.0)); // plaques de mousse
    }
    roots(&mut b, 7, (1.0, 1.1), 3.3, 0.34, 0.2);
    // Le tronc : profil (rayon, hauteur), évasé au pied et à la fourche.
    b.bulb(BARK, &[[1.55, 0.0], [1.3, 0.6], [1.0, 1.5], [0.85, 2.5], [0.9, 3.2], [1.15, 3.8], [0.6, 4.2], [0.0, 4.3]], Transform::IDENTITY);
    // La poche de sève en façade (−z), dans un creux bordé d'écorce sombre.
    b.pulsing(SAP_PULSE, |b| {
        b.blob(SAP, 0.55, 0.06, 2.0, v(0.0, 1.55, -0.92), v(1.0, 1.35, 0.55));
    });
    b.branch(
        DARK_BARK,
        &[[-0.62, 0.85, -1.0], [-0.72, 1.6, -0.95], [-0.42, 2.3, -0.88], [0.0, 2.45, -0.86], [0.42, 2.3, -0.88], [0.72, 1.6, -0.95], [0.62, 0.85, -1.0]],
        0.13,
    );
    // Veines de sève qui montent dans l'écorce.
    b.branch(SAP, &[[0.95, 0.9, -0.7], [0.8, 1.9, -0.55], [0.75, 2.8, -0.45], [0.95, 3.6, -0.5]], 0.05);
    b.branch(SAP, &[[-0.85, 1.4, 0.8], [-0.75, 2.4, 0.55], [-0.95, 3.4, 0.6]], 0.05);
    // Cinq branches maîtresses, du feuillage au bout de chacune.
    for i in 0..5 {
        let a = i as f32 * TAU / 5.0 + 0.95;
        let (sin, cos) = a.sin_cos();
        let at = |r: f32, y: f32| v(cos * r, y, sin * r);
        b.branch(BARK, &[at(0.5, 3.7).to_array(), at(1.4, 4.3).to_array(), at(2.2, 4.7).to_array(), at(2.6, 4.95).to_array()], 0.3);
        b.leaves(LEAVES, at(2.55, 5.1), 1.2, 32);
        b.blob(TEAM, 0.32, 0.25, 2.4, at(2.6, 5.85), Vec3::ONE); // bourgeon d'équipe
        if i % 2 == 0 {
            // Lanterne de spores pendue sous la branche.
            b.branch(LICHEN, &[at(1.7, 4.55).to_array(), at(1.75, 3.95).to_array()], 0.025);
            b.sphere(SPORE, 0.16, at(1.75, 3.8));
        }
    }
    b.blob(MOSS, 0.65, 0.3, 2.2, v(0.75, 3.75, 0.55), Vec3::ONE); // mousse à la fourche
    // Couronne aux couleurs du joueur, au sommet.
    b.leaves(TEAM, v(0.0, 5.4, 0.0), 1.3, 44);
    b.leaves(LEAVES, v(0.0, 4.8, 0.0), 1.6, 34);
    b
}

/// Souche nourricière : une large souche coupée, évasée sur ses racines,
/// au cerne pâle, coiffée de trois champignons aux couleurs du joueur ; une
/// fente de sève lumineuse en façade. Vu de dessus : un disque et trois
/// chapeaux d'équipe (rien à voir avec la couronne de l'Arbre-mère).
fn souche() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    roots(&mut b, 5, (1.05, 0.45), 1.55, 0.16, 0.4);
    b.bulb(BARK, &[[1.3, 0.0], [1.2, 0.3], [0.98, 0.8], [0.92, 1.3], [0.98, 1.5], [0.86, 1.56], [0.0, 1.5]], Transform::IDENTITY);
    b.cylinder(LICHEN, 0.84, 0.04, v(0.0, 1.53, 0.0), Vec3::ZERO); // cerne
    b.cylinder(DARK_BARK, 0.5, 0.05, v(0.0, 1.545, 0.0), Vec3::ZERO);
    mushroom(&mut b, v(0.18, 1.5, 0.15), 1.0);
    mushroom(&mut b, v(-0.5, 1.5, -0.3), 0.68);
    mushroom(&mut b, v(0.5, 1.5, -0.5), 0.48);
    b.pulsing(SAP_PULSE, |b| {
        b.blob(SAP, 0.2, 0.06, 2.0, v(0.0, 0.75, -1.0), v(0.8, 1.6, 0.5)); // fente de sève
    });
    b.blob(MOSS, 0.45, 0.3, 2.2, v(-0.9, 0.5, 0.55), v(1.0, 0.7, 1.0));
    b
}

/// Calice : une fleur géante qui boit l'Éther de la source qu'elle couvre.
/// Six racines plongent dans la faille, une tige trapue porte une grande
/// coupe de mousse ouverte, pleine d'un nectar de spores qui palpite, et
/// cerclée de sept grands pétales aux couleurs du joueur. Vu de dessus : un
/// disque turquoise dans une étoile de pétales d'équipe.
fn calice() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    // Racines arquées par-dessus la margelle de la source, jusqu'au fond.
    for i in 0..6 {
        let a = i as f32 * TAU / 6.0 + 0.25;
        let (sin, cos) = a.sin_cos();
        let at = |r: f32, y: f32| [cos * r, y, sin * r];
        b.branch(DARK_BARK, &[at(0.4, 1.2), at(1.3, 1.0), at(1.85, 0.5), at(1.95, -0.05)], 0.17);
    }
    b.bulb(BARK, &[[0.7, 0.5], [0.45, 0.9], [0.38, 1.5], [0.5, 2.0], [0.0, 2.05]], Transform::IDENTITY); // tige
    // La coupe : le profil monte jusqu'au bord, puis redescend à l'intérieur.
    b.bulb(
        MOSS,
        &[[0.0, 1.85], [0.55, 1.95], [1.05, 2.3], [1.38, 2.8], [1.48, 3.05], [1.35, 3.04], [1.0, 2.7], [0.5, 2.55], [0.0, 2.5]],
        Transform::IDENTITY,
    );
    b.pulsing(SPORE_PULSE, |b| {
        b.cylinder(SPORE, 1.02, 0.04, v(0.0, 2.68, 0.0), Vec3::ZERO); // nectar de spores
    });
    for i in 0..7 {
        let a = i as f32 * TAU / 7.0;
        let (sin, cos) = a.sin_cos();
        let out = v(cos, 0.0, sin);
        spike(&mut b, TEAM, 0.34, 1.15, out * 1.32 + v(0.0, 2.85, 0.0), out + v(0.0, 0.9, 0.0));
    }
    for (x, y, z) in [(0.3, 3.4, -0.2), (-0.35, 3.7, 0.15), (0.05, 4.0, 0.35)] {
        b.sphere(SPORE, 0.08, v(x, y, z)); // spores qui s'envolent
    }
    b.leaves(LEAVES, v(0.0, 1.0, 0.0), 0.75, 14);
    b
}

/// Ronceraie : trois grandes arches de ronces hérissées d'épines se croisent
/// en voûte au-dessus d'une butte d'humus ; dessous mûrissent trois cocons
/// aux couleurs du joueur (les Ronciers à naître) près d'une poche de sève.
/// L'entrée, en façade, passe entre deux pieds d'arche. Vu de dessus : une
/// étoile de ronces à six branches sur trois cocons d'équipe.
fn ronceraie() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.blob(DARK_BARK, 1.5, 0.16, 1.6, Vec3::ZERO, v(1.0, 0.3, 1.0)); // butte d'humus
    b.blob(MOSS, 0.55, 0.3, 2.2, v(-1.0, 0.15, -0.6), v(1.0, 0.5, 1.0));
    // Arches à 0°, 60° et 120° : leurs pieds encadrent l'entrée (vers −z).
    for i in 0..3 {
        let a = i as f32 * PI / 3.0;
        let (sin, cos) = a.sin_cos();
        let at = |r: f32, y: f32| v(cos * r, y, sin * r);
        let profile = [(2.25, -0.05), (1.95, 1.4), (1.15, 2.6), (0.0, 3.0), (-1.15, 2.6), (-1.95, 1.4), (-2.25, -0.05)];
        let points: Vec<[f32; 3]> = profile.iter().map(|&(r, y)| at(r, y).to_array()).collect();
        b.branch(BARK, &points, 0.22);
        // Épines tournées vers l'extérieur et vers le haut.
        for &(r, y) in &[(1.9f32, 1.5), (1.2, 2.55), (-1.2, 2.55), (-1.9, 1.5)] {
            let out = at(r.signum(), 0.0) + v(0.0, 0.9, 0.0);
            spike(&mut b, DARK_BARK, 0.08, 0.5, at(r, y + 0.1), out);
        }
    }
    b.leaves(LEAVES, v(0.0, 3.05, 0.0), 0.7, 18); // feuillage au faîte
    // Trois cocons d'équipe sous la voûte, et la poche de sève qui les nourrit.
    for (x, z, scale) in [(-0.75, 0.25, 1.0), (0.75, 0.3, 0.9), (0.0, 0.95, 1.1)] {
        b.blob(TEAM, 0.45, 0.16, 2.2, v(x, 0.8, z), v(0.8 * scale, 1.15 * scale, 0.8 * scale));
    }
    b.pulsing(SAP_PULSE, |b| {
        b.blob(SAP, 0.35, 0.06, 2.0, v(0.0, 0.55, 1.55), v(1.2, 0.8, 0.9));
    });
    b
}

/// Épineux : défense fixe. Un bulbe d'écorce sombre sur ses racines, et une
/// tête qui balaie lentement l'horizon : couronne de longues épines de
/// lichen, collerette de pétales aux couleurs du joueur, œil de spores et
/// grand dard pointé vers l'avant.
fn epineux() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    roots(&mut b, 5, (0.45, 0.35), 0.95, 0.1, 0.0);
    b.blob(DARK_BARK, 0.62, 0.15, 1.8, v(0.0, 0.5, 0.0), v(1.0, 0.85, 1.0));
    b.bulb(BARK, &[[0.32, 0.9], [0.24, 1.3], [0.3, 1.5], [0.0, 1.55]], Transform::IDENTITY); // cou
    let sweep = Motion::Swing { axis: Vec3::Y, amplitude: 0.6, frequency: 0.08 };
    b.animated(Transform::from_xyz(0.0, 1.5, 0.0), sweep, |b| {
        b.blob(BARK, 0.38, 0.12, 2.0, v(0.0, 0.15, 0.0), v(1.0, 0.75, 1.1)); // tête
        for i in 0..6 {
            let a = i as f32 * TAU / 6.0 + 0.3;
            let (sin, cos) = a.sin_cos();
            let out = v(cos, 0.0, sin);
            spike(b, TEAM, 0.16, 0.5, out * 0.3 + v(0.0, 0.05, 0.0), out + v(0.0, 0.35, 0.0)); // collerette
            spike(b, LICHEN, 0.06, 0.85, out * 0.22 + v(0.0, 0.3, 0.0), out * 0.6 + v(0.0, 1.0, 0.0)); // épines
        }
        spike(b, DARK_BARK, 0.1, 0.9, v(0.0, 0.15, -0.3), v(0.0, 0.05, -1.0)); // dard
        b.sphere(SPORE, 0.12, v(0.0, 0.32, -0.3)); // œil
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

    /// Même garde-fou pour les bâtiments : ils sont peu nombreux mais grands.
    #[test]
    fn buildings_stay_light() {
        for building in building_types() {
            let vertices: usize = building.parts.iter().map(|p| p.shape.mesh().count_vertices()).sum();
            println!("{} : {} sommets", building.name, vertices);
            assert!(vertices < 14_000, "{} est trop lourd : {vertices} sommets", building.name);
        }
    }

    /// Le palier 1 a les mêmes rôles que celui de la Coalition.
    #[test]
    fn tier_one_roles() {
        use building::*;
        let types = building_types();
        assert!(types[ARBRE_MERE].dropoff && types[ARBRE_MERE].supply > 0);
        assert_eq!(types[ARBRE_MERE].production[0].unit_type, kind::POUSSE);
        assert!(types[SOUCHE].supply > 0);
        assert_eq!(types[CALICE].placement, PlacementRule::OnResource(ResourceKind::Rare));
        assert_eq!(types[RONCERAIE].production[0].unit_type, kind::RONCIER);
        assert_eq!(types[EPINEUX].requires, vec![RONCERAIE]);
        // Raccourcis distincts après « B ».
        let mut keys: Vec<char> = types.iter().map(|t| t.hotkey).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), types.len());
    }
}
