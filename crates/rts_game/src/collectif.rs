//! Le Collectif : palette, ambiance et unités (faction en exploration visuelle).
//!
//! Direction artistique : « or jaune, émail vert et émeraudes. Des
//! machines-bijoux : automates couronnés, colosse serti, scorpions-colliers et
//! vaisseau-bracelet. » Les unités restent des **machines et des créatures**
//! (des jambes, des bras, des pinces, des ailes) : la joaillerie est dans
//! leurs lignes et leurs ornements, pas dans un bijou posé tel quel. Le corps
//! est en plaques d'or poli et d'émail vert ; dessus viennent les gestes du
//! joaillier : sertis et cabochons, rangs de perles, couronnes de lames,
//! émeraudes taillées, anneaux et bracelet ouvert.
//!
//! Là où la Garde tient le terrain avec des fantassins tireurs et des
//! blindés, et où les Sylves avancent lentement au contact, le Collectif mise
//! sur l'air et sur le nombre de petites machines : ses ouvriers volent
//! (Collecteurs), un vaisseau porte-drones (Essaimeur) et des nuées de
//! guêpes (Essaims) harcèlent, des scorpions (Perceurs) percent les blindés,
//! des Automates couronnés tirent au rayon et un Titan sert de forteresse
//! ambulante. Sa faiblesse : peu d'unités au sol, chères et lentes à remplacer.
//!
//! Les unités reprennent pièce par pièce les cotes de la page de direction
//! artistique (`buildCollectif`), dans le **repère de la page** (avant vers
//! +z, formes de three.js) : chaque modèle est dessiné dans un groupe tourné
//! d'un demi-tour et mis à l'échelle ([`page`]), pour que l'avant passe vers
//! −z. Les petites fonctions de la section « Formes de la page » traduisent
//! chacune une forme de three.js (`plate`, `hexc`, `octa`, `brill`, `cab`,
//! `arc`…) en pièces du moteur.
//!
//! Les emplacements de matériau du moteur reçoivent ici des rôles de joaillier :
//!
//! | Emplacement | Collectif |
//! | --- | --- |
//! | `Paint` | or jaune poli |
//! | `Dark` | émail vert profond (le « noir » de la faction) |
//! | `Steel` | or vieilli, plus sombre et satiné (articulations, montures) |
//! | `Team` | émail laqué aux couleurs du joueur |
//! | `Glow` | émeraude lumineuse (yeux, filets) ; `Jewel` en dérive pour les pierres taillées |
//! | `Engine` | halo vert d'eau ; `Flame` en dérive pour les ailes translucides |
//! | `Bare`, `Rubber` | inutilisés (or clair, or vieilli) |

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use rts_engine::graphics::shapes::catmull_rom;
use rts_engine::graphics::{
    Ambiance, EnvironmentSettings, FogSettings, ParticleSettings, ReflectionPanel, ReflectionPanels, SceneLight,
};
use rts_engine::units::{
    HoverMotion, MaterialSlot, ModelBuilder, Motion, Palette, PartShape, Pulse, SurfaceDetail, SurfaceStyle, UnitType,
};

use crate::map::{GroundPattern, GroundStyle};

// Noms des rôles des emplacements, pour que les modèles se lisent comme la page.
/// L'or poli (`gold` / `cer` de la page).
const GOLD: MaterialSlot = MaterialSlot::Paint;
/// L'émail vert (`enam` / `black` de la page).
const ENAMEL: MaterialSlot = MaterialSlot::Dark;
/// L'or vieilli (`old` / `tit` de la page).
const OLD_GOLD: MaterialSlot = MaterialSlot::Steel;
/// L'émail aux couleurs du joueur (absent de la page, voir [`unit_types`]).
const TEAM: MaterialSlot = MaterialSlot::Team;
/// Les filets et les yeux lumineux (`en` de la page).
const GLOW: MaterialSlot = MaterialSlot::Glow;
/// Les émeraudes taillées (`gem` de la page).
const EMERALD_STONE: MaterialSlot = MaterialSlot::Jewel;
/// Les ailes translucides (`wingM` de la page).
const WING: MaterialSlot = MaterialSlot::Flame;

/// Les trois grosses pierres (cœur du Titan, dard du Perceur, orbe de
/// l'Essaimeur) respirent lentement. Sur la page, toutes les émeraudes
/// pulsent ensemble ; ici, seules ces trois-là, car chaque matériau qui pulse
/// coûte cher (trois matériaux en tout, un par décalage).
const GEM_PULSE: Pulse = Pulse { min: 0.6, max: 1.7, frequency: 0.35 };

/// Couleurs de la page de direction artistique.
const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color::srgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}
pub const ENAMEL_GREEN: Color = hex(0x0F, 0x3D, 0x2E);
pub const GOLD_COLOR: Color = hex(0xE2, 0xB5, 0x52);
pub const OLD_GOLD_COLOR: Color = hex(0xB9, 0x8A, 0x2F);
pub const EMERALD: Color = hex(0x10, 0xB9, 0x81);
pub const HALO_COLOR: Color = hex(0x6E, 0xE7, 0xB7);
/// Le vert clair de l'éclairage d'ambiance de la page (`makeEnv(0x34d399, …)`).
const MINT: Color = hex(0x34, 0xD3, 0x99);
/// Le contre-jour chaud de la page.
const WARM_GOLD: Color = hex(0xFF, 0xD2, 0x7A);
/// Lueur des ailes : le halo #6EE7B7 assombri (les ailes, additives,
/// saturent en blanc si elle est plus claire).
const WING_GLOW: Color = hex(0x2C, 0x80, 0x64);

/// Matériaux du Collectif. `team_color` est la couleur du joueur.
///
/// Aucune texture de détail (`Smooth` partout) : un bijou est lisse, ce sont
/// ses formes et ses reflets qui le dessinent. Les ors reflètent surtout
/// l'éclairage d'ambiance, d'où les panneaux de reflet de [`reflection_panels`].
pub fn palette(team_color: Color) -> Palette {
    Palette {
        paint: SurfaceStyle::new(GOLD_COLOR, 1.0, 0.18, SurfaceDetail::Smooth),
        // L'émail est un verre coloré : peu métallique, brillant.
        dark: SurfaceStyle::new(ENAMEL_GREEN, 0.3, 0.22, SurfaceDetail::Smooth),
        steel: SurfaceStyle::new(OLD_GOLD_COLOR, 1.0, 0.32, SurfaceDetail::Smooth),
        bare: SurfaceStyle::new(hex(0xF3, 0xD5, 0x8A), 1.0, 0.1, SurfaceDetail::Smooth),
        rubber: SurfaceStyle::new(OLD_GOLD_COLOR, 1.0, 0.45, SurfaceDetail::Smooth),
        // Émail laqué : non métallique (la couleur reste franche à côté de
        // l'or, qui prend la teinte de ses reflets) et brillant.
        team: SurfaceStyle::new(team_color, 0.0, 0.14, SurfaceDetail::Smooth),
        glow: SurfaceStyle::glowing(EMERALD, 3.2),
        engine: SurfaceStyle::glowing(WING_GLOW, 1.0),
        // Anneau de sélection émeraude.
        selection: LinearRgba::rgb(0.5, 2.6, 1.5),
    }
}

/// Ambiance des cartes du Collectif, d'après le `look` de la page : une
/// vitrine de nuit. Fond bleu-vert très sombre, lumière principale blanc
/// chaud, contre-jours émeraude et or chaud, ciel d'ambiance vert et or
/// assez sombre (pour le contraste) et poussière vert d'eau qui monte.
pub fn ambiance() -> Ambiance {
    // `BG` de la page (#0A1018).
    let background = hex(0x0A, 0x10, 0x18);
    Ambiance {
        background,
        exposure: 9.7,
        sun: SceneLight { color: hex(0xFF, 0xF3, 0xDC), illuminance: 4300.0, from: Vec3::new(-0.5, 1.0, 0.55) },
        fill_lights: vec![
            // Contre-jours : émeraude (le plus fort) à droite, or chaud à gauche.
            SceneLight { color: EMERALD, illuminance: 3000.0, from: Vec3::new(0.9, 0.35, -0.5) },
            SceneLight { color: WARM_GOLD, illuminance: 1800.0, from: Vec3::new(-0.8, 0.45, -0.7) },
        ],
        environment: EnvironmentSettings {
            sky: Color::srgb(0.32, 0.46, 0.36),
            ground: Color::srgb(0.15, 0.12, 0.07),
            background: Color::srgb(0.012, 0.02, 0.025),
            light_reflections: 1.0,
            intensity: 850.0,
        },
        fog: Some(FogSettings { start: 1.0, end: 2.6 }),
        bloom: 0.22,
        particles: Some(ParticleSettings { color: HALO_COLOR, count: 90, glow: 3.0 }),
    }
}

/// Les boîtes à lumière du bijoutier : les panneaux du `makeEnv` de la page,
/// qu'on ne voit que dans les reflets. Petits et intenses, sur un
/// environnement sombre, ils dessinent sur l'or des reflets nets et
/// contrastés (des lignes de lumière franches plutôt qu'un brillant diffus).
pub fn reflection_panels() -> ReflectionPanels {
    ReflectionPanels(vec![
        // Grand panneau blanc au-dessus de la caméra : le reflet principal vu
        // de dessus (modéré : plus fort, les faces planes d'or du Titan,
        // tournées vers le ciel, saturaient en blanc en test de charge).
        ReflectionPanel { color: Color::WHITE, intensity: 1.4, from: Vec3::new(-0.2, 1.0, 0.35), sharpness: 18.0 },
        // Petit spot blanc chaud, très net : l'étincelle des pierres.
        ReflectionPanel { color: hex(0xFF, 0xF6, 0xE0), intensity: 3.5, from: Vec3::new(0.45, 0.9, 0.6), sharpness: 90.0 },
        // Bandes colorées rasantes, de chaque côté : vert clair et or pâle.
        ReflectionPanel { color: MINT, intensity: 2.0, from: Vec3::new(-1.0, 0.3, 0.4), sharpness: 25.0 },
        ReflectionPanel { color: hex(0xFF, 0xE4, 0xA8), intensity: 1.8, from: Vec3::new(1.0, 0.25, -0.4), sharpness: 25.0 },
    ])
}

/// Sol du Collectif : les dalles hexagonales sombres de la page (#121821),
/// légèrement teintées de vert, liseré émeraude.
pub fn ground() -> GroundStyle {
    GroundStyle {
        base: hex(0x10, 0x17, 0x1D),
        dust: hex(0x15, 0x22, 0x22),
        grid_lines: false,
        roughness: 0.55,
        border: EMERALD.to_linear() * 1.2,
    }
}

/// Motif du sol : les dalles hexagonales.
pub fn ground_pattern() -> GroundPattern {
    GroundPattern::Hexagons
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
///
/// La page n'a pas de couleur d'équipe : l'émail du joueur ([`TEAM`]) prend
/// la place de l'or ou de l'émeraude sur les pièces les plus visibles de
/// dessus — le joug des épaules et la crête du Titan, le plastron des
/// Automates, le cabochon du dos et les perles serties de la queue des
/// Perceurs, les élytres des Collecteurs, l'abdomen des guêpes et les
/// cabochons du bracelet de l'Essaimeur.
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
// Formes de la page : chaque fonction reprend une forme de three.js.
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

/// Rotation de three.js (`rotation.set(x, y, z)`, ordre XYZ, le même que
/// `EulerRot::XYZ` de Bevy).
fn euler(r: Vec3) -> Quat {
    Quat::from_euler(EulerRot::XYZ, r.x, r.y, r.z)
}

/// Placement complet : position, rotation (x, y, z en radians), échelle par axe.
fn placed(at: Vec3, rotation: Vec3, scale: Vec3) -> Transform {
    Transform::from_translation(at).with_rotation(euler(rotation)).with_scale(scale)
}

const NO_TURN: Vec3 = Vec3::ZERO;
const ONE: Vec3 = Vec3::ONE;
/// Quart de tour autour de x : couche une pièce d'axe y le long de +z.
const ALONG_Z: Vec3 = Vec3::new(FRAC_PI_2, 0.0, 0.0);
/// Quart de tour autour de z : couche une pièce d'axe y le long de x.
const ALONG_X: Vec3 = Vec3::new(0.0, 0.0, FRAC_PI_2);

/// `plate(w, h, d, b)` : plaque biseautée. Comme sur la page, le biseau ne
/// dépasse pas 22 % de la plus petite cote.
fn plate(b: &mut ModelBuilder, slot: MaterialSlot, size: [f32; 3], bevel: f32, at: Vec3, rotation: Vec3) {
    let size = Vec3::from_array(size);
    let bevel = bevel.min(size.min_element() * 0.22);
    b.add(slot, PartShape::Plate { size, bevel }, placed(at, rotation, ONE));
}

/// `box(w, h, d)` : petite barre à arêtes vives (les filets lumineux).
fn bar(b: &mut ModelBuilder, slot: MaterialSlot, size: [f32; 3], at: Vec3) {
    let size = Vec3::from_array(size);
    b.add(slot, PartShape::Plate { size, bevel: size.min_element() * 0.1 }, Transform::from_translation(at));
}

/// `hexc(a, b, h)` : prisme hexagonal d'axe y, rayon `top` en haut et
/// `bottom` en bas, une face tournée vers +z (comme sur la page).
fn hexc(b: &mut ModelBuilder, slot: MaterialSlot, top: f32, bottom: f32, height: f32, at: Vec3, rotation: Vec3) {
    let h = height / 2.0;
    // Profil (rayon, hauteur) du bas vers le haut : fond, flanc, couvercle.
    let profile = [Vec2::new(0.0, -h), Vec2::new(bottom, -h), Vec2::new(top, h), Vec2::new(0.0, h)];
    b.faceted(slot, &profile, 6, at, rotation);
}

/// `cyl(a, b, h)` : cylindre (ou cône tronqué) d'axe y, rayon `top` en haut.
fn cyl(b: &mut ModelBuilder, slot: MaterialSlot, top: f32, bottom: f32, height: f32, at: Vec3, rotation: Vec3) {
    b.cone(slot, bottom, top, height, at, rotation);
}

/// `sph(r)` étirée par `scale` : têtes, carapaces, perles.
fn ball(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3, scale: Vec3) {
    b.add(slot, PartShape::Sphere { radius }, Transform::from_translation(at).with_scale(scale));
}

/// `octa(r)` : octaèdre (pierre à huit faces), étiré par `scale`.
fn octa(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3, scale: Vec3) {
    let profile = [Vec2::new(0.0, -radius), Vec2::new(radius, 0.0), Vec2::new(0.0, radius)];
    b.add(slot, PartShape::Faceted { profile: profile.to_vec(), sides: 4 }, Transform::from_translation(at).with_scale(scale));
}

/// `IcosahedronGeometry(r, 0)` : à défaut d'icosaèdre, une pierre ronde à
/// vingt facettes (deux pointes et une ceinture de dix faces) qui en a
/// l'allure. Centrée sur l'origine du groupe en cours.
fn ico(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32) {
    let r = radius;
    let profile = vec![Vec2::new(0.0, -r), Vec2::new(0.89 * r, -0.45 * r), Vec2::new(0.89 * r, 0.45 * r), Vec2::new(0.0, r)];
    b.add(slot, PartShape::Faceted { profile, sides: 5 }, Transform::IDENTITY);
}

/// `brill(r)` : la taille brillant de la page, à huit côtés : une pointe
/// (le pavillon) vers −y, le rondiste, puis la couronne et la table plate
/// vers +y. Facettes vives : la page l'éclaire en `flatShading`.
fn brill(b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, at: Vec3, rotation: Vec3) {
    let r = radius;
    let profile = vec![Vec2::new(0.0, -1.1 * r), Vec2::new(r, 0.0), Vec2::new(0.72 * r, 0.38 * r), Vec2::new(0.0, 0.42 * r)];
    b.faceted(slot, &profile, 8, at, rotation);
}

/// `prism(pts, depth, b)` : contour (x, y) de la page extrudé sur `depth` le
/// long de z, arêtes biseautées. Le prisme du moteur, lui, est dessiné en
/// (z, y) et extrudé le long de x : on y range le contour en (−x, y), puis un
/// quart de tour autour de y (−π/2) ramène le −x du dessin en +x et
/// l'extrusion le long de z. Le contour doit être convexe.
fn prism(b: &mut ModelBuilder, slot: MaterialSlot, outline: &[[f32; 2]], depth: f32, bevel: f32, placement: Transform) {
    let profile: Vec<Vec2> = outline.iter().map(|p| Vec2::new(-p[0], p[1])).collect();
    let shape = if bevel > 0.0 {
        PartShape::BeveledPrism { profile, width: depth, bevel }
    } else {
        PartShape::Prism { profile, width: depth }
    };
    b.add(slot, shape, placement * Transform::from_rotation(Quat::from_rotation_y(-FRAC_PI_2)));
}

/// `bladeG` : la lame des couronnes, un éclat d'or effilé de 1,7 m (cote de
/// la page) qui penche vers −x en montant.
const BLADE: [[f32; 2]; 4] = [[0.0, 0.0], [0.26, 0.0], [0.12, 1.7], [-0.2, 1.25]];

/// Une lame de couronne, posée en `at`, tournée de `rotation` et réduite de `scale`.
fn blade(b: &mut ModelBuilder, slot: MaterialSlot, at: Vec3, rotation: Vec3, scale: f32) {
    prism(b, slot, &BLADE, 0.06, 0.02, placed(at, rotation, Vec3::splat(scale)));
}

/// `TorusGeometry(major, minor, …, arc)` de la page : un anneau dans le plan
/// (x, y), d'axe z, dont l'arc part de +x et tourne vers +y. Le tore du
/// moteur est couché à plat (axe y, arc de +x vers +z) : un quart de tour
/// autour de x (−π/2) le redresse dans le plan (x, y).
fn torus(b: &mut ModelBuilder, slot: MaterialSlot, major: f32, minor: f32, arc: f32, placement: Transform) {
    let upright = Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2));
    b.add(slot, PartShape::Torus { major, minor, arc }, placement * upright);
}

/// `ring(p, R, r, …)` : un anneau fermé de la page.
fn ring(b: &mut ModelBuilder, slot: MaterialSlot, major: f32, minor: f32, at: Vec3, rotation: Vec3) {
    torus(b, slot, major, minor, TAU, placed(at, rotation, ONE));
}

/// `arc(p, R, r, ang, …, spin)` : un anneau ouvert (pince en croissant),
/// placé et tourné en (x, y) par `rotation`, l'ouverture orientée par `spin`
/// (rotation autour de son axe).
fn arc(b: &mut ModelBuilder, slot: MaterialSlot, major: f32, minor: f32, angle: f32, at: Vec3, rotation: Vec3, spin: f32) {
    let placement = placed(at, rotation, ONE) * Transform::from_rotation(Quat::from_rotation_z(spin));
    torus(b, slot, major, minor, angle, placement);
}

/// `cab(p, r, …)` : cabochon serti. Une pierre ronde et bombée (sphère
/// aplatie) dans un jonc d'or, dans le plan (x, y) de son groupe, bombée
/// vers +z. `stone` est l'émail ou la pierre.
fn cab(b: &mut ModelBuilder, stone: MaterialSlot, radius: f32, at: Vec3, rotation: Vec3) {
    b.group(placed(at, rotation, ONE), |b| {
        ring(b, GOLD, radius * 1.05, radius * 0.2, Vec3::ZERO, NO_TURN);
        ball(b, stone, radius, v(0.0, 0.0, radius * 0.12), v(1.0, 1.0, 0.55));
    });
}

/// Tube courbe de rayon constant qui passe par `points`.
fn tube(b: &mut ModelBuilder, slot: MaterialSlot, points: &[[f32; 3]], radius: f32) {
    let points: Vec<Vec3> = points.iter().map(|&p| Vec3::from_array(p)).collect();
    b.tube(slot, &points, radius);
}

/// Point d'une courbe de Catmull-Rom passant par `points`, à `t` ∈ [0, 1]
/// (comme `CatmullRomCurve3.getPoint` de three.js : chaque tronçon entre
/// deux points reçoit la même part de `t`). Aux deux bouts, il manque un
/// voisin : three.js le prolonge en ligne droite, on fait de même.
fn curve_point(points: &[Vec3], t: f32) -> Vec3 {
    let last = points.len() - 1;
    let x = t.clamp(0.0, 1.0) * last as f32;
    let i = (x.floor() as usize).min(last - 1);
    let at = |k: isize| -> Vec3 {
        if k < 0 {
            2.0 * points[0] - points[1]
        } else if k as usize > last {
            2.0 * points[last] - points[last - 1]
        } else {
            points[k as usize]
        }
    };
    let i = i as isize;
    catmull_rom(at(i - 1), at(i), at(i + 1), at(i + 2), x - i as f32)
}

/// Aile translucide, à plat, vers +x (`side` = 1) ou −x (−1). `outline` est
/// le contour vu de dessus, en (x, z) : c'est le contour de la page, que
/// celle-ci couche à plat d'un quart de tour.
fn wing(b: &mut ModelBuilder, outline: &[[f32; 2]], at: Vec3, side: f32) {
    // Le prisme du moteur est dessiné dans le plan (z, y) et extrudé le long
    // de x : on y range le contour en (z, x), puis un quart de tour autour de
    // z fait passer son « y » en x et l'extrusion à la verticale.
    let profile: Vec<Vec2> = outline.iter().map(|p| Vec2::new(p[1], p[0] * side)).collect();
    let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_z(-FRAC_PI_2));
    b.add(WING, PartShape::Prism { profile, width: 0.012 }, transform);
}

// ---------------------------------------------------------------------------
// Les unités.
// ---------------------------------------------------------------------------

/// Automate : l'infanterie, une figurine mécanique couronnée. Jambes en
/// plaques d'or et d'émail vert, genoux et coudes en perles d'or vieilli,
/// buste en écu d'émail aux couleurs du joueur où est sertie une émeraude,
/// tête d'or au visage d'émail, et derrière elle une couronne de trois lames
/// d'or. Son arme est une longue baguette d'émail parcourue d'un filet
/// émeraude. 1,95 m.
fn automate() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(0.67), |b| {
        b.mirrored(|b, s| {
            plate(b, ENAMEL, [0.26, 0.14, 0.5], 0.03, v(s * 0.22, 0.07, 0.06), NO_TURN); // pied
            plate(b, GOLD, [0.2, 0.7, 0.24], 0.04, v(s * 0.22, 0.52, 0.0), NO_TURN); // tibia
            bar(b, GLOW, [0.03, 0.4, 0.02], v(s * 0.22, 0.52, 0.125));
            ball(b, OLD_GOLD, 0.11, v(s * 0.22, 0.92, 0.0), ONE); // genou
            plate(b, ENAMEL, [0.22, 0.6, 0.26], 0.04, v(s * 0.2, 1.25, 0.0), NO_TURN); // cuisse
            plate(b, GOLD, [0.18, 0.4, 0.06], 0.02, v(s * 0.2, 1.27, 0.14), NO_TURN);
            ball(b, GOLD, 0.15, v(s * 0.52, 2.46, 0.0), ONE); // épaule
            plate(b, ENAMEL, [0.15, 0.5, 0.17], 0.03, v(s * 0.56, 2.12, 0.0), NO_TURN); // bras
            ball(b, OLD_GOLD, 0.08, v(s * 0.56, 1.84, 0.0), ONE); // coude
            plate(b, GOLD, [0.17, 0.5, 0.2], 0.03, v(s * 0.56, 1.62, 0.14), v(-0.5, 0.0, 0.0)); // avant-bras
            plate(b, ENAMEL, [0.12, 0.16, 0.14], 0.02, v(s * 0.56, 1.38, 0.3), NO_TURN); // main
            ball(b, GLOW, 0.035, v(s * 0.07, 2.86, 0.215), ONE); // œil
            // Lames latérales de la couronne.
            blade(b, GOLD, v(s * 0.55, 2.6, 0.0), v(0.0, 0.0, -s * 0.7), 0.3);
        });
        plate(b, ENAMEL, [0.56, 0.22, 0.3], 0.05, v(0.0, 1.62, 0.0), NO_TURN); // bassin
        cyl(b, OLD_GOLD, 0.14, 0.18, 0.3, v(0.0, 1.82, 0.0), NO_TURN); // taille
        // Buste : un écu évasé vers les épaules, en émail du joueur.
        let chest = [[-0.28, 0.0], [0.28, 0.0], [0.46, 0.5], [0.36, 0.62], [-0.36, 0.62], [-0.46, 0.5]];
        prism(b, TEAM, &chest, 0.34, 0.05, Transform::from_xyz(0.0, 1.94, 0.0));
        octa(b, EMERALD_STONE, 0.12, v(0.0, 2.28, 0.2), ONE); // émeraude du plastron
        plate(b, OLD_GOLD, [0.08, 0.4, 0.2], 0.02, v(0.0, 2.3, -0.26), NO_TURN); // arête dorsale
        cyl(b, ENAMEL, 0.07, 0.08, 0.14, v(0.0, 2.62, 0.0), NO_TURN); // cou
        ball(b, GOLD, 0.2, v(0.0, 2.84, 0.0), v(0.9, 1.1, 1.0)); // tête
        ball(b, ENAMEL, 0.17, v(0.0, 2.83, 0.07), v(0.85, 1.0, 0.9)); // visage
        // L'arme, le long de l'avant-bras droit.
        plate(b, ENAMEL, [0.08, 0.12, 1.1], 0.02, v(0.56, 1.4, 0.7), NO_TURN);
        bar(b, GLOW, [0.02, 0.03, 0.7], v(0.56, 1.47, 0.75));
        ball(b, GLOW, 0.05, v(0.56, 1.4, 1.28), ONE);
        // Lame centrale de la couronne, penchée en arrière.
        blade(b, GOLD, v(0.0, 2.98, -0.1), v(-0.45, 0.0, 0.0), 0.4);
    });
    b
}

/// Perceur : scorpion d'assaut anti-blindé, monté comme un collier. Carapace
/// d'or bombée sur un ventre d'émail vert, un grand cabochon d'émail du
/// joueur serti sur le dos ; la queue est un rang de neuf perles d'or qui
/// diminuent, une sur trois sertie à la couleur du joueur, et se termine par
/// une émeraude taille brillant qui pulse (le dard). Bras en joncs d'or
/// vieilli, pinces en anneaux ouverts, six pattes fines aux pointes d'or.
fn perceur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(0.62), |b| {
        ball(b, GOLD, 0.6, v(0.0, 0.95, 0.2), v(1.1, 0.5, 1.5)); // carapace
        ball(b, ENAMEL, 0.55, v(0.0, 0.82, 0.2), v(1.0, 0.4, 1.4)); // ventre
        cab(b, TEAM, 0.3, v(0.0, 1.24, 0.2), v(-FRAC_PI_2, 0.0, 0.0)); // cabochon du dos
        ball(b, GOLD, 0.3, v(0.0, 0.95, 1.2), v(1.2, 0.6, 1.0)); // tête
        // La queue : neuf perles réparties le long d'une courbe.
        let tail = [v(0.0, 1.05, -1.0), v(0.0, 1.8, -1.7), v(0.0, 2.7, -1.6), v(0.0, 3.3, -1.0), v(0.0, 3.4, -0.3)];
        for i in 0..9 {
            let slot = if i % 3 == 2 { TEAM } else { GOLD };
            ball(b, slot, 0.27 - i as f32 * 0.017, curve_point(&tail, i as f32 / 8.0), ONE);
        }
        b.pulsing(GEM_PULSE, |b| brill(b, EMERALD_STONE, 0.3, v(0.0, 3.3, 0.1), v(2.2, 0.0, 0.0))); // dard
        b.mirrored(|b, s| {
            octa(b, EMERALD_STONE, 0.06, v(s * 0.14, 1.0, 1.45), ONE); // œil
            tube(b, OLD_GOLD, &[[s * 0.5, 0.9, 1.0], [s * 1.1, 1.0, 1.5], [s * 1.0, 1.0, 1.95]], 0.07); // bras
            // Pince : un anneau d'or ouvert, couché à plat (comme sur la
            // page, l'ouverture est tournée du même côté pour les deux pinces).
            arc(b, GOLD, 0.35, 0.08, PI * 1.3, v(s * 1.0, 1.0, 2.25), v(-FRAC_PI_2, 0.0, 0.0), -PI * 0.15);
            for (z, dz) in [(0.6, 0.2), (0.1, 0.0), (-0.4, -0.25)] {
                // Patte en arc, de la hanche au sol, et sa pointe d'or.
                tube(b, OLD_GOLD, &[[s * 0.5, 0.85, z], [s * 1.3, 1.5, z + dz], [s * 1.75, 0.06, z + dz * 2.0]], 0.045);
                cyl(b, GOLD, 0.0, 0.07, 0.22, v(s * 1.75, 0.12, z + dz * 2.0), NO_TURN);
            }
        });
    });
    b
}

/// Collecteur : l'ouvrier, un scarabée volant. Corps d'émail vert, tête d'or,
/// élytres en émail aux couleurs du joueur, antennes fines, deux ailes
/// translucides qui battent. Sous le ventre, l'émeraude qu'il rapporte.
/// Il vole à 1,3 m du sol.
fn collecteur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(Transform::from_xyz(0.0, 1.3, 0.0) * page(1.0), |b| {
        ball(b, ENAMEL, 0.4, Vec3::ZERO, v(1.0, 0.6, 1.3)); // corps
        ball(b, GOLD, 0.2, v(0.0, 0.02, 0.52), v(1.1, 0.75, 1.0)); // tête
        octa(b, EMERALD_STONE, 0.16, v(0.0, -0.3, 0.05), ONE); // cargaison
        b.mirrored(|b, s| {
            // Élytre : une demi-coque aplatie, inclinée vers l'extérieur.
            b.add(TEAM, PartShape::Sphere { radius: 0.42 }, placed(v(s * 0.3, 0.28, -0.1), v(0.0, 0.0, s * 0.5), v(0.5, 0.3, 1.2)));
            ball(b, GLOW, 0.05, v(s * 0.1, 0.06, 0.68), ONE); // œil
            tube(b, ENAMEL, &[[s * 0.06, 0.12, 0.66], [s * 0.16, 0.3, 0.85], [s * 0.24, 0.28, 1.02]], 0.015); // antenne
            for k in 0..3 {
                // Pattes repliées sous le ventre.
                cyl(b, ENAMEL, 0.015, 0.02, 0.36, v(s * 0.3, -0.3, 0.3 - k as f32 * 0.28), v(0.0, 0.0, s * 0.5));
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

/// Une guêpe-drone, dans son propre repère (avant vers +z) : thorax en
/// losange d'or, abdomen en cône d'émail aux couleurs du joueur (pointe vers
/// l'arrière), tête d'émail vert, yeux et dard émeraude, deux petites ailes.
fn wasp(b: &mut ModelBuilder) {
    octa(b, GOLD, 0.16, Vec3::ZERO, v(1.0, 0.5, 1.4)); // thorax
    cyl(b, TEAM, 0.0, 0.08, 0.36, v(0.0, -0.02, -0.36), v(-FRAC_PI_2, 0.0, 0.0)); // abdomen
    ball(b, ENAMEL, 0.08, v(0.0, 0.02, 0.26), ONE); // tête
    b.mirrored(|b, s| ball(b, GLOW, 0.03, v(s * 0.04, 0.04, 0.32), ONE)); // yeux
    ball(b, GLOW, 0.03, v(0.0, -0.02, -0.56), ONE); // pointe du dard
    // Ailes réduites de 35 % par rapport à la page : en test de charge, des
    // dizaines d'Essaims aux grandes ailes lumineuses formaient un tapis coloré.
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

/// Essaim : une nuée de sept guêpes en orbite autour d'un centre vide, sur
/// deux anneaux qui tournent en sens contraires (la page en montre 28 qui
/// tournent chacune à sa vitesse : trop d'animations pour une unité qu'on
/// produit en masse). De dessus : un cercle de petits points d'or et de
/// couleur d'équipe qui tourbillonne.
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

/// Essaimeur : le grand vaisseau porte-drones, un bracelet ouvert qui vole à
/// plat. Un jonc d'or épais sur un lit d'émail vert, bordé de deux filets
/// d'or vieilli, serti de sept cabochons d'émail aux couleurs du joueur ; à
/// chaque bout, une boule d'or coiffée d'une émeraude, et dans l'ouverture,
/// à l'avant, l'orbe : une grosse émeraude qui tourne sur elle-même en
/// pulsant. Trois émeraudes pendent sous le jonc. Six anneaux d'or, chacun
/// autour d'un éclat d'émeraude, l'escortent en orbite : ce sont les drones
/// qu'il lancera au combat. Il vole à 3,2 m du sol.
///
/// Ce modèle-là est dessiné directement dans notre repère (sans [`page`]) :
/// le tore du moteur est déjà couché à plat, et l'ouverture du bracelet est
/// placée à l'avant (−z). Un point d'angle θ du jonc est en (R cos θ, ·, R sin θ).
fn essaimeur() -> ModelBuilder {
    // Cotes de la page : rayon du jonc, longueur de l'arc (1,62 π sur 2 π).
    const R: f32 = 3.0;
    const SPAN: f32 = PI * 1.62;
    // L'ouverture est centrée sur −z (θ = 3π/2) : l'arc commence juste après.
    let start = 1.5 * PI + (TAU - SPAN) / 2.0;
    let on_ring = |angle: f32, radius: f32, y: f32| v(radius * angle.cos(), y, radius * angle.sin());
    // Tourner un tore de α autour de y retranche α à ses angles : pour que
    // l'arc commence en `start`, on le tourne de −start.
    let band = |b: &mut ModelBuilder, slot: MaterialSlot, radius: f32, thickness: f32, y: f32, squash: f32| {
        let placement = Transform::from_xyz(0.0, y, 0.0)
            .with_rotation(Quat::from_rotation_y(-start))
            .with_scale(v(1.0, squash, 1.0));
        b.add(slot, PartShape::Torus { major: radius, minor: thickness, arc: SPAN }, placement);
    };

    let mut b = ModelBuilder::new();
    b.group(Transform::from_xyz(0.0, 3.2, 0.0).with_scale(Vec3::splat(0.55)), |b| {
        band(b, GOLD, R, 0.46, 0.0, 1.0); // le jonc
        band(b, OLD_GOLD, R - 0.62, 0.09, 0.1, 1.0); // filet intérieur
        band(b, OLD_GOLD, R + 0.6, 0.07, -0.05, 1.0); // filet extérieur
        band(b, ENAMEL, R, 0.48, -0.12, 0.6); // lit d'émail, aplati, qui déborde dessous
        // Les sept cabochons sur le dessus, petits et grands en alternance.
        for k in 1..8 {
            let at = on_ring(start + SPAN * k as f32 / 8.0, R, 0.4);
            let radius = if k % 2 == 1 { 0.24 } else { 0.32 };
            // `cab` est dessiné dans le plan (x, y) : un quart de tour le couche à plat.
            cab(b, TEAM, radius, at, v(-FRAC_PI_2, 0.0, 0.0));
        }
        // Les deux bouts : une boule d'or coiffée d'une émeraude, table vers le haut.
        for angle in [start, start + SPAN] {
            let end = on_ring(angle, R, 0.0);
            ball(b, GOLD, 0.62, end, ONE);
            brill(b, EMERALD_STONE, 0.36, end + v(0.0, 0.62, 0.0), NO_TURN);
        }
        // Trois émeraudes pendues sous le jonc, pointe en haut.
        for t in [0.3, 0.5, 0.7] {
            brill(b, EMERALD_STONE, 0.22, on_ring(start + SPAN * t, R, -0.95), v(PI, 0.0, 0.0));
        }
        // L'orbe, dans l'ouverture : elle tourne sur un axe penché en pulsant.
        let spin = Motion::Spin { axis: v(0.5, 0.8, 0.0).normalize(), speed: 0.95 };
        b.animated(Transform::from_translation(on_ring(1.5 * PI, R, 0.0)), spin, |b| {
            b.pulsing(GEM_PULSE, |b| ico(b, EMERALD_STONE, 0.75));
        });
        // L'escorte : six anneaux d'or dressés, chacun autour d'un éclat
        // d'émeraude, qui tournent autour du bracelet un peu plus bas, chacun
        // tourné dans le sens de sa course (on regarde à travers l'anneau).
        // Agrandis de 80 % par rapport à la page : à leur taille d'origine
        // (22 cm), on ne les voyait pas de la caméra.
        let escort = Motion::Spin { axis: Vec3::Y, speed: 0.6 };
        b.animated(Transform::from_xyz(0.0, -0.6, 0.0), escort, |b| {
            for j in 0..6 {
                let angle = j as f32 * TAU / 6.0;
                let radius = 4.4 + (j % 3) as f32 * 0.45;
                let lift = (j % 2) as f32 * 0.5 - 0.25;
                // La tangente au cercle en θ est (−sin θ, 0, cos θ) : on y amène
                // l'axe de l'anneau (y → z par un quart de tour, puis z → tangente).
                let facing = Quat::from_rotation_y(-angle) * Quat::from_rotation_x(FRAC_PI_2);
                let at = Transform::from_translation(on_ring(angle, radius, lift)).with_rotation(facing).with_scale(Vec3::splat(1.8));
                b.group(at, |b| {
                    b.torus(GOLD, 0.22, 0.05, TAU, Vec3::ZERO, NO_TURN);
                    octa(b, EMERALD_STONE, 0.1, Vec3::ZERO, v(1.0, 1.0, 1.6));
                });
            }
        });
    });
    b
}

/// Titan : le colosse serti. Jambes et bras en plaques d'or et d'émail vert,
/// genoux et taille en prismes hexagonaux d'or vieilli ; un buste hexagonal
/// d'or ceint d'un filet émeraude, et au milieu le cœur, une grosse émeraude
/// cerclée d'or qui tourne et pulse. Le bras droit est un canon hexagonal à
/// bouche émeraude, le gauche un poing à trois griffes. Les épaules sont de
/// grands prismes hexagonaux d'or ourlés d'émeraude, terminés par une
/// émeraude taillée. Derrière la tête se dresse une couronne de cinq lames
/// d'or, et deux lames d'or vieilli sur les épaules. Le joug des épaules et
/// la crête sont en émail du joueur : de dessus, une barre de couleur sous
/// la couronne. 4,5 m.
fn titan() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(0.62), |b| {
        b.mirrored(|b, s| {
            // Jambes.
            plate(b, ENAMEL, [1.0, 0.4, 1.7], 0.1, v(s * 1.05, 0.2, 0.2), NO_TURN); // pied
            plate(b, GOLD, [0.9, 0.32, 0.55], 0.08, v(s * 1.05, 0.26, 0.95), NO_TURN); // orteils
            plate(b, GOLD, [0.7, 0.5, 0.4], 0.08, v(s * 1.05, 0.45, -0.55), NO_TURN); // talon
            plate(b, GOLD, [0.85, 1.5, 0.95], 0.12, v(s * 1.05, 1.3, 0.0), NO_TURN); // tibia
            bar(b, GLOW, [0.1, 1.0, 0.04], v(s * 1.05, 1.3, 0.49));
            plate(b, ENAMEL, [0.6, 1.2, 0.3], 0.06, v(s * 1.05, 1.3, -0.55), NO_TURN); // mollet
            hexc(b, OLD_GOLD, 0.52, 0.52, 1.0, v(s * 1.05, 2.2, 0.05), ALONG_X); // genou
            plate(b, GOLD, [0.66, 0.56, 0.22], 0.08, v(s * 1.05, 2.25, 0.58), NO_TURN); // rotule
            plate(b, ENAMEL, [0.72, 1.1, 0.85], 0.1, v(s * 0.95, 2.95, -0.05), NO_TURN); // cuisse
            plate(b, GOLD, [0.5, 0.8, 0.14], 0.05, v(s * 0.95, 2.95, 0.44), NO_TURN);
            // Épaule : grand prisme hexagonal couché, filet émeraude, disque
            // d'émail du joueur et émeraude au bout.
            hexc(b, GOLD, 0.9, 0.9, 1.15, v(s * 2.45, 5.7, 0.0), ALONG_X);
            hexc(b, GLOW, 0.93, 0.93, 0.1, v(s * 2.2, 5.7, 0.0), ALONG_X);
            hexc(b, TEAM, 0.6, 0.6, 0.08, v(s * 3.05, 5.7, 0.0), ALONG_X);
            octa(b, EMERALD_STONE, 0.3, v(s * 3.12, 5.7, 0.0), ONE);
            plate(b, ENAMEL, [0.62, 1.3, 0.72], 0.08, v(s * 2.5, 4.55, 0.0), NO_TURN); // bras
            // Aileron dorsal et lame d'épaule.
            plate(b, GOLD, [0.16, 1.2, 0.9], 0.05, v(s * 0.7, 6.1, -1.15), v(0.5, 0.0, -s * 0.25));
            blade(b, OLD_GOLD, v(s * 2.45, 6.55, -0.2), v(0.0, 0.0, -s * 0.5), 0.8);
            ball(b, GLOW, 0.07, v(s * 0.16, 6.74, 0.72), ONE); // œil
        });
        // Bras droit : le canon hexagonal, bague d'émail et bouche émeraude.
        hexc(b, GOLD, 0.5, 0.62, 1.9, v(2.5, 3.75, 0.65), ALONG_Z);
        hexc(b, ENAMEL, 0.64, 0.64, 0.12, v(2.5, 3.75, 0.1), ALONG_Z);
        hexc(b, GLOW, 0.38, 0.38, 0.06, v(2.5, 3.75, 1.62), ALONG_Z);
        bar(b, GLOW, [0.06, 0.06, 1.2], v(2.5, 4.3, 0.7));
        // Bras gauche : avant-bras d'or, poing d'émail, trois griffes d'or.
        plate(b, GOLD, [0.62, 1.5, 0.72], 0.08, v(-2.5, 3.5, 0.25), v(-0.25, 0.0, 0.0));
        plate(b, ENAMEL, [0.64, 0.55, 0.66], 0.1, v(-2.5, 2.62, 0.5), NO_TURN);
        for k in 0..3 {
            plate(b, GOLD, [0.17, 0.32, 0.22], 0.03, v(-2.5 + (k as f32 - 1.0) * 0.21, 2.25, 0.68), NO_TURN);
        }
        bar(b, GLOW, [0.5, 0.05, 0.03], v(-2.5, 2.72, 0.84));
        // Le corps.
        plate(b, ENAMEL, [2.7, 0.62, 1.15], 0.1, v(0.0, 3.55, 0.0), NO_TURN); // bassin
        plate(b, GOLD, [0.7, 0.5, 0.3], 0.08, v(0.0, 3.5, 0.55), NO_TURN);
        hexc(b, OLD_GOLD, 0.72, 0.85, 0.55, v(0.0, 4.05, 0.0), NO_TURN); // taille
        hexc(b, GOLD, 1.75, 1.35, 1.85, v(0.0, 5.15, 0.0), NO_TURN); // buste
        hexc(b, GLOW, 1.73, 1.73, 0.08, v(0.0, 5.86, 0.0), NO_TURN); // filet du buste
        plate(b, ENAMEL, [1.3, 0.95, 0.2], 0.06, v(0.0, 5.15, 1.32), NO_TURN); // plastron
        bar(b, GLOW, [0.9, 0.05, 0.04], v(0.0, 4.5, 1.24));
        // Le cœur : un jonc d'or vieilli (fixe), et l'émeraude qui tourne
        // lentement sur elle-même en pulsant.
        ring(b, OLD_GOLD, 0.5, 0.07, v(0.0, 5.15, 1.42), NO_TURN);
        b.animated(Transform::from_xyz(0.0, 5.15, 1.42), Motion::Spin { axis: Vec3::Y, speed: 0.4 }, |b| {
            b.pulsing(GEM_PULSE, |b| ico(b, EMERALD_STONE, 0.42));
        });
        plate(b, TEAM, [3.7, 0.5, 1.25], 0.1, v(0.0, 6.2, 0.0), NO_TURN); // joug des épaules
        ball(b, GOLD, 0.5, v(0.0, 6.74, 0.22), v(0.95, 1.0, 1.05)); // tête
        ball(b, ENAMEL, 0.42, v(0.0, 6.68, 0.4), v(0.85, 0.9, 0.8)); // visage
        bar(b, GLOW, [0.3, 0.03, 0.02], v(0.0, 6.5, 0.7));
        plate(b, TEAM, [0.12, 0.5, 0.9], 0.03, v(0.0, 7.2, 0.1), NO_TURN); // crête
        // Réacteur dorsal.
        hexc(b, OLD_GOLD, 0.5, 0.5, 0.8, v(0.0, 5.4, -1.5), ALONG_Z);
        hexc(b, GLOW, 0.36, 0.36, 0.06, v(0.0, 5.4, -1.92), ALONG_Z);
        // La couronne : cinq lames d'or en éventail derrière la tête.
        for k in -2..=2 {
            let k = k as f32;
            blade(b, GOLD, v(k * 0.42, 6.45, -0.75), v(-0.35, 0.0, -k * 0.32), 1.0);
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
            println!("{} : {} sommets, {} pièces", unit.name, vertices, unit.parts.len());
            assert!(vertices < 14_000, "{} est trop lourd : {vertices} sommets", unit.name);
        }
    }

    /// La courbe de la queue passe bien par ses deux bouts.
    #[test]
    fn curve_reaches_its_ends() {
        let points = [Vec3::ZERO, Vec3::X, Vec3::new(2.0, 1.0, 0.0)];
        assert!(curve_point(&points, 0.0).distance(points[0]) < 1e-5);
        assert!(curve_point(&points, 1.0).distance(points[2]) < 1e-5);
        assert!(curve_point(&points, 0.5).distance(points[1]) < 1e-5);
    }
}
