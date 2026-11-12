//! Variante « tout or rose » du Collectif (essai de direction artistique,
//! `OPENRTS_FACTION=collectif-or-rose`). Mêmes unités, stats et empreintes
//! que [`crate::collectif`] ; seules l'apparence, la palette et l'ambiance
//! changent. Fichier autonome, facile à supprimer si l'essai n'est pas retenu
//! (il suffit d'enlever ce fichier et la variante de `Faction` dans main.rs).
//!
//! Ici, chaque unité **est** un bijou reconnaissable, plutôt qu'une créature
//! ornée de bijoux, presque entièrement en or rose :
//!
//! - Titan : bague cocktail dressée, le solitaire serti en griffes au sommet ;
//! - Automate : pendant d'oreille articulé (dormeuse), crochet, anneau, goutte ;
//! - Fauve (à la place du Perceur, mêmes stats) : un grand félin joaillier
//!   original en mouvement de traque, rayé d'or bruni, yeux en pierres d'équipe ;
//! - Collecteur : broche scarabée Art nouveau aux élytres ajourées ;
//! - Essaim : grappe d'épingles à tête de diamant ;
//! - Essaimeur : pendentif, grande pierre ovale cerclée de pavé, sur sa chaîne.
//!
//! Le monochrome joue sur trois finitions d'or rose (poli miroir, satiné, et
//! un grain de micro-pavé) et un or rose bruni plus sombre pour la profondeur.
//! Les pierres sont des accents : diamants incolores, une morganite rose pâle
//! pour les grandes unités, et une petite pierre aux couleurs du joueur.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use rts_engine::graphics::{
    Ambiance, EnvironmentSettings, FogSettings, ParticleSettings, ReflectionPanel, ReflectionPanels, SceneLight,
};
use rts_engine::units::{
    HoverMotion, MaterialSlot, ModelBuilder, Motion, Palette, PartShape, Pulse, SurfaceDetail, SurfaceStyle, UnitType,
};

use crate::map::{GroundPattern, GroundStyle};

// Rôles des emplacements de matériau dans cette variante.
const POLISHED: MaterialSlot = MaterialSlot::Paint;
const BURNISHED: MaterialSlot = MaterialSlot::Dark;
const GRAIN: MaterialSlot = MaterialSlot::Steel;
const SATIN: MaterialSlot = MaterialSlot::Rubber;
const TEAM_STONE: MaterialSlot = MaterialSlot::Team;
const MORGANITE: MaterialSlot = MaterialSlot::Jewel;
const DIAMOND: MaterialSlot = MaterialSlot::Brilliant;

/// La morganite des grandes unités respire lentement (un seul réglage : trois
/// matériaux qui pulsent en tout).
const STONE_PULSE: Pulse = Pulse { min: 0.6, max: 1.6, frequency: 0.3 };

const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}
const ROSE_GOLD: Color = hex(0xF4, 0xC3, 0xB8);
const SATIN_GOLD: Color = hex(0xE8, 0xB6, 0xAB);
const BURNISHED_GOLD: Color = hex(0x8E, 0x52, 0x46);
const MORGANITE_COLOR: Color = hex(0xF7, 0xB6, 0xC2);
const DIAMOND_COLOR: Color = hex(0xF2, 0xF6, 0xFF);
const WARM_WHITE: Color = hex(0xFF, 0xF1, 0xE6);
const PEACH: Color = hex(0xFF, 0xC4, 0xAE);

/// Matériaux : presque tout en or rose. `team_color` ne colore qu'une petite
/// pierre par unité (laquée, très brillante pour scintiller comme une gemme).
pub fn palette(team_color: Color) -> Palette {
    Palette {
        paint: SurfaceStyle::new(ROSE_GOLD, 1.0, 0.07, SurfaceDetail::Smooth),
        dark: SurfaceStyle::new(BURNISHED_GOLD, 1.0, 0.18, SurfaceDetail::Smooth),
        // Grain de micro-pavé : la grille d'hexagones du moteur, très fine,
        // donne à l'or un scintillement granuleux.
        steel: SurfaceStyle { detail_size: 0.12, ..SurfaceStyle::new(ROSE_GOLD, 1.0, 0.22, SurfaceDetail::Hexagons) },
        bare: SurfaceStyle::new(DIAMOND_COLOR, 1.0, 0.04, SurfaceDetail::Smooth),
        rubber: SurfaceStyle::new(SATIN_GOLD, 1.0, 0.4, SurfaceDetail::Smooth),
        team: SurfaceStyle::new(team_color, 0.3, 0.06, SurfaceDetail::Smooth),
        // `glow` sert de base à la morganite (`Jewel`) : éclairée, à peine lumineuse.
        glow: SurfaceStyle::glowing(MORGANITE_COLOR, 1.6),
        engine: SurfaceStyle::glowing(PEACH, 1.0),
        selection: LinearRgba::rgb(2.2, 1.3, 1.0),
    }
}

/// Ambiance d'écrin : fond presque noir et chaud, lumière principale blanc
/// chaud, contre-jours doux blanc et pêche (aucune couleur qui concurrence
/// l'or rose), très peu de poussière dorée.
pub fn ambiance() -> Ambiance {
    let background = Color::srgb(0.016, 0.012, 0.014);
    Ambiance {
        background,
        exposure: 9.7,
        // Lumière principale blanc neutre : une lumière chaude ferait virer l'or rose au cuivre.
        sun: SceneLight { color: Color::WHITE, illuminance: 4600.0, from: Vec3::new(-0.5, 1.0, 0.55) },
        fill_lights: vec![
            SceneLight { color: hex(0xEE, 0xF0, 0xFF), illuminance: 2200.0, from: Vec3::new(0.9, 0.4, -0.5) },
            SceneLight { color: PEACH, illuminance: 1200.0, from: Vec3::new(-0.8, 0.45, -0.7) },
        ],
        environment: EnvironmentSettings {
            sky: Color::srgb(0.45, 0.42, 0.45),
            ground: Color::srgb(0.08, 0.05, 0.05),
            background: Color::srgb(0.01, 0.008, 0.009),
            light_reflections: 1.0,
            intensity: 1000.0,
        },
        fog: Some(FogSettings { start: 1.0, end: 2.6 }),
        bloom: 0.2,
        particles: Some(ParticleSettings { color: hex(0xFF, 0xC8, 0xA0), count: 35, glow: 2.0 }),
    }
}

/// Éclairage de joaillier : de longs panneaux blancs nets (ils dessinent des
/// lignes de lumière franches sur l'or poli) et un petit spot pour les diamants.
pub fn reflection_panels() -> ReflectionPanels {
    ReflectionPanels(vec![
        ReflectionPanel { color: Color::WHITE, intensity: 2.6, from: Vec3::new(-0.3, 1.0, 0.4), sharpness: 10.0 },
        ReflectionPanel { color: WARM_WHITE, intensity: 2.0, from: Vec3::new(0.8, 0.6, 0.5), sharpness: 45.0 },
        ReflectionPanel { color: DIAMOND_COLOR, intensity: 4.0, from: Vec3::new(0.2, 0.95, 0.75), sharpness: 110.0 },
        ReflectionPanel { color: PEACH, intensity: 1.4, from: Vec3::new(-1.0, 0.3, -0.4), sharpness: 30.0 },
    ])
}

/// Velours noir d'écrin.
pub fn ground() -> GroundStyle {
    GroundStyle {
        base: Color::srgb(0.03, 0.022, 0.026),
        dust: Color::srgb(0.06, 0.045, 0.05),
        grid_lines: false,
        roughness: 0.98,
        border: ROSE_GOLD.to_linear() * 0.6,
    }
}

pub fn ground_pattern() -> GroundPattern {
    GroundPattern::Velvet
}

/// Les six unités, dans l'ordre de [`crate::collectif::kind`], avec les mêmes
/// stats (le Fauve occupe la place du Perceur).
pub fn unit_types() -> Vec<UnitType> {
    vec![
        UnitType {
            hover: Some(HoverMotion { amplitude: 0.15, frequency: 0.25, lands: false }),
            ..unit("Collecteur", collecteur(), 0.55, 0.8, 4.2)
        },
        unit("Automate", automate(), 0.48, 1.0, 3.4),
        // Le Fauve prend la place (et les stats) du Perceur.
        unit("Fauve", fauve(), 1.0, 3.0, 4.4),
        UnitType {
            hover: Some(HoverMotion { amplitude: 0.25, frequency: 0.4, lands: false }),
            ..unit("Essaim", essaim(), 0.9, 0.5, 5.8)
        },
        UnitType {
            hover: Some(HoverMotion { amplitude: 0.2, frequency: 0.08, lands: false }),
            ..unit("Essaimeur", essaimeur(), 2.2, 10.0, 2.2)
        },
        unit("Titan", titan(), 2.1, 18.0, 1.7),
    ]
}

fn unit(name: &str, model: ModelBuilder, footprint_radius: f32, mass: f32, speed: f32) -> UnitType {
    UnitType { name: name.into(), parts: model.build(), footprint_radius, mass, speed, hover: None }
}

// ---------------------------------------------------------------------------
// Raccourcis de joaillier (copiés de collectif.rs pour que l'essai reste autonome).
// ---------------------------------------------------------------------------

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn placed(at: Vec3, rotation: Vec3, scale: Vec3) -> Transform {
    Transform::from_translation(at)
        .with_rotation(Quat::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z))
        .with_scale(scale)
}

/// Rotation qui couche une pièce d'axe y le long de +z.
const ALONG_Z: Vec3 = Vec3::new(FRAC_PI_2, 0.0, 0.0);

fn pearl(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3, scale: Vec3) {
    b.add(slot, PartShape::Sphere { radius }, Transform::from_translation(at).with_scale(scale));
}

/// Fuseau effilé aux deux bouts, d'axe y avant rotation.
fn spindle(b: &mut ModelBuilder, slot: MaterialSlot, length: f32, radius: f32, at: Vec3, rotation: Vec3, scale: Vec3) {
    let (h, r) = (length / 2.0, radius);
    let profile = vec![
        Vec2::new(0.0, -h),
        Vec2::new(r * 0.45, -h * 0.72),
        Vec2::new(r * 0.85, -h * 0.35),
        Vec2::new(r, 0.0),
        Vec2::new(r * 0.85, h * 0.35),
        Vec2::new(r * 0.45, h * 0.72),
        Vec2::new(0.0, h),
    ];
    b.add(slot, PartShape::Lathe { profile, segments: 12 }, placed(at, rotation, scale));
}

/// Anneau (tore) couché à plat, de `segments` pas autour de l'axe y.
fn ring_with(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, thickness: f32, segments: u32, transform: Transform) {
    let profile: Vec<Vec2> = (0..=8)
        .map(|k| {
            let a = -FRAC_PI_2 + TAU * k as f32 / 8.0;
            Vec2::new(radius + thickness * a.cos(), thickness * a.sin())
        })
        .collect();
    b.add(slot, PartShape::Lathe { profile, segments }, transform);
}

fn ring(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, thickness: f32, at: Vec3, rotation: Vec3, scale: Vec3) {
    ring_with(b, slot, radius, thickness, 18, placed(at, rotation, scale));
}

/// Taille brillant à 8 côtés, table vers +y ; `scale` l'étire en ovale ou en marquise.
fn brilliant(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3, rotation: Vec3, scale: Vec3) {
    let r = radius;
    let profile = vec![
        Vec2::new(0.0, -0.85 * r),
        Vec2::new(r, -0.05 * r),
        Vec2::new(r, 0.05 * r),
        Vec2::new(0.58 * r, 0.38 * r),
        Vec2::new(0.0, 0.38 * r),
    ];
    b.add(slot, PartShape::Faceted { profile, sides: 8 }, placed(at, rotation, scale));
}

/// Petite pierre de pavé à 6 facettes.
fn chip(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3) {
    let r = radius;
    let profile = vec![Vec2::new(0.0, -0.7 * r), Vec2::new(r, 0.0), Vec2::new(0.55 * r, 0.35 * r), Vec2::new(0.0, 0.35 * r)];
    b.add(slot, PartShape::Faceted { profile, sides: 6 }, Transform::from_translation(at));
}

/// `count` griffes autour d'une pierre de rayon `radius` (étirée par `cut`),
/// centrée en `at`, table vers +y.
fn prongs(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3, count: u32, cut: Vec3) {
    for k in 0..count {
        let a = TAU * (k as f32 + 0.5) / count as f32;
        let (sin, cos) = a.sin_cos();
        let foot = at + v(sin * radius * cut.x, 0.1 * radius, cos * radius * cut.z);
        b.cone(slot, 0.11 * radius, 0.0, 0.6 * radius, foot, v(-cos * 0.45, 0.0, sin * 0.45));
    }
}

/// Pierre sertie de griffes, table vers +y dans le repère `setting`.
fn set_stone(b: &mut ModelBuilder, stone: MaterialSlot, metal: MaterialSlot, radius: f32, count: u32, setting: Transform, cut: Vec3) {
    b.group(setting, |b| {
        brilliant(b, stone, radius, Vec3::ZERO, Vec3::ZERO, cut);
        prongs(b, metal, radius, Vec3::ZERO, count, cut);
    });
}

/// Chaîne de maillons le long de la ligne brisée `path`, `count` maillons
/// également espacés, tournés d'un quart de tour un sur deux.
fn chain(b: &mut ModelBuilder, slot: MaterialSlot, path: &[Vec3], count: u32, link: f32, wire: f32) {
    let lengths: Vec<f32> = path.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f32 = lengths.iter().sum();
    for k in 0..count {
        // Position à la distance `d` le long du chemin.
        let mut d = total * (k as f32 + 0.5) / count as f32;
        let mut segment = 0;
        while segment < lengths.len() - 1 && d > lengths[segment] {
            d -= lengths[segment];
            segment += 1;
        }
        let (a, c) = (path[segment], path[segment + 1]);
        let at = a.lerp(c, d / lengths[segment].max(1e-4));
        let along = Quat::from_rotation_arc(Vec3::Y, (c - a).normalize_or(Vec3::Y));
        let twist = Quat::from_rotation_y(if k % 2 == 0 { 0.0 } else { FRAC_PI_2 });
        let transform = Transform::from_translation(at)
            .with_rotation(along * twist * Quat::from_rotation_x(FRAC_PI_2))
            .with_scale(v(0.7, 1.0, 1.0));
        ring_with(b, slot, link, wire, 10, transform);
    }
}

fn tube(b: &mut ModelBuilder, slot: MaterialSlot, points: &[Vec3], radius: f32) {
    b.tube(slot, points, radius);
}

// ---------------------------------------------------------------------------
// Les unités. Elles sont dessinées directement dans notre repère : avant
// vers −z, et presque toutes symétriques.
// ---------------------------------------------------------------------------

/// Titan : une bague cocktail dressée, vue de face comme dans la fente d'un
/// écrin. L'anneau d'or poli forme le corps et les jambes ; ses épaules sont
/// en or grenu et pavées de diamants ; au sommet, une grosse morganite taille
/// brillant, sertie de six griffes sur un panier satiné, tourne lentement et
/// pulse. Deux pierres aux couleurs du joueur l'encadrent (visibles de dessus).
fn titan() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let radius = 1.5;
    let center = v(0.0, 1.66, 0.0);
    // Section de l'anneau : un rectangle aux coins adoucis, 90 cm de large
    // (le long de l'axe) et 30 cm d'épaisseur, parcouru en partant du bas.
    let band: Vec<Vec2> = [
        [1.42, -0.45],
        [1.58, -0.45],
        [1.65, -0.38],
        [1.65, 0.38],
        [1.58, 0.45],
        [1.42, 0.45],
        [1.35, 0.38],
        [1.35, -0.38],
        [1.42, -0.45],
    ]
    .iter()
    .map(|p| Vec2::new(p[0], p[1]))
    .collect();
    // Dressé : l'axe de l'anneau passe de y à z (il regarde vers l'avant).
    b.add(POLISHED, PartShape::Lathe { profile: band, segments: 36 }, placed(center, ALONG_Z, Vec3::ONE));
    // Les épaules de la bague : deux fuseaux grenus qui épaississent l'anneau
    // de part et d'autre du chaton, pavés de diamants sur la face avant.
    b.mirrored(|b, s| {
        let a = s * 0.6;
        let at = center + v(radius * a.sin(), radius * a.cos(), 0.0);
        spindle(b, GRAIN, 1.3, 0.3, at, v(0.0, 0.0, -a + s * FRAC_PI_2), v(1.0, 1.0, 1.7));
        for k in 0..5 {
            let a = s * (0.25 + k as f32 * 0.17);
            chip(b, DIAMOND, 0.075, center + v((radius + 0.02) * a.sin(), (radius + 0.02) * a.cos(), 0.5));
        }
        // Pierre d'épaule aux couleurs du joueur, table vers le haut.
        let side = center + v(radius * (s * 0.42).sin(), radius * 0.42f32.cos() + 0.24, 0.0);
        set_stone(b, TEAM_STONE, POLISHED, 0.22, 4, Transform::from_translation(side), Vec3::ONE);
    });
    // Le panier satiné qui porte la pierre.
    let top = center.y + 1.62;
    let basket = [[0.22, 0.0], [0.3, 0.04], [0.5, 0.36], [0.56, 0.42], [0.0, 0.42]];
    let basket: Vec<Vec2> = basket.iter().map(|p| Vec2::new(p[0], p[1])).collect();
    b.lathe(SATIN, &basket, 18, v(0.0, top, 0.0), Vec3::ZERO);
    let stone_at = v(0.0, top + 0.8, 0.0);
    prongs(&mut b, POLISHED, 0.72, stone_at, 6, Vec3::ONE);
    // Le solitaire tourne lentement sur lui-même : ses facettes accrochent la lumière.
    b.animated(Transform::from_translation(stone_at), Motion::Spin { axis: Vec3::Y, speed: 0.4 }, |b| {
        b.pulsing(STONE_PULSE, |b| brilliant(b, MORGANITE, 0.72, Vec3::ZERO, Vec3::ZERO, Vec3::ONE));
    });
    b
}

/// Automate : un pendant d'oreille articulé (dormeuse). De haut en bas : le
/// crochet d'or poli, une pierre aux couleurs du joueur sur le fermoir, une
/// barrette satinée, un anneau grenu, et la grande goutte d'or poli coiffée
/// d'un diamant, reliés par de petits anneaux. 2,2 m de haut.
fn automate() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    // La goutte, pointe en bas, presque au ras du sol.
    let drop = [[0.0, 0.05], [0.13, 0.12], [0.28, 0.32], [0.34, 0.52], [0.28, 0.72], [0.12, 0.84], [0.0, 0.87]];
    let drop: Vec<Vec2> = drop.iter().map(|p| Vec2::new(p[0], p[1])).collect();
    b.add(POLISHED, PartShape::Lathe { profile: drop, segments: 18 }, Transform::IDENTITY);
    ring(&mut b, SATIN, 0.34, 0.025, v(0.0, 0.52, 0.0), Vec3::ZERO, Vec3::ONE); // jonc à la taille de la goutte
    chip(&mut b, DIAMOND, 0.07, v(0.0, 0.9, 0.0));
    ring(&mut b, POLISHED, 0.07, 0.02, v(0.0, 0.99, 0.0), ALONG_Z, Vec3::ONE); // anneau de liaison
    // L'anneau grenu, dressé face à l'avant, un diamant suspendu en son centre.
    ring(&mut b, GRAIN, 0.24, 0.045, v(0.0, 1.32, 0.0), ALONG_Z, Vec3::ONE);
    chip(&mut b, DIAMOND, 0.09, v(0.0, 1.3, 0.0));
    ring(&mut b, POLISHED, 0.07, 0.02, v(0.0, 1.62, 0.0), Vec3::new(0.0, FRAC_PI_2, FRAC_PI_2), Vec3::ONE);
    spindle(&mut b, SATIN, 0.3, 0.06, v(0.0, 1.8, 0.0), Vec3::ZERO, Vec3::ONE); // barrette
    // Le fermoir et sa pierre d'équipe, table vers le haut.
    set_stone(&mut b, TEAM_STONE, POLISHED, 0.12, 4, Transform::from_xyz(0.0, 1.98, 0.0), Vec3::ONE);
    // Le crochet : un fil d'or qui monte et retombe vers l'arrière.
    let hook = [v(0.0, 2.0, 0.0), v(0.0, 2.22, -0.06), v(0.0, 2.3, 0.1), v(0.0, 2.2, 0.28), v(0.0, 1.98, 0.32)];
    tube(&mut b, POLISHED, &hook, 0.03);
    b
}

/// Fauve (remplace le Perceur, mêmes stats) : un grand félin joaillier
/// original, en mouvement de traque. Corps long et souple en pavé d'or rose
/// (le grain du moteur), poitrail et arrière-train polis, rayures en
/// diagonale d'or bruni sur les flancs, tête basse tendue vers l'avant,
/// patte avant gauche levée, pattes arrière ramassées, longue queue en jonc
/// satiné terminée par un diamant. Ses yeux sont deux petites pierres aux
/// couleurs du joueur. Dessiné directement avant vers −z.
fn fauve() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    // Le corps d'un seul tenant : une forme de révolution couchée le long de z,
    // profil (rayon, position) de la tête vers la queue — poitrail profond,
    // taille fine, arrière-train rond. Aplatie sur les côtés.
    let torso = [[0.0, -0.72], [0.17, -0.66], [0.27, -0.42], [0.25, -0.15], [0.19, 0.2], [0.23, 0.5], [0.24, 0.72], [0.15, 0.9], [0.0, 0.96]];
    let torso: Vec<Vec2> = torso.iter().map(|p| Vec2::new(p[0], p[1])).collect();
    b.add(POLISHED, PartShape::Lathe { profile: torso, segments: 16 }, placed(v(0.0, 1.0, 0.0), ALONG_Z, v(0.8, 1.0, 1.1)));
    // Un pavé d'or rose grenu le long de l'échine.
    spindle(&mut b, GRAIN, 1.4, 0.12, v(0.0, 1.2, 0.1), ALONG_Z, v(0.9, 1.0, 0.35));
    // Rayures : de courts traits d'or bruni, en diagonale sur chaque flanc.
    b.mirrored(|b, s| {
        for k in 0..4 {
            let z = -0.3 + k as f32 * 0.24;
            tube(b, BURNISHED, &[v(s * 0.13, 1.22, z - 0.04), v(s * 0.2, 1.06, z + 0.03), v(s * 0.19, 0.86, z + 0.12)], 0.022);
        }
    });
    // Le cou et la tête, plus bas que les épaules : le fauve traque.
    spindle(&mut b, POLISHED, 0.6, 0.18, v(0.0, 0.98, -0.8), v(-FRAC_PI_2 - 0.45, 0.0, 0.0), Vec3::ONE);
    let head = v(0.0, 0.9, -1.08);
    pearl(&mut b, POLISHED, 0.25, head, v(1.0, 0.8, 1.05));
    pearl(&mut b, SATIN, 0.13, head + v(0.0, -0.07, -0.22), v(0.85, 0.7, 1.0)); // museau
    b.mirrored(|b, s| {
        b.cone(POLISHED, 0.09, 0.0, 0.2, head + v(s * 0.14, 0.21, 0.06), v(-0.3, 0.0, -s * 0.35)); // oreille
        // Œil : une petite pierre en amande aux couleurs du joueur.
        brilliant(b, TEAM_STONE, 0.06, head + v(s * 0.1, 0.07, -0.17), v(0.3, 0.0, -s * 0.5), v(0.8, 1.0, 1.4));
    });
    // Pattes : des joncs qui s'affinent, terminés par des coussinets polis.
    let paw = |b: &mut ModelBuilder, at: Vec3| pearl(b, POLISHED, 0.08, at, v(1.0, 0.55, 1.3));
    let leg = |b: &mut ModelBuilder, points: Vec<Vec3>| {
        b.add(POLISHED, PartShape::Tube { points, radius: 0.075, radius_end: 0.05 }, Transform::IDENTITY);
    };
    // Patte avant droite posée, patte avant gauche levée vers l'avant.
    leg(&mut b, vec![v(0.18, 0.95, -0.5), v(0.21, 0.5, -0.62), v(0.2, 0.08, -0.75)]);
    paw(&mut b, v(0.2, 0.05, -0.8));
    leg(&mut b, vec![v(-0.18, 0.95, -0.5), v(-0.22, 0.62, -0.85), v(-0.2, 0.38, -1.0)]);
    paw(&mut b, v(-0.2, 0.36, -1.05));
    // Pattes arrière ramassées : hanche, genou vers l'avant, jarret vers l'arrière.
    b.mirrored(|b, s| {
        leg(b, vec![v(s * 0.2, 0.98, 0.62), v(s * 0.25, 0.58, 0.42), v(s * 0.23, 0.32, 0.9), v(s * 0.21, 0.07, 0.86)]);
        paw(b, v(s * 0.21, 0.05, 0.82));
    });
    // La queue : un long jonc satiné qui se relève et s'enroule, pointe en diamant.
    let tail = [v(0.0, 1.05, 0.85), v(0.05, 1.05, 1.2), v(0.12, 1.25, 1.5), v(0.28, 1.45, 1.55), v(0.36, 1.5, 1.35)];
    b.add(SATIN, PartShape::Tube { points: tail.to_vec(), radius: 0.045, radius_end: 0.025 }, Transform::IDENTITY);
    chip(&mut b, DIAMOND, 0.05, v(0.36, 1.52, 1.33));
    b
}

/// Collecteur : broche scarabée Art nouveau. Corps en or rose bruni, corselet
/// grenu où brille la pierre du joueur, élytres **ajourées** (un cadre d'or
/// poli et des nervures, le corps visible au travers), ailes en fil d'or qui
/// battent, antennes à pointes de diamant. Il vole à 1,3 m du sol.
fn collecteur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    // Dessiné avant vers +z, puis tourné d'un demi-tour.
    let frame = Transform::from_xyz(0.0, 1.3, 0.0).with_rotation(Quat::from_rotation_y(PI));
    b.group(frame, |b| {
        pearl(b, BURNISHED, 0.4, Vec3::ZERO, v(1.0, 0.55, 1.3)); // corps
        pearl(b, GRAIN, 0.3, v(0.0, 0.08, 0.45), v(1.1, 0.5, 0.6)); // corselet
        pearl(b, POLISHED, 0.16, v(0.0, 0.02, 0.68), v(1.1, 0.7, 0.9)); // tête
        brilliant(b, TEAM_STONE, 0.12, v(0.0, 0.2, 0.46), Vec3::ZERO, v(1.0, 1.0, 0.8)); // pierre du joueur
        b.mirrored(|b, s| {
            // Élytre ajourée : un cadre ovale et trois nervures posées sur sa courbe.
            let elytron = placed(v(s * 0.2, 0.2, -0.12), v(0.0, 0.0, s * 0.35), Vec3::ONE);
            b.group(elytron, |b| {
                ring(b, POLISHED, 0.42, 0.03, Vec3::ZERO, Vec3::ZERO, v(0.5, 1.0, 1.2));
                for x in [-0.09, 0.0, 0.09] {
                    let vein: Vec<Vec3> = (0..=6)
                        .map(|k| {
                            let z = -0.46 + k as f32 * 0.153;
                            let h = 1.0 - (x / 0.21f32).powi(2) - (z / 0.504f32).powi(2);
                            v(x, 0.13 * h.max(0.0).sqrt(), z)
                        })
                        .collect();
                    tube(b, POLISHED, &vein, 0.016);
                }
                chip(b, DIAMOND, 0.04, v(0.0, 0.13, 0.0));
            });
            // Antenne et pattes.
            tube(b, POLISHED, &[v(s * 0.06, 0.08, 0.8), v(s * 0.16, 0.26, 0.98), v(s * 0.26, 0.24, 1.12)], 0.016);
            chip(b, DIAMOND, 0.04, v(s * 0.26, 0.25, 1.14));
            for k in 0..3 {
                b.cylinder(SATIN, 0.018, 0.36, v(s * 0.3, -0.25, 0.3 - k as f32 * 0.28), v(0.0, 0.0, s * 0.5));
            }
        });
        // Ailes en fil d'or : un contour et une nervure, qui battent ensemble.
        let beat = Motion::Flicker { axis: Vec3::X, amplitude: 0.35, frequency: 6.0 };
        b.animated(Transform::from_xyz(0.0, 0.14, -0.15), beat, |b| {
            b.mirrored(|b, s| {
                let outline = [v(0.1, 0.0, 0.0), v(0.95, 0.0, -0.25), v(1.1, 0.0, 0.05), v(0.35, 0.0, 0.3), v(0.1, 0.0, 0.0)];
                let outline: Vec<Vec3> = outline.iter().map(|p| v(p.x * s, p.y, p.z)).collect();
                tube(b, POLISHED, &outline, 0.014);
                tube(b, POLISHED, &[v(s * 0.1, 0.0, 0.0), v(s * 0.6, 0.0, -0.02), v(s * 1.02, 0.0, -0.05)], 0.01);
            });
        });
    });
    b
}

/// Une épingle à tête de diamant (avant vers +z) : longue aiguille d'or poli,
/// petit fermoir satiné, et la tête sertie de quatre griffes. `team` met la
/// pierre du joueur à la place du diamant.
fn pin(b: &mut ModelBuilder, team: bool) {
    spindle(b, POLISHED, 0.8, 0.03, v(0.0, 0.0, -0.05), ALONG_Z, Vec3::ONE);
    ring(b, SATIN, 0.06, 0.018, v(0.0, 0.0, -0.22), ALONG_Z, Vec3::ONE);
    let stone = if team { TEAM_STONE } else { DIAMOND };
    set_stone(b, stone, POLISHED, 0.11, 4, Transform::from_xyz(0.0, 0.03, 0.4), Vec3::ONE);
}

/// Place une épingle sur une orbite, tournée dans le sens de sa course.
fn orbiting_pin(b: &mut ModelBuilder, angle: f32, radius: f32, y: f32, direction: f32, scale: f32, team: bool) {
    let around = Quat::from_rotation_y(angle);
    let heading = if direction > 0.0 { around * Quat::from_rotation_y(PI) } else { around };
    let at = around * Vec3::new(radius, y, 0.0);
    b.group(Transform::from_translation(at).with_rotation(heading).with_scale(Vec3::splat(scale)), |b| pin(b, team));
}

/// Essaim : une grappe de sept épingles à tête de diamant qui tournent sur
/// deux anneaux en sens contraires ; une épingle par anneau porte la pierre
/// du joueur.
fn essaim() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.animated(Transform::from_xyz(0.0, 1.6, 0.0), Motion::Spin { axis: Vec3::Y, speed: 1.6 }, |b| {
        for i in 0..4 {
            orbiting_pin(b, i as f32 * TAU / 4.0, 0.85, (i % 2) as f32 * 0.25, 1.0, 1.35, i == 0);
        }
    });
    b.animated(Transform::from_xyz(0.0, 2.1, 0.0), Motion::Spin { axis: Vec3::Y, speed: -2.3 }, |b| {
        for i in 0..3 {
            orbiting_pin(b, i as f32 * TAU / 3.0 + 0.5, 0.42, (i % 2) as f32 * -0.2, -1.0, 1.35, i == 1);
        }
    });
    b
}

/// Essaimeur : un pendentif. Grande morganite taille ovale, cerclée d'un
/// halo de pavé de diamants dans un serti clos d'or poli, et sa chaîne
/// maillée qui dessine un collier derrière elle (de dessus : une goutte et
/// sa boucle). La bélière porte la pierre du joueur. Une escorte de quatre
/// épingles tourne autour. Il vole à 3,2 m du sol.
fn essaimeur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let cut = v(0.75, 0.9, 1.45);
    b.group(Transform::from_xyz(0.0, 3.2, 0.0).with_scale(Vec3::splat(0.8)), |b| {
        // La pierre pointe vers l'avant (−z) ; la bélière est derrière (+z).
        b.pulsing(STONE_PULSE, |b| brilliant(b, MORGANITE, 0.9, Vec3::ZERO, Vec3::ZERO, cut));
        ring(b, POLISHED, 0.92, 0.07, Vec3::ZERO, Vec3::ZERO, v(0.77, 1.0, 1.46)); // serti clos
        // Halo de pavé autour du serti.
        for k in 0..18 {
            let a = TAU * k as f32 / 18.0;
            chip(b, DIAMOND, 0.09, v(1.0 * 0.78 * a.sin(), 0.04, 1.0 * 1.47 * a.cos()));
        }
        ring(b, GRAIN, 1.12, 0.05, v(0.0, -0.02, 0.0), Vec3::ZERO, v(0.78, 1.0, 1.42)); // jonc extérieur grenu
        // La bélière : un anneau dressé, la pierre du joueur sur le dessus.
        ring(b, POLISHED, 0.24, 0.07, v(0.0, 0.1, 1.85), Vec3::new(0.0, 0.0, FRAC_PI_2), Vec3::ONE);
        set_stone(b, TEAM_STONE, POLISHED, 0.16, 4, Transform::from_xyz(0.0, 0.38, 1.85), Vec3::ONE);
        // La chaîne, qui ondule doucement autour de la bélière.
        let sway = Motion::Swing { axis: Vec3::X, amplitude: 0.08, frequency: 0.2 };
        b.animated(Transform::from_xyz(0.0, 0.2, 2.05), sway, |b| {
            let path = [
                v(0.0, 0.0, 0.0),
                v(-0.8, 0.2, 0.7),
                v(-1.05, 0.35, 1.7),
                v(-0.6, 0.45, 2.5),
                v(0.0, 0.5, 2.7),
                v(0.6, 0.45, 2.5),
                v(1.05, 0.35, 1.7),
                v(0.8, 0.2, 0.7),
                v(0.0, 0.0, 0.0),
            ];
            chain(b, SATIN, &path, 34, 0.11, 0.028);
        });
    });
    b.animated(Transform::from_xyz(0.0, 2.6, 0.0), Motion::Spin { axis: Vec3::Y, speed: 0.7 }, |b| {
        for i in 0..4 {
            let a = i as f32 * TAU / 4.0 + 0.3;
            orbiting_pin(b, a, 2.3 + (i % 2) as f32 * 0.3, (i % 2) as f32 * 0.3, 1.0, 1.2, false);
        }
    });
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Même garde-fou que pour les autres factions : nombre de sommets par type.
    #[test]
    fn units_stay_light() {
        for unit in unit_types() {
            let vertices: usize = unit.parts.iter().map(|p| p.shape.mesh().count_vertices()).sum();
            println!("{} : {} sommets, {} pièces", unit.name, vertices, unit.parts.len());
            assert!(vertices < 14_000, "{} est trop lourd : {vertices} sommets", unit.name);
        }
    }
}
