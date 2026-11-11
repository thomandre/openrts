//! Le Collectif : palette, ambiance et unités (faction en exploration visuelle).
//!
//! Direction artistique : « obsidienne polie, or rose et améthystes taillées ;
//! une joaillerie mécanique à silhouette vivante ». Les unités sont des
//! **bijoux modernes** plutôt que des robots en plaques : surfaces lisses et
//! poli miroir, courbes continues (joncs, anneaux, fuseaux effilés, perles),
//! pierres taillées serties en griffes, lignes de pavé le long des membres,
//! membres en anneaux enchaînés. La couleur du joueur est un émail laqué
//! (perles, cabochons, élytres) et non une peinture sur des blocs.
//!
//! Là où la Coalition tient le terrain avec des fantassins tireurs et des
//! blindés, et où les Sylves avancent lentement au contact, le Collectif mise
//! sur l'air et sur le nombre de petites machines : ses ouvriers volent
//! (Collecteurs), un vaisseau porte-drones (Essaimeur) et des nuées de
//! guêpes (Essaims) harcèlent, des scorpions (Perceurs) percent les blindés,
//! des Automates élancés tirent au rayon et un Titan sert de forteresse
//! ambulante. Sa faiblesse : peu d'unités au sol, chères et lentes à remplacer.
//!
//! Les unités reprennent les silhouettes et les cotes de la page de direction
//! artistique (`buildCollectif`), dans le **repère de la page** (avant vers
//! +z) : chaque modèle est dessiné dans un groupe tourné d'un demi-tour et
//! mis à l'échelle ([`page`]), pour que l'avant passe vers −z.
//!
//! Les emplacements de matériau du moteur reçoivent ici des rôles de joaillier :
//!
//! | Emplacement | Collectif |
//! | --- | --- |
//! | `Paint` | or rose poli miroir |
//! | `Dark` | onyx / obsidienne polie (perles, cabochons, coques) |
//! | `Steel` | platine poli (le contraste froid de l'or rose) |
//! | `Bare` | couleur des diamants (utilisée par `Brilliant`) |
//! | `Rubber` | or rose satiné, la seule finition brossée (joncs, pattes) |
//! | `Team` | émail laqué aux couleurs du joueur |
//! | `Glow` | améthyste lumineuse (yeux) ; `Jewel` en dérive pour les pierres taillées |
//! | `Engine` | lueur violette (réacteurs) ; `Flame` en dérive pour les ailes translucides |
//! | `Brilliant` | diamants incolores (pavé, pointes, griffes de cœur) |

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use rts_engine::graphics::{
    Ambiance, EnvironmentSettings, FogSettings, ParticleSettings, ReflectionPanel, ReflectionPanels, SceneLight,
};
use rts_engine::units::{
    HoverMotion, MaterialSlot, ModelBuilder, Motion, Palette, PartShape, Pulse, SurfaceDetail, SurfaceStyle, UnitType,
};

use crate::map::{GroundPattern, GroundStyle};

// Noms des rôles des emplacements, pour que les modèles se lisent bien.
const GOLD: MaterialSlot = MaterialSlot::Paint;
const ONYX: MaterialSlot = MaterialSlot::Dark;
const PLATINUM: MaterialSlot = MaterialSlot::Steel;
const SATIN: MaterialSlot = MaterialSlot::Rubber;
const ENAMEL: MaterialSlot = MaterialSlot::Team;
const GLOW: MaterialSlot = MaterialSlot::Glow;
const AMETHYST_STONE: MaterialSlot = MaterialSlot::Jewel;
const DIAMOND: MaterialSlot = MaterialSlot::Brilliant;
const ENGINE: MaterialSlot = MaterialSlot::Engine;
const WING: MaterialSlot = MaterialSlot::Flame;

/// Les grosses pierres (cœur du Titan, pierre du Perceur, marquise de
/// l'Essaimeur) respirent lentement. Une seule pulsation pour toute la
/// faction : trois matériaux en tout (un par décalage, voir `PULSE_PHASES`).
const GEM_PULSE: Pulse = Pulse { min: 0.5, max: 1.8, frequency: 0.35 };

/// Couleurs de la page de direction artistique.
const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}
pub const OBSIDIAN_COLOR: Color = hex(0x0B, 0x0B, 0x12);
pub const ROSE_GOLD: Color = hex(0xE6, 0xA4, 0x8E);
pub const BRUSHED_GOLD: Color = hex(0xC9, 0x8A, 0x78);
pub const AMETHYST: Color = hex(0x8B, 0x5C, 0xF6);
pub const HALO_COLOR: Color = hex(0xC4, 0xB5, 0xFD);
const PEACH: Color = hex(0xFF, 0xBF, 0xA6);
const PLATINUM_COLOR: Color = hex(0xD8, 0xDB, 0xE2);
/// Diamant : blanc froid, légèrement bleuté.
const DIAMOND_COLOR: Color = hex(0xEE, 0xF3, 0xFF);
/// Lueur des ailes et des réacteurs : le halo #C4B5FD assombri (les ailes,
/// additives, saturent en blanc si elle est plus claire).
const WING_GLOW: Color = hex(0x6A, 0x58, 0xB8);

/// Matériaux du Collectif. `team_color` est la couleur du joueur.
///
/// Aucune texture de détail (`Smooth` partout) : un bijou est lisse, ce
/// sont ses formes et ses reflets qui le dessinent. Les métaux reflètent
/// surtout l'éclairage d'ambiance, d'où les panneaux de reflet de
/// [`reflection_panels`].
pub fn palette(team_color: Color) -> Palette {
    Palette {
        paint: SurfaceStyle::new(ROSE_GOLD, 1.0, 0.1, SurfaceDetail::Smooth),
        // L'onyx est une pierre (pas un métal) : noire, avec des reflets nets et discrets.
        dark: SurfaceStyle::new(OBSIDIAN_COLOR, 0.15, 0.06, SurfaceDetail::Smooth),
        steel: SurfaceStyle::new(PLATINUM_COLOR, 1.0, 0.1, SurfaceDetail::Smooth),
        bare: SurfaceStyle::new(DIAMOND_COLOR, 1.0, 0.05, SurfaceDetail::Smooth),
        rubber: SurfaceStyle::new(BRUSHED_GOLD, 1.0, 0.42, SurfaceDetail::Smooth),
        // Émail laqué : non métallique (la couleur reste franche à côté de
        // l'or rose, qui prend la teinte de ses reflets) et brillant.
        team: SurfaceStyle::new(team_color, 0.0, 0.12, SurfaceDetail::Smooth),
        glow: SurfaceStyle::glowing(AMETHYST, 3.0),
        engine: SurfaceStyle::glowing(WING_GLOW, 1.0),
        // Anneau de sélection lavande, couleur du halo.
        selection: LinearRgba::rgb(1.3, 1.0, 2.6),
    }
}

/// Ambiance des cartes du Collectif : une vitrine de nuit. Fond violet très
/// sombre, lumière principale blanc chaud, contre-jours améthyste et pêche,
/// ciel d'ambiance assez sombre (pour le contraste) et poussière d'améthyste.
pub fn ambiance() -> Ambiance {
    let background = Color::srgb(0.03, 0.025, 0.055);
    Ambiance {
        background,
        exposure: 9.7,
        sun: SceneLight { color: hex(0xFF, 0xF0, 0xE2), illuminance: 4300.0, from: Vec3::new(-0.5, 1.0, 0.55) },
        fill_lights: vec![
            // Contre-jours : améthyste à droite, pêche à gauche, comme sur la page de référence.
            SceneLight { color: AMETHYST, illuminance: 3000.0, from: Vec3::new(0.9, 0.35, -0.5) },
            SceneLight { color: PEACH, illuminance: 2000.0, from: Vec3::new(-0.8, 0.45, -0.7) },
        ],
        environment: EnvironmentSettings {
            sky: Color::srgb(0.4, 0.34, 0.58),
            ground: Color::srgb(0.16, 0.1, 0.12),
            background: Color::srgb(0.015, 0.012, 0.03),
            light_reflections: 1.0,
            intensity: 850.0,
        },
        fog: Some(FogSettings { start: 1.0, end: 2.6 }),
        bloom: 0.22,
        particles: Some(ParticleSettings { color: HALO_COLOR, count: 90, glow: 3.0 }),
    }
}

/// Les boîtes à lumière du bijoutier : quatre panneaux qu'on ne voit que dans
/// les reflets. Petits et intenses, sur un environnement sombre, ils dessinent
/// sur l'or et le platine des reflets nets et contrastés (des lignes de
/// lumière franches plutôt qu'un brillant diffus).
pub fn reflection_panels() -> ReflectionPanels {
    ReflectionPanels(vec![
        // Grand panneau blanc au-dessus de la caméra : le reflet principal vu de dessus.
        ReflectionPanel { color: Color::WHITE, intensity: 2.0, from: Vec3::new(-0.2, 1.0, 0.35), sharpness: 18.0 },
        // Petit spot blanc froid, très net : l'étincelle des pierres.
        ReflectionPanel { color: DIAMOND_COLOR, intensity: 4.0, from: Vec3::new(0.45, 0.9, 0.6), sharpness: 90.0 },
        // Bandes colorées rasantes, de chaque côté.
        ReflectionPanel { color: AMETHYST, intensity: 2.0, from: Vec3::new(1.0, 0.25, -0.2), sharpness: 25.0 },
        ReflectionPanel { color: PEACH, intensity: 1.6, from: Vec3::new(-1.0, 0.3, -0.3), sharpness: 25.0 },
    ])
}

/// Sol du Collectif : velours d'écrin, prune très sombre et mat, liseré améthyste.
pub fn ground() -> GroundStyle {
    GroundStyle {
        base: Color::srgb(0.06, 0.035, 0.075),
        dust: Color::srgb(0.12, 0.07, 0.14),
        grid_lines: false,
        roughness: 0.97,
        border: AMETHYST.to_linear() * 1.2,
    }
}

/// Motif du sol : le velours.
pub fn ground_pattern() -> GroundPattern {
    GroundPattern::Velvet
}

/// Indices des types dans [`unit_types`], pour s'y référer par leur nom.
pub mod kind {
    pub const COLLECTEUR: usize = 0;
    pub const AUTOMATE: usize = 1;
    pub const PERCEUR: usize = 2;
    pub const ESSAIM: usize = 3;
    pub const ESSAIMEUR: usize = 4;
    pub const TITAN: usize = 5;
}

/// Les six unités du Collectif (stats provisoires, voir GAME_DESIGN.md).
pub fn unit_types() -> Vec<UnitType> {
    vec![
        UnitType {
            hover: Some(HoverMotion { amplitude: 0.15, frequency: 0.25 }),
            ..unit("Collecteur", collecteur(), 0.55, 0.8, 4.2)
        },
        unit("Automate", automate(), 0.48, 1.0, 3.4),
        unit("Perceur", perceur(), 1.0, 3.0, 4.4),
        UnitType {
            hover: Some(HoverMotion { amplitude: 0.25, frequency: 0.4 }),
            ..unit("Essaim", essaim(), 0.9, 0.5, 5.8)
        },
        UnitType {
            hover: Some(HoverMotion { amplitude: 0.2, frequency: 0.08 }),
            ..unit("Essaimeur", essaimeur(), 2.2, 10.0, 2.2)
        },
        unit("Titan", titan(), 2.1, 18.0, 1.7),
    ]
}

/// `mass` : masse relative pour les collisions, 1 pour un fantassin.
fn unit(name: &str, model: ModelBuilder, footprint_radius: f32, mass: f32, speed: f32) -> UnitType {
    UnitType { name: name.into(), parts: model.build(), footprint_radius, mass, speed, hover: None }
}

// ---------------------------------------------------------------------------
// Raccourcis d'écriture : les pièces d'un joaillier.
// ---------------------------------------------------------------------------

/// Raccourci pour écrire les positions.
fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// Repère de la page, ramené au nôtre : demi-tour (l'avant passe de +z à −z)
/// et mise à l'échelle.
fn page(scale: f32) -> Transform {
    Transform::from_rotation(Quat::from_rotation_y(PI)).with_scale(Vec3::splat(scale))
}

/// Placement complet : position, rotation (x, y, z en radians), échelle par axe.
fn placed(at: Vec3, rotation: Vec3, scale: Vec3) -> Transform {
    Transform::from_translation(at)
        .with_rotation(Quat::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z))
        .with_scale(scale)
}

/// Rotation qui couche une pièce d'axe y le long de +z (aiguilles, canons).
const ALONG_Z: Vec3 = Vec3::new(FRAC_PI_2, 0.0, 0.0);
/// Rotation qui couche une pièce d'axe y le long de x.
const ALONG_X: Vec3 = Vec3::new(0.0, 0.0, FRAC_PI_2);

/// Perle : sphère polie, étirée par `scale` (cabochons, têtes, coques).
fn pearl(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3, scale: Vec3) {
    b.add(slot, PartShape::Sphere { radius }, Transform::from_translation(at).with_scale(scale));
}

/// Fuseau : forme de révolution effilée aux deux bouts, de `length` mètres le
/// long de son axe (y avant rotation) et de rayon `radius` en son milieu.
/// Aplati par `scale` (x petit), il devient une lame « knife-edge ».
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

/// Anneau (tore) : un petit cercle de rayon `thickness` tourne autour de
/// l'axe y à `radius` mètres du centre. Couché à plat sans rotation.
fn ring(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, thickness: f32, at: Vec3, rotation: Vec3, scale: Vec3) {
    // Le profil fait le tour du petit cercle en partant du bas, dans le sens
    // qui garde l'extérieur à droite (comme le demande `Lathe`).
    let profile: Vec<Vec2> = (0..=8)
        .map(|k| {
            let a = -FRAC_PI_2 + TAU * k as f32 / 8.0;
            Vec2::new(radius + thickness * a.cos(), thickness * a.sin())
        })
        .collect();
    b.add(slot, PartShape::Lathe { profile, segments: 18 }, placed(at, rotation, scale));
}

/// Taille brillant à 8 côtés : pavillon pointu (la colette en bas),
/// rondiste, couronne et table plate en haut (+y). `scale` l'étire en
/// marquise ou en poire.
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

/// Petite pierre de pavé : 6 facettes seulement (une quarantaine de sommets).
fn chip(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3) {
    let r = radius;
    let profile = vec![Vec2::new(0.0, -0.7 * r), Vec2::new(r, 0.0), Vec2::new(0.55 * r, 0.35 * r), Vec2::new(0.0, 0.35 * r)];
    b.add(slot, PartShape::Faceted { profile, sides: 6 }, Transform::from_translation(at));
}

/// Ligne de pavé : `count` petites pierres régulièrement espacées de `from` à `to`.
fn pave(b: &mut ModelBuilder, slot: MaterialSlot, from: Vec3, to: Vec3, count: u32, radius: f32) {
    for k in 0..count {
        let t = if count > 1 { k as f32 / (count - 1) as f32 } else { 0.5 };
        chip(b, slot, radius, from.lerp(to, t));
    }
}

/// Pierre sertie : une taille brillant (table vers +y dans le repère du
/// serti) tenue par `prongs` griffes de métal qui remontent sur sa couronne.
/// `setting` place le serti (tourner pour orienter la table) ; `cut` étire la
/// pierre (marquise, poire).
fn set_stone(b: &mut ModelBuilder, stone: MaterialSlot, metal: MaterialSlot, radius: f32, prongs: u32, setting: Transform, cut: Vec3) {
    b.group(setting, |b| {
        brilliant(b, stone, radius, Vec3::ZERO, Vec3::ZERO, cut);
        for k in 0..prongs {
            let a = TAU * (k as f32 + 0.5) / prongs as f32;
            let (sin, cos) = a.sin_cos();
            // Griffe : petit cône du rondiste vers le haut, penché vers la pierre.
            let at = v(sin * radius * cut.x, 0.12 * radius, cos * radius * cut.z);
            let tilt = v(-cos * 0.45, 0.0, sin * 0.45);
            b.cone(metal, 0.12 * radius, 0.0, 0.6 * radius, at, tilt);
        }
    });
}

/// Tube courbe de rayon constant qui passe par `points`.
fn tube(b: &mut ModelBuilder, slot: MaterialSlot, points: &[[f32; 3]], radius: f32) {
    let points: Vec<Vec3> = points.iter().map(|&p| Vec3::from_array(p)).collect();
    b.tube(slot, &points, radius);
}

/// Chaîne de `count` maillons (anneaux enchaînés) de `from` à `to`, chaque
/// maillon tourné d'un quart de tour par rapport au précédent.
fn chain(b: &mut ModelBuilder, slot: MaterialSlot, from: Vec3, to: Vec3, count: u32, link: f32, wire: f32) {
    let axis = (to - from).normalize_or(Vec3::Y);
    // Rotation qui amène l'axe y du maillon le long de la chaîne.
    let along = Quat::from_rotation_arc(Vec3::Y, axis);
    for k in 0..count {
        let t = (k as f32 + 0.5) / count as f32;
        // Maillon dressé : un anneau couché tourné d'un quart de tour autour de x,
        // puis d'un quart de tour autour de l'axe de la chaîne un maillon sur deux.
        let twist = Quat::from_rotation_y(if k % 2 == 0 { 0.0 } else { FRAC_PI_2 });
        let rotation = along * twist * Quat::from_rotation_x(FRAC_PI_2);
        let transform = Transform::from_translation(from.lerp(to, t))
            .with_rotation(rotation)
            .with_scale(v(0.75, 1.0, 1.0));
        b.group(transform, |b| ring(b, slot, link, wire, Vec3::ZERO, Vec3::ZERO, Vec3::ONE));
    }
}

/// Plaque plate et horizontale découpée selon un contour convexe (le
/// `tprism` de la page) : ailes, ailerons. `outline` est le contour vu de
/// dessus, en (x, z) ; `thickness` son épaisseur verticale. Un biseau
/// marqué lui donne une arête vive (« knife-edge ») qui attrape la lumière.
///
/// Le prisme du moteur est dessiné dans le plan (z, y) et extrudé le long
/// de x : on y range le contour en (z, x), puis un quart de tour autour de z
/// fait passer son « y » en x et l'extrusion à la verticale.
fn flat_prism(b: &mut ModelBuilder, slot: MaterialSlot, outline: &[[f32; 2]], thickness: f32, bevel: f32, at: Vec3) {
    let profile: Vec<Vec2> = outline.iter().map(|p| Vec2::new(p[1], p[0])).collect();
    let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_z(-FRAC_PI_2));
    if bevel > 0.0 {
        b.add(slot, PartShape::BeveledPrism { profile, width: thickness, bevel }, transform);
    } else {
        b.add(slot, PartShape::Prism { profile, width: thickness }, transform);
    }
}

/// Aile translucide, à plat, vers +x (`side` = 1) ou −x (−1).
fn wing(b: &mut ModelBuilder, outline: &[[f32; 2]], at: Vec3, side: f32) {
    let mirrored: Vec<[f32; 2]> = outline.iter().map(|p| [p[0] * side, p[1]]).collect();
    flat_prism(b, WING, &mirrored, 0.012, 0.0, at);
}

// ---------------------------------------------------------------------------
// Les unités.
// ---------------------------------------------------------------------------

/// Automate : l'infanterie, une figurine de joaillerie. Jambes en fuseaux de
/// platine et d'or rose, genoux en perles d'onyx, bassin en jonc, buste en
/// goutte d'or rose où est sertie une améthyste taille poire, bras en
/// anneaux enchaînés. Ses épaules sont deux perles d'émail serties d'un jonc
/// d'or (de dessus : deux points de couleur qui encadrent la tête). Son arme
/// est une longue aiguille de platine pavée d'améthystes. 1,95 m.
fn automate() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(0.67), |b| {
        let one = Vec3::ONE;
        b.mirrored(|b, s| {
            spindle(b, GOLD, 0.55, 0.11, v(s * 0.22, 0.08, 0.08), ALONG_Z, v(1.1, 1.0, 0.6)); // pied effilé
            spindle(b, PLATINUM, 0.8, 0.085, v(s * 0.22, 0.52, 0.0), Vec3::ZERO, one); // tibia
            pearl(b, ONYX, 0.1, v(s * 0.22, 0.92, 0.0), one); // genou
            spindle(b, GOLD, 0.7, 0.11, v(s * 0.2, 1.27, 0.0), Vec3::ZERO, one); // cuisse
            pave(b, DIAMOND, v(s * 0.2, 1.1, 0.1), v(s * 0.2, 1.42, 0.1), 3, 0.035); // pavé de cuisse
            // Épaule : perle d'émail sertie d'un jonc d'or à l'équateur.
            pearl(b, ENAMEL, 0.22, v(s * 0.52, 2.46, 0.0), one);
            ring(b, GOLD, 0.225, 0.025, v(s * 0.52, 2.46, 0.0), Vec3::ZERO, one);
            chain(b, GOLD, v(s * 0.55, 2.27, 0.0), v(s * 0.56, 1.88, 0.0), 2, 0.08, 0.02); // bras
            pearl(b, DIAMOND, 0.05, v(s * 0.56, 1.84, 0.0), one); // coude
            spindle(b, PLATINUM, 0.55, 0.075, v(s * 0.56, 1.62, 0.14), v(-0.5, 0.0, 0.0), one); // avant-bras
            pearl(b, ONYX, 0.07, v(s * 0.56, 1.38, 0.3), one); // main
            pearl(b, GLOW, 0.032, v(s * 0.07, 2.86, 0.215), one); // œil
        });
        ring(b, GOLD, 0.22, 0.06, v(0.0, 1.6, 0.0), Vec3::ZERO, v(1.2, 1.0, 0.8)); // bassin en jonc
        spindle(b, SATIN, 0.45, 0.1, v(0.0, 1.8, 0.0), Vec3::ZERO, one); // taille
        // Buste : une goutte d'or rose qui s'évase vers les épaules, aplatie d'avant en arrière.
        let chest = [[0.0, 0.0], [0.18, 0.03], [0.36, 0.3], [0.42, 0.5], [0.3, 0.64], [0.0, 0.68]];
        let chest: Vec<Vec2> = chest.iter().map(|p| Vec2::new(p[0], p[1])).collect();
        b.add(GOLD, PartShape::Lathe { profile: chest, segments: 16 }, placed(v(0.0, 1.9, 0.0), Vec3::ZERO, v(1.0, 1.0, 0.6)));
        // Améthyste taille poire au sternum, sertie de quatre griffes de platine.
        let sternum = placed(v(0.0, 2.26, 0.25), ALONG_Z, Vec3::ONE);
        set_stone(b, AMETHYST_STONE, PLATINUM, 0.11, 4, sternum, v(0.8, 1.0, 1.3));
        spindle(b, PLATINUM, 0.5, 0.1, v(0.0, 2.3, -0.24), Vec3::ZERO, v(0.3, 1.0, 1.0)); // arête dorsale
        spindle(b, PLATINUM, 0.2, 0.07, v(0.0, 2.62, 0.0), Vec3::ZERO, one); // cou
        pearl(b, GOLD, 0.2, v(0.0, 2.84, 0.0), v(0.9, 1.1, 1.0)); // tête
        pearl(b, ONYX, 0.17, v(0.0, 2.83, 0.07), v(0.85, 1.0, 0.9)); // visage
        chip(b, DIAMOND, 0.06, v(0.0, 3.07, 0.0)); // diamant du front
        // L'arme : aiguille de platine le long de l'avant-bras droit, pavée
        // d'améthystes, tenue par un jonc d'or, pointe en diamant.
        spindle(b, PLATINUM, 1.4, 0.05, v(0.56, 1.42, 0.72), ALONG_Z, one);
        ring(b, GOLD, 0.07, 0.02, v(0.56, 1.42, 0.3), ALONG_Z, one);
        pave(b, AMETHYST_STONE, v(0.56, 1.47, 0.45), v(0.56, 1.47, 1.05), 5, 0.03);
        chip(b, DIAMOND, 0.05, v(0.56, 1.42, 1.42));
    });
    b
}

/// Perceur : scorpion d'assaut anti-blindé, broche « Serpenti ». Carapace
/// d'or rose lisse rehaussée d'une ligne de pavé de diamants, ventre et tête
/// en cabochons d'onyx, six pattes en joncs d'or satiné. La queue enfile cinq
/// perles d'émail (de dessus : une virgule colorée au-dessus du dos), chacune
/// tenue par un anneau d'or, et se termine par une améthyste taille poire
/// sertie qui pulse, et un dard de platine.
fn perceur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(0.62), |b| {
        let one = Vec3::ONE;
        pearl(b, GOLD, 0.6, v(0.0, 0.95, 0.2), v(1.1, 0.55, 1.5)); // carapace
        pearl(b, ONYX, 0.55, v(0.0, 0.8, 0.2), v(1.0, 0.5, 1.4)); // ventre
        // Pavé le long de l'arête du dos, posé sur la courbe de la carapace.
        for k in 0..7 {
            let z = -0.55 + k as f32 * 0.22;
            let t = ((z - 0.2) / 0.9).clamp(-1.0, 1.0);
            chip(b, DIAMOND, 0.055, v(0.0, 0.95 + 0.33 * (1.0 - t * t).sqrt() + 0.01, z));
        }
        pearl(b, ONYX, 0.3, v(0.0, 0.95, 1.15), v(1.0, 0.6, 0.8)); // tête en cabochon
        ring(b, PLATINUM, 0.38, 0.08, v(0.0, 1.0, -0.75), ALONG_Z, one); // anneau de croupe
        // La queue : un jonc d'or satiné qui enfile cinq perles d'émail.
        let tail = [[0.0, 1.05, -1.1], [0.0, 1.8, -1.7], [0.0, 2.7, -1.6], [0.0, 3.3, -1.0], [0.0, 3.4, -0.2]];
        tube(b, SATIN, &tail, 0.1);
        for (i, p) in tail.iter().enumerate() {
            let at = Vec3::from_array(*p);
            let r = 0.27 - i as f32 * 0.025;
            pearl(b, ENAMEL, r, at, v(1.0, 0.85, 1.0));
            // Anneau d'or perpendiculaire à la queue, autour de la perle.
            let next = Vec3::from_array(tail[(i + 1).min(tail.len() - 1)]);
            let prev = Vec3::from_array(tail[i.saturating_sub(1)]);
            let along = Quat::from_rotation_arc(Vec3::Y, (next - prev).normalize_or(Vec3::Y));
            b.group(Transform::from_translation(at).with_rotation(along), |b| {
                ring(b, GOLD, r * 0.95, 0.03, Vec3::ZERO, Vec3::ZERO, one);
            });
        }
        b.pulsing(GEM_PULSE, |b| {
            let tip = placed(v(0.0, 3.42, -0.1), v(-0.6, 0.0, 0.0), Vec3::ONE);
            set_stone(b, AMETHYST_STONE, GOLD, 0.24, 4, tip, v(0.8, 1.0, 1.3));
        });
        spindle(b, PLATINUM, 1.3, 0.11, v(0.0, 3.1, 0.5), v(2.07, 0.0, 0.0), one); // dard
        b.mirrored(|b, s| {
            pearl(b, GLOW, 0.05, v(s * 0.14, 1.06, 1.38), one); // œil
            tube(b, SATIN, &[[s * 0.5, 0.9, 1.0], [s * 1.1, 1.0, 1.5], [s * 1.0, 1.0, 2.1]], 0.07); // bras
            pearl(b, ONYX, 0.13, v(s * 1.0, 1.0, 2.1), one); // poignet
            // Pinces : deux lames de platine.
            spindle(b, PLATINUM, 0.8, 0.12, v(s * 1.1, 1.06, 2.45), ALONG_Z, v(0.5, 1.0, 1.0));
            spindle(b, PLATINUM, 0.65, 0.1, v(s * 0.88, 0.94, 2.4), ALONG_Z, v(0.5, 1.0, 1.0));
            for (z, dz) in [(0.6, 0.2), (0.1, 0.0), (-0.4, -0.25)] {
                // Patte en arc : de la hanche au genou (un diamant), puis au sol.
                tube(b, SATIN, &[[s * 0.5, 0.85, z], [s * 1.3, 1.5, z + dz], [s * 1.75, 0.06, z + dz * 2.0]], 0.05);
                chip(b, DIAMOND, 0.08, v(s * 1.3, 1.53, z + dz));
                b.cone(GOLD, 0.07, 0.0, 0.24, v(s * 1.75, 0.12, z + dz * 2.0), Vec3::ZERO);
            }
        });
    });
    b
}

/// Collecteur : l'ouvrier, un scarabée-broche volant. Corps en cabochon
/// d'onyx, tête d'or rose, élytres en émail serties d'un fil d'or, antennes
/// de platine aux pointes de diamant, deux ailes translucides qui battent.
/// Sous le ventre, l'améthyste qu'il rapporte, tenue par quatre griffes.
/// Il vole à 1,3 m du sol.
fn collecteur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(Transform::from_xyz(0.0, 1.3, 0.0) * page(1.0), |b| {
        let one = Vec3::ONE;
        pearl(b, ONYX, 0.4, Vec3::ZERO, v(1.0, 0.6, 1.3)); // corps
        pearl(b, GOLD, 0.2, v(0.0, 0.02, 0.52), v(1.1, 0.75, 1.0)); // tête
        // Cargaison : une améthyste table en bas, sertie sous le ventre.
        let cargo = placed(v(0.0, -0.3, 0.05), v(PI, 0.0, 0.0), Vec3::ONE);
        set_stone(b, AMETHYST_STONE, GOLD, 0.17, 4, cargo, Vec3::ONE);
        spindle(b, PLATINUM, 0.9, 0.05, v(0.0, 0.33, -0.12), ALONG_Z, v(1.0, 1.0, 0.6)); // couture des élytres
        b.mirrored(|b, s| {
            // Élytre en émail, et le fil d'or qui la borde (un anneau étiré comme elle).
            let elytron = placed(v(s * 0.3, 0.28, -0.1), v(0.0, 0.0, s * 0.5), Vec3::ONE);
            b.group(elytron, |b| {
                pearl(b, ENAMEL, 0.42, Vec3::ZERO, v(0.5, 0.3, 1.2));
                ring(b, GOLD, 0.42, 0.035, Vec3::ZERO, Vec3::ZERO, v(0.52, 0.5, 1.22));
            });
            pearl(b, GLOW, 0.045, v(s * 0.1, 0.06, 0.68), one); // œil
            tube(b, PLATINUM, &[[s * 0.06, 0.12, 0.66], [s * 0.16, 0.3, 0.85], [s * 0.24, 0.28, 1.02]], 0.018); // antenne
            chip(b, DIAMOND, 0.04, v(s * 0.24, 0.28, 1.04));
            for k in 0..3 {
                // Pattes repliées sous le ventre, en or satiné.
                b.cylinder(SATIN, 0.02, 0.36, v(s * 0.3, -0.3, 0.3 - k as f32 * 0.28), v(0.0, 0.0, s * 0.5));
            }
        });
        // Les ailes : un seul groupe animé, qui s'étire et se rétracte vite
        // en largeur (vu de dessus, c'est un battement d'ailes).
        let beat = Motion::Flicker { axis: Vec3::X, amplitude: 0.35, frequency: 6.0 };
        b.animated(Transform::from_xyz(0.0, 0.18, -0.15), beat, |b| {
            let outline = [[0.0, 0.0], [0.9, -0.25], [1.05, 0.05], [0.3, 0.3]];
            b.mirrored(|b, s| wing(b, &outline, v(s * 0.15, 0.0, 0.0), s));
        });
    });
    b
}

/// Une guêpe-drone, petite épingle sertie, dans son propre repère (avant
/// vers +z) : thorax en fuseau d'or rose, abdomen en fuseau d'émail aux
/// couleurs du joueur, tête en diamant, dard de platine, deux petites ailes.
fn wasp(b: &mut ModelBuilder) {
    spindle(b, GOLD, 0.36, 0.09, Vec3::ZERO, ALONG_Z, v(1.0, 1.0, 0.65)); // thorax
    spindle(b, ENAMEL, 0.46, 0.11, v(0.0, -0.02, -0.34), ALONG_Z, v(1.0, 1.0, 0.8)); // abdomen
    chip(b, DIAMOND, 0.075, v(0.0, 0.02, 0.24)); // tête
    spindle(b, PLATINUM, 0.22, 0.02, v(0.0, -0.03, -0.64), ALONG_Z, Vec3::ONE); // dard
    // Ailes réduites de 35 % par rapport à la page : en test de charge, des
    // dizaines d'Essaims aux grandes ailes lumineuses formaient un tapis violet.
    let outline = [[0.0, 0.0], [0.32, -0.09], [0.39, 0.01], [0.08, 0.08]];
    b.mirrored(|b, s| wing(b, &outline, v(s * 0.06, 0.08, 0.0), s));
}

/// Place une guêpe sur une orbite : à l'angle `angle` sur un cercle de rayon
/// `radius`, à la hauteur `y`, tournée dans le sens de sa course (`direction`
/// = 1 pour une rotation positive autour de +y, −1 pour l'autre sens).
fn orbiting_wasp(b: &mut ModelBuilder, angle: f32, radius: f32, y: f32, direction: f32, scale: f32) {
    let around = Quat::from_rotation_y(angle);
    // Une rotation positive autour de +y emmène le point (r, 0, 0) vers −z :
    // la guêpe (avant vers +z) fait alors un demi-tour pour regarder vers −z.
    let heading = if direction > 0.0 { around * Quat::from_rotation_y(PI) } else { around };
    let at = around * Vec3::new(radius, y, 0.0);
    b.group(Transform::from_translation(at).with_rotation(heading).with_scale(Vec3::splat(scale)), wasp);
}

/// Essaim : une nuée de sept guêpes-épingles en orbite autour d'un centre
/// vide, sur deux anneaux qui tournent en sens contraires. De dessus : un
/// cercle de petits points d'or et de couleur d'équipe qui tourbillonne.
fn essaim() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let outer = Motion::Spin { axis: Vec3::Y, speed: 1.6 };
    b.animated(Transform::from_xyz(0.0, 1.6, 0.0), outer, |b| {
        for i in 0..4 {
            let a = i as f32 * TAU / 4.0;
            orbiting_wasp(b, a, 0.85, (i % 2) as f32 * 0.25, 1.0, 1.35);
        }
    });
    let inner = Motion::Spin { axis: Vec3::Y, speed: -2.3 };
    b.animated(Transform::from_xyz(0.0, 2.1, 0.0), inner, |b| {
        for i in 0..3 {
            let a = i as f32 * TAU / 3.0 + 0.5;
            orbiting_wasp(b, a, 0.42, (i % 2) as f32 * -0.2, -1.0, 1.35);
        }
    });
    b
}

/// Essaimeur : grand vaisseau porte-drones en forme de broche-fuseau. Coque
/// en fuseau d'onyx, coquille d'or rose par-dessus, grosse améthyste taille
/// marquise sertie au centre, deux cabochons d'émail aux couleurs du joueur,
/// grandes ailes d'or à arête vive bordées de pavé, et en poupe une
/// pendeloque (une pierre poire au bout d'un fil de platine). Une escorte de
/// quatre guêpes tourne autour de lui : ce sont les drones qu'il lancera au
/// combat. Il vole à 3,2 m du sol.
///
/// Sur la page, il vole vers +x : le groupe le tourne d'un quart de tour
/// pour qu'il avance vers −z.
fn essaimeur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let ship = Transform::from_xyz(0.0, 3.2, 0.0)
        .with_rotation(Quat::from_rotation_y(FRAC_PI_2))
        .with_scale(Vec3::splat(0.5));
    b.group(ship, |b| {
        // Le fuseau : profil (rayon, position le long du vaisseau) de la poupe
        // à la proue, couché le long de x et aplati verticalement.
        let hull: Vec<Vec2> = [[0.0, -3.6], [0.55, -3.2], [1.0, -2.0], [1.22, 0.3], [0.95, 2.4], [0.4, 3.4], [0.0, 3.8]]
            .iter()
            .map(|p| Vec2::new(p[0], p[1]))
            .collect();
        // Quart de tour autour de z : l'axe du fuseau (y) passe le long de +x,
        // et son x local devient la verticale, qu'on écrase.
        let lying = |at: Vec3, scale: Vec3| placed(at, v(0.0, 0.0, -FRAC_PI_2), scale);
        b.add(ONYX, PartShape::Lathe { profile: hull.clone(), segments: 16 }, lying(Vec3::ZERO, v(0.32, 1.0, 1.0))); // coque
        b.add(GOLD, PartShape::Lathe { profile: hull, segments: 16 }, lying(v(0.1, 0.18, 0.0), v(0.3, 0.84, 0.78))); // coquille
        // La marquise centrale, sertie de six griffes de platine et cerclée d'or.
        b.pulsing(GEM_PULSE, |b| {
            let setting = Transform::from_xyz(0.9, 0.52, 0.0);
            set_stone(b, AMETHYST_STONE, PLATINUM, 0.48, 6, setting, v(1.7, 1.0, 0.75));
        });
        ring(b, GOLD, 0.5, 0.06, v(0.9, 0.46, 0.0), Vec3::ZERO, v(1.7, 1.0, 0.8));
        // Deux cabochons d'émail en arrière de la pierre.
        b.mirrored(|b, s| {
            pearl(b, ENAMEL, 0.45, v(-1.2, 0.42, s * 0.42), v(2.0, 0.45, 0.75));
            ring(b, GOLD, 0.45, 0.04, v(-1.2, 0.42, s * 0.42), Vec3::ZERO, v(2.02, 1.0, 0.77));
        });
        // Pendeloque de poupe.
        tube(b, PLATINUM, &[[-3.6, 0.0, 0.0], [-5.0, 0.25, 0.0], [-6.2, 0.9, 0.0]], 0.06);
        brilliant(b, AMETHYST_STONE, 0.24, v(-6.2, 0.75, 0.0), v(PI, 0.0, 0.0), v(0.8, 1.6, 0.8));
        chip(b, DIAMOND, 0.14, v(3.85, 0.05, 0.0)); // diamant de proue
        b.mirrored(|b, s| {
            // Pavé d'améthystes le long des flancs.
            pave(b, AMETHYST_STONE, v(-2.2, 0.12, s * 1.02), v(1.6, 0.12, s * 1.06), 7, 0.09);
            // Réacteur : forme de révolution de platine couchée vers l'arrière (−x).
            let pod = [Vec2::new(0.0, 0.0), Vec2::new(0.22, 0.0), Vec2::new(0.3, 0.35), Vec2::new(0.24, 0.7), Vec2::new(0.0, 0.7)];
            b.lathe(PLATINUM, &pod, 14, v(-3.3, 0.0, s * 0.45), ALONG_X);
            b.cylinder(ENGINE, 0.2, 0.03, v(-4.01, 0.0, s * 0.45), ALONG_X); // tuyère
            // Grande aile d'or à arête vive, bordée de pavé de diamants, dans un
            // groupe animé qui ondule autour de l'axe du vaisseau.
            let flap = Motion::Swing { axis: Vec3::X * s, amplitude: 0.12, frequency: 0.14 };
            b.animated(Transform::IDENTITY, flap, |b| {
                let fin = [[-1.6, 1.1], [1.4, 1.1], [0.1, 3.3], [-1.3, 2.7]];
                let fin: Vec<[f32; 2]> = fin.iter().map(|p| [p[0], p[1] * s]).collect();
                flat_prism(b, GOLD, &fin, 0.1, 0.045, Vec3::ZERO);
                pave(b, DIAMOND, v(1.2, 0.07, s * 1.42), v(0.2, 0.07, s * 3.05), 5, 0.08);
            });
        });
    });
    // L'escorte, à l'échelle des guêpes de l'Essaim, sous le vaisseau.
    let escort = Motion::Spin { axis: Vec3::Y, speed: 0.7 };
    b.animated(Transform::from_xyz(0.0, 2.6, 0.0), escort, |b| {
        for i in 0..4 {
            let a = i as f32 * TAU / 4.0 + 0.3;
            orbiting_wasp(b, a, 2.3 + (i % 2) as f32 * 0.3, (i % 2) as f32 * 0.3, 1.0, 1.2);
        }
    });
    b
}

/// Titan : le géant, une grande bague qui marche. Jambes en fuseaux de
/// platine cerclés de joncs d'or, genoux en perles d'onyx serties d'un
/// diamant, buste en coupe d'or rose couronnée de pavé, et au milieu le
/// cœur : une grosse améthyste taille brillant qui tourne et pulse, tenue
/// par six griffes de platine. Un bras-canon autour duquel s'enroule un
/// serpent d'or satiné, un poing d'onyx à trois griffes. Les épaules sont
/// deux grandes perles d'émail serties d'or, la crête une lame d'émail : de
/// dessus, deux grandes taches de couleur reliées par un joug de platine.
fn titan() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(0.62), |b| {
        let one = Vec3::ONE;
        b.mirrored(|b, s| {
            pearl(b, GOLD, 0.6, v(s * 1.05, 0.22, 0.25), v(0.85, 0.35, 1.5)); // pied
            spindle(b, PLATINUM, 1.8, 0.44, v(s * 1.05, 1.3, 0.0), Vec3::ZERO, one); // tibia
            for y in [0.85, 1.75] {
                ring(b, GOLD, 0.37, 0.07, v(s * 1.05, y, 0.0), Vec3::ZERO, one); // joncs du tibia
            }
            pearl(b, ONYX, 0.42, v(s * 1.05, 2.2, 0.05), one); // genou
            let kneecap = placed(v(s * 1.05, 2.25, 0.45), ALONG_Z, Vec3::ONE);
            set_stone(b, DIAMOND, PLATINUM, 0.15, 4, kneecap, Vec3::ONE);
            spindle(b, GOLD, 1.4, 0.44, v(s * 0.95, 2.95, -0.05), Vec3::ZERO, one); // cuisse
            // Épaule : grande perle d'émail sertie d'un jonc d'or, une pierre au bout.
            pearl(b, ENAMEL, 0.92, v(s * 2.45, 5.7, 0.0), v(1.0, 0.8, 1.0));
            ring(b, GOLD, 0.93, 0.09, v(s * 2.45, 5.7, 0.0), Vec3::ZERO, one);
            brilliant(b, AMETHYST_STONE, 0.3, v(s * 3.35, 5.7, 0.0), v(0.0, 0.0, -s * FRAC_PI_2), one);
            chain(b, GOLD, v(s * 2.5, 5.05, 0.0), v(s * 2.5, 4.0, 0.0), 3, 0.28, 0.07); // bras en maillons
            // Aileron dorsal : une lame de fuseau aplatie.
            spindle(b, GOLD, 1.4, 0.5, v(s * 0.7, 6.1, -1.15), v(0.5, 0.0, -s * 0.25), v(0.22, 1.0, 1.0));
            pearl(b, GLOW, 0.07, v(s * 0.16, 6.74, 0.72), one); // œil
        });
        // Bras droit : le canon, un fût d'or effilé vers l'avant, et un
        // serpent d'or satiné enroulé autour.
        let barrel = [Vec2::new(0.0, 0.0), Vec2::new(0.62, 0.0), Vec2::new(0.62, 0.3), Vec2::new(0.48, 1.9), Vec2::new(0.0, 1.9)];
        b.lathe(GOLD, &barrel, 18, v(2.5, 3.75, -0.3), ALONG_Z);
        let coil: Vec<[f32; 3]> = (0..=12)
            .map(|k| {
                let t = k as f32 / 12.0;
                let a = t * 3.0 * TAU;
                let r = 0.66 - 0.14 * t;
                [2.5 + r * a.cos(), 3.75 + r * a.sin(), -0.15 + 1.7 * t]
            })
            .collect();
        tube(b, SATIN, &coil, 0.07);
        ring(b, PLATINUM, 0.6, 0.08, v(2.5, 3.75, -0.25), ALONG_Z, one); // bague arrière
        brilliant(b, AMETHYST_STONE, 0.4, v(2.5, 3.75, 1.62), ALONG_Z, one); // bouche du canon
        // Bras gauche : avant-bras de platine, poing d'onyx, trois griffes d'or.
        spindle(b, PLATINUM, 1.7, 0.38, v(-2.5, 3.5, 0.25), v(-0.25, 0.0, 0.0), one);
        pearl(b, ONYX, 0.42, v(-2.5, 2.62, 0.5), one);
        for k in 0..3 {
            spindle(b, GOLD, 0.55, 0.11, v(-2.5 + (k as f32 - 1.0) * 0.22, 2.25, 0.72), v(0.4, 0.0, 0.0), v(0.6, 1.0, 1.0));
        }
        // Le corps.
        ring(b, PLATINUM, 1.0, 0.24, v(0.0, 3.55, 0.0), Vec3::ZERO, v(1.3, 1.0, 0.6)); // bassin en jonc
        spindle(b, SATIN, 1.0, 0.75, v(0.0, 4.05, 0.0), Vec3::ZERO, one); // taille
        // Buste : une coupe d'or rose qui s'évase vers les épaules.
        let cup = [[0.0, -0.95], [1.2, -0.9], [1.5, -0.3], [1.72, 0.5], [1.62, 0.85], [0.0, 0.95]];
        let cup: Vec<Vec2> = cup.iter().map(|p| Vec2::new(p[0], p[1])).collect();
        b.add(GOLD, PartShape::Lathe { profile: cup, segments: 20 }, placed(v(0.0, 5.15, 0.0), Vec3::ZERO, v(1.0, 1.0, 0.8)));
        // Couronne de pavé autour du bord, diamants et améthystes en alternance.
        for k in 0..14 {
            let a = TAU * k as f32 / 14.0;
            let slot = if k % 2 == 0 { DIAMOND } else { AMETHYST_STONE };
            chip(b, slot, 0.13, v(1.5 * a.sin(), 6.04, 1.5 * 0.8 * a.cos()));
        }
        // Le cœur : un serti de six griffes et un anneau de platine (fixes),
        // et la pierre qui tourne lentement sur elle-même en pulsant.
        ring(b, PLATINUM, 0.6, 0.07, v(0.0, 5.15, 1.36), ALONG_Z, one);
        for k in 0..6 {
            let a = TAU * (k as f32 + 0.5) / 6.0;
            b.cone(PLATINUM, 0.06, 0.0, 0.3, v(0.55 * a.sin(), 5.15 + 0.55 * a.cos(), 1.55), v(FRAC_PI_2 - 0.5 * a.cos(), 0.0, 0.5 * a.sin()));
        }
        b.animated(Transform::from_xyz(0.0, 5.15, 1.5), Motion::Spin { axis: Vec3::Z, speed: 0.6 }, |b| {
            b.pulsing(GEM_PULSE, |b| brilliant(b, AMETHYST_STONE, 0.5, Vec3::ZERO, ALONG_Z, Vec3::ONE));
        });
        // Joug des épaules : un fuseau de platine couché en travers.
        spindle(b, PLATINUM, 4.0, 0.32, v(0.0, 6.2, 0.0), ALONG_X, v(1.0, 1.0, 0.8));
        pearl(b, GOLD, 0.5, v(0.0, 6.74, 0.22), v(0.95, 1.0, 1.05)); // tête
        pearl(b, ONYX, 0.42, v(0.0, 6.68, 0.4), v(0.85, 0.9, 0.8)); // visage
        chip(b, DIAMOND, 0.1, v(0.0, 7.05, 0.55)); // diamant du front
        spindle(b, ENAMEL, 1.1, 0.28, v(0.0, 7.2, 0.05), ALONG_Z, v(0.35, 1.0, 1.0)); // crête d'émail
        // Réacteur dorsal : anneau de platine autour d'une améthyste.
        ring(b, PLATINUM, 0.42, 0.1, v(0.0, 5.4, -1.45), ALONG_Z, one);
        brilliant(b, AMETHYST_STONE, 0.3, v(0.0, 5.4, -1.5), v(-FRAC_PI_2, 0.0, 0.0), one);
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
            println!("{} : {} sommets, {} pièces", unit.name, vertices, unit.parts.len());
            assert!(vertices < 14_000, "{} est trop lourd : {vertices} sommets", unit.name);
        }
    }
}
