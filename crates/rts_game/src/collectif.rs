//! Le Collectif : palette, ambiance, unités et bâtiments (faction en exploration visuelle).
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
//!
//! Les bâtiments des paliers 1 et 2 (section « Les bâtiments ») suivent la même
//! direction artistique : des machines aux lignes de joaillerie (socles
//! hexagonaux d'émail, griffes de chaton, arches d'or, rangs de perles,
//! émeraudes taillées), pas des bijoux posés au sol. Ils se construisent
//! par **sertissage à distance** (voir [`CONSTRUCTION`]) et, pour certains,
//! seulement dans un **champ de résonance** émis par l'Écrin ou un
//! Résonateur (voir `rts_engine::buildings::field`).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use rts_engine::buildings::{BuildingType, ConstructionStyle, Cost, PlacementRule};
use rts_engine::economy::production::{ProductionOption, Requirement, UnitEconomy};
use rts_engine::economy::{EconomyRules, ResourceInfo, ResourceKind};
use rts_engine::graphics::shapes::catmull_rom;
use rts_engine::graphics::{
    Ambiance, EnvironmentSettings, FogSettings, ParticleSettings, ReflectionPanel, ReflectionPanels, SceneLight,
};
use rts_engine::units::{
    HoverMotion, MaterialSlot, ModelBuilder, Motion, Palette, Part, PartShape, Pulse, SurfaceDetail, SurfaceStyle, UnitType,
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

// ---------------------------------------------------------------------------
// Les bâtiments (paliers 1 et 2, propositions : voir GAME_DESIGN.md).
// ---------------------------------------------------------------------------

/// Façon de construire du Collectif : le **sertissage à distance**. Le
/// Collecteur vole jusqu'à l'emplacement, y pose une gemme d'ancrage et
/// repart aussitôt récolter ; le bâtiment s'assemble seul. Annulé, un
/// chantier rend 75 % de son coût.
pub const CONSTRUCTION: ConstructionStyle = ConstructionStyle::RemoteAnchor { refund_percent: 75 };

/// Couleur du champ de résonance dessiné au sol pendant la pose : le halo vert d'eau.
pub const FIELD_COLOR: Color = HALO_COLOR;

/// Rayons des champs de résonance (mètres, depuis le centre du bâtiment) :
/// l'Écrin couvre ses abords (4,5 m au-delà de son bord), le Résonateur un
/// cercle où tiennent deux ou trois bâtiments.
pub const CORE_FIELD_RADIUS: f32 = 8.0;
pub const RELAY_FIELD_RADIUS: f32 = 6.5;

/// Indices des bâtiments dans [`building_types`].
pub mod building {
    pub const ECRIN: usize = 0;
    pub const RESONATEUR: usize = 1;
    pub const CREUSET: usize = 2;
    pub const ORFEVRERIE: usize = 3;
    pub const AIGUILLON: usize = 4;
    pub const LAPIDAIRE: usize = 5;
    pub const VOLIERE: usize = 6;
    pub const FONDERIE: usize = 7;
}

/// Les bâtiments du palier 1, mêmes rôles que ceux de la Garde et des Sylves :
///
/// - **Écrin** (7 × 7 m) : le cœur de la base, dépôt des récoltes, produit
///   les Collecteurs, 15 de population, émet un champ de résonance ;
/// - **Résonateur** (2 × 2 m) : le relais, 8 de population, émet le champ
///   de résonance où se posent les autres bâtiments ;
/// - **Creuset** (4 × 4 m) : posé sur une source d'Éther, il la distille ;
///   les Collecteurs le récoltent à son bord ;
/// - **Orfèvrerie** (5 × 5 m) : l'atelier des Automates (et des Perceurs,
///   une fois le Lapidaire terminé) ;
/// - **Aiguillon** (2 × 2 m) : défense fixe, débloquée par l'Orfèvrerie.
///
/// Puis le palier 2, en miroir des rôles de la Garde (Armurerie, Centre de
/// propulsion, Usine), avec l'identité du Collectif : tout passe par les
/// pierres taillées, et l'air a son propre atelier.
///
/// - **Lapidaire** (4 × 4 m, demande l'Orfèvrerie) : il taille les grosses
///   pierres. Ne produit rien lui-même, mais débloque le Perceur (à
///   l'Orfèvrerie), l'Essaimeur (à la Volière) et la Fonderie ;
/// - **Volière** (5 × 5 m, demande l'Orfèvrerie) : l'atelier aérien, produit
///   les Essaims, et les Essaimeurs une fois le Lapidaire terminé ;
/// - **Fonderie** (6 × 6 m, demande le Lapidaire) : coule les Titans.
///
/// Tous, sauf l'Écrin, le Résonateur et le Creuset, ne se posent que **dans
/// un champ de résonance** (celui de l'Écrin ou d'un Résonateur terminé) ;
/// ces trois-là se posent partout (le Résonateur, pour étendre le champ ;
/// l'Écrin, pour s'étendre vers d'autres gisements).
///
/// Chiffres proposés, à ajuster en test : mêmes prix en Cristal que la
/// Garde à peu près (le Collecteur ne perd pas son temps sur le chantier,
/// mais le champ contraint la pose), temps de construction un peu plus longs.
/// Le palier 2 coûte de l'Éther : il faut un Creuset avant de s'y lancer.
pub fn building_types() -> Vec<BuildingType> {
    use building::*;
    vec![
        BuildingType {
            dropoff: true,
            supply: 15,
            field_radius: CORE_FIELD_RADIUS,
            production: vec![option(kind::COLLECTEUR, 'c', &[])],
            ..building_type("Écrin", (7, 7), 1500.0, 75.0, (400, 0), &[], 'e', ecrin())
        },
        BuildingType {
            supply: 8,
            field_radius: RELAY_FIELD_RADIUS,
            ..building_type("Résonateur", (2, 2), 300.0, 25.0, (100, 0), &[], 'r', resonateur())
        },
        BuildingType {
            placement: PlacementRule::OnResource(ResourceKind::Rare),
            ..building_type("Creuset", (4, 4), 450.0, 25.0, (75, 0), &[], 'c', creuset())
        },
        // L'Orfèvrerie assemble les Automates, et les Perceurs dès qu'un
        // Lapidaire est terminé (leur dard est une pierre taillée).
        BuildingType {
            needs_field: true,
            production: vec![option(kind::AUTOMATE, 'a', &[]), option(kind::PERCEUR, 'p', &[LAPIDAIRE])],
            ..building_type("Orfèvrerie", (5, 5), 1000.0, 50.0, (150, 0), &[ECRIN], 'o', orfevrerie())
        },
        BuildingType {
            needs_field: true,
            ..building_type("Aiguillon", (2, 2), 300.0, 25.0, (100, 0), &[ORFEVRERIE], 'a', aiguillon())
        },
        // Palier 2. Le Lapidaire ne produit rien : il débloque.
        BuildingType {
            needs_field: true,
            ..building_type("Lapidaire", (4, 4), 750.0, 50.0, (150, 100), &[ORFEVRERIE], 'l', lapidaire())
        },
        // La Volière : les Essaims tout de suite, les Essaimeurs (leur orbe
        // est une grosse émeraude) avec le Lapidaire.
        BuildingType {
            needs_field: true,
            production: vec![option(kind::ESSAIM, 'e', &[]), option(kind::ESSAIMEUR, 'm', &[LAPIDAIRE])],
            ..building_type("Volière", (5, 5), 1000.0, 55.0, (150, 100), &[ORFEVRERIE], 'v', voliere())
        },
        BuildingType {
            needs_field: true,
            production: vec![option(kind::TITAN, 't', &[])],
            ..building_type("Fonderie", (6, 6), 1250.0, 65.0, (200, 150), &[LAPIDAIRE], 'f', fonderie())
        },
    ]
}

/// Règles économiques du Collectif : les Collecteurs récoltent avec les
/// mêmes cadences que les Techniciens (5 de Cristal en 2,8 s, 4 d'Éther en
/// 2 s, au bord du Creuset) ; ils volent un peu plus vite qu'eux. Coûts,
/// temps de production (en secondes) et population proposés : des unités
/// chères et solides, peu nombreuses.
pub fn economy_rules(resources: [ResourceInfo; 2]) -> EconomyRules {
    // Dans l'ordre de `unit_types` : (Cristal, Éther), temps, population.
    let units = [
        ((50, 0), 12.0, 1), // Collecteur
        ((100, 0), 24.0, 2), // Automate
        ((125, 50), 30.0, 3), // Perceur
        ((75, 50), 25.0, 2), // Essaim
        ((300, 200), 60.0, 6), // Essaimeur
        ((350, 250), 70.0, 8), // Titan
    ];
    EconomyRules {
        resources,
        harvesters: vec![kind::COLLECTEUR],
        carry: [5, 4],
        gather_time: [2.8, 2.0],
        units: units
            .iter()
            .map(|&((primary, rare), train_time, population)| UnitEconomy { cost: Cost::new(primary, rare), train_time, population })
            .collect(),
    }
}

/// Une unité produite par un bâtiment ; `requires` : indices (dans
/// [`building_types`]) des bâtiments terminés qu'il faut posséder pour la
/// produire (le bouton reste grisé en attendant).
fn option(unit_type: usize, hotkey: char, requires: &[usize]) -> ProductionOption {
    ProductionOption { unit_type, hotkey, requires: requires.iter().map(|&index| Requirement::Building(index)).collect() }
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

/// La gemme d'ancrage que pose le Collecteur : une émeraude taille brillant,
/// table vers le haut, tenue dans un jonc d'or à quatre griffes, qui tourne
/// lentement au-dessus du chantier pendant qu'il s'assemble.
pub fn anchor_gem() -> Vec<Part> {
    let mut b = ModelBuilder::new();
    b.animated(Transform::from_xyz(0.0, 0.5, 0.0), Motion::Spin { axis: Vec3::Y, speed: 1.1 }, |b| {
        brill(b, EMERALD_STONE, 0.42, Vec3::ZERO, NO_TURN);
        b.torus(GOLD, 0.44, 0.06, TAU, v(0.0, 0.02, 0.0), NO_TURN);
        for i in 0..4 {
            let a = i as f32 * TAU / 4.0 + 0.4;
            let (sin, cos) = a.sin_cos();
            tube(b, GOLD, &[[cos * 0.44, 0.0, sin * 0.44], [cos * 0.4, 0.2, sin * 0.4], [cos * 0.26, 0.26, sin * 0.26]], 0.035);
        }
    });
    b.build()
}

/// Une griffe de chaton : tube d'or de `from` à `to`, bombé vers l'extérieur
/// en son milieu (`bulge`), terminé par une perle d'or.
fn prong(b: &mut ModelBuilder, from: Vec3, to: Vec3, bulge: f32, radius: f32) {
    let mid = (from + to) / 2.0;
    let out = mid.with_y(0.0).normalize_or_zero() * bulge;
    b.tube(GOLD, &[from, mid + out, to], radius);
    ball(b, GOLD, radius * 1.5, to, ONE);
}

/// Écrin : le cœur de la base, 5 m de haut. Un socle hexagonal d'émail vert
/// sur une semelle d'or vieilli, ourlé d'or ; dessus, un plateau laqué aux
/// couleurs du joueur cerclé d'un rang de perles d'or, et un tambour d'or
/// d'où montent six griffes (le chaton de la machine) qui tiennent une
/// grosse émeraude taille brillant, table vers le ciel ; elle tourne
/// lentement en pulsant. Au bout de chaque griffe, une lame d'or de
/// couronne. En façade (−z), le dépôt : une arche d'or dressée autour d'un
/// disque émeraude, où les Collecteurs versent leur récolte. Vu de dessus :
/// un hexagone vert, un anneau d'équipe et une étoile d'or autour d'une
/// émeraude.
fn ecrin() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    hexc(&mut b, OLD_GOLD, 3.4, 3.45, 0.3, v(0.0, 0.15, 0.0), NO_TURN); // semelle
    hexc(&mut b, ENAMEL, 3.0, 3.25, 0.6, v(0.0, 0.6, 0.0), NO_TURN); // socle
    hexc(&mut b, GOLD, 3.08, 3.08, 0.1, v(0.0, 0.93, 0.0), NO_TURN); // ourlet
    hexc(&mut b, TEAM, 2.35, 2.55, 0.2, v(0.0, 1.06, 0.0), NO_TURN); // plateau d'équipe
    for i in 0..12 {
        let a = i as f32 * TAU / 12.0;
        ball(&mut b, GOLD, 0.13, v(a.cos() * 2.62, 1.0, a.sin() * 2.62), ONE); // rang de perles
    }
    hexc(&mut b, GOLD, 1.2, 1.4, 0.7, v(0.0, 1.5, 0.0), NO_TURN); // tambour
    hexc(&mut b, GLOW, 1.22, 1.22, 0.06, v(0.0, 1.62, 0.0), NO_TURN); // filet émeraude
    for i in 0..6 {
        let a = i as f32 * TAU / 6.0 + PI / 6.0;
        let (sin, cos) = a.sin_cos();
        let at = |r: f32, y: f32| v(cos * r, y, sin * r);
        prong(&mut b, at(1.25, 1.8), at(1.0, 4.1), 0.75, 0.11);
        // Lame de couronne au bout de la griffe, penchée vers l'extérieur.
        let turn = Quat::from_rotation_y(-a) * Quat::from_rotation_z(-0.45);
        prism(&mut b, GOLD, &BLADE, 0.06, 0.02, Transform::from_translation(at(1.02, 4.05)).with_rotation(turn).with_scale(Vec3::splat(0.55)));
    }
    // La pierre, tenue entre les griffes.
    b.animated(Transform::from_xyz(0.0, 3.35, 0.0), Motion::Spin { axis: Vec3::Y, speed: 0.25 }, |b| {
        b.pulsing(GEM_PULSE, |b| brill(b, EMERALD_STONE, 1.2, Vec3::ZERO, NO_TURN));
    });
    // Le dépôt, en façade.
    torus(&mut b, GOLD, 0.9, 0.13, TAU, Transform::from_xyz(0.0, 1.75, -2.95));
    b.cylinder(EMERALD_STONE, 0.74, 0.04, v(0.0, 1.75, -2.95), ALONG_Z);
    // Deux cabochons d'équipe l'encadrent, bombés vers l'avant.
    b.mirrored(|b, s| cab(b, TEAM, 0.22, v(s * 1.35, 1.25, -2.98), v(0.0, PI, 0.0)));
    b
}

/// Résonateur : le relais, 3,8 m. Un pied hexagonal d'émail, ourlé d'or,
/// coiffé d'un disque laqué aux couleurs du joueur ; une colonne d'or
/// effilée baguée d'or vieilli ; au sommet, trois griffes tiennent une
/// émeraude en losange qui pulse (c'est elle qui émet le champ), et un
/// anneau d'or ouvert tourne autour d'elle. Vu de dessus : un petit
/// hexagone d'équipe, une pierre et un croissant d'or.
fn resonateur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    hexc(&mut b, ENAMEL, 0.9, 0.98, 0.36, v(0.0, 0.18, 0.0), NO_TURN); // pied
    hexc(&mut b, GOLD, 0.93, 0.93, 0.07, v(0.0, 0.39, 0.0), NO_TURN);
    hexc(&mut b, TEAM, 0.72, 0.8, 0.12, v(0.0, 0.48, 0.0), NO_TURN); // disque d'équipe
    hexc(&mut b, GOLD, 0.16, 0.3, 2.1, v(0.0, 1.55, 0.0), NO_TURN); // colonne
    for y in [0.95, 1.9] {
        hexc(&mut b, OLD_GOLD, 0.27, 0.27, 0.09, v(0.0, y, 0.0), NO_TURN); // bagues
    }
    for i in 0..3 {
        let a = i as f32 * TAU / 3.0;
        let (sin, cos) = a.sin_cos();
        prong(&mut b, v(cos * 0.14, 2.55, sin * 0.14), v(cos * 0.24, 3.55, sin * 0.24), 0.32, 0.06);
    }
    b.pulsing(GEM_PULSE, |b| octa(b, EMERALD_STONE, 0.4, v(0.0, 3.1, 0.0), v(1.0, 1.4, 1.0)));
    // L'anneau de résonance : ouvert, incliné, il tourne autour de la pierre.
    b.animated(Transform::from_xyz(0.0, 3.1, 0.0), Motion::Spin { axis: Vec3::Y, speed: 0.7 }, |b| {
        b.torus(GOLD, 0.78, 0.05, PI * 1.45, Vec3::ZERO, v(0.45, 0.0, 0.0));
    });
    b
}

/// Creuset : posé sur une source d'Éther, il la distille. Trois jambes d'or
/// vieilli plongent dans la faille et portent une coupe d'or pleine d'une
/// nappe émeraude, ceinte d'un jonc d'émail vert et bordée d'un anneau
/// laqué aux couleurs du joueur ; une anse d'or l'enjambe, d'où pend une
/// émeraude pointe en bas. Vu de dessus : un disque émeraude dans un anneau
/// d'équipe, barré d'or.
fn creuset() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    for i in 0..3 {
        let a = i as f32 * TAU / 3.0 + PI / 6.0;
        let (sin, cos) = a.sin_cos();
        let at = |r: f32, y: f32| [cos * r, y, sin * r];
        tube(&mut b, OLD_GOLD, &[at(1.85, -0.05), at(1.75, 0.8), at(1.25, 1.45)], 0.13);
    }
    // La coupe : le profil monte jusqu'au bord, puis redescend à l'intérieur.
    let bowl = [[0.0, 1.15], [0.6, 1.2], [1.2, 1.5], [1.55, 2.0], [1.62, 2.4], [1.45, 2.42], [1.1, 2.15], [0.0, 2.05]];
    let profile: Vec<Vec2> = bowl.iter().map(|p| Vec2::from_array(*p)).collect();
    b.lathe(GOLD, &profile, 24, Vec3::ZERO, NO_TURN);
    b.cylinder(EMERALD_STONE, 1.28, 0.04, v(0.0, 2.25, 0.0), NO_TURN); // la nappe
    b.torus(ENAMEL, 1.42, 0.13, TAU, v(0.0, 1.75, 0.0), NO_TURN); // jonc d'émail
    b.torus(TEAM, 1.56, 0.11, TAU, v(0.0, 2.42, 0.0), NO_TURN); // bord d'équipe
    // L'anse, et la pierre pendue dessous, pointe en bas, à une bélière.
    torus(&mut b, GOLD, 1.45, 0.08, PI, Transform::from_xyz(0.0, 2.4, 0.0));
    tube(&mut b, OLD_GOLD, &[[0.0, 3.8, 0.0], [0.0, 3.6, 0.0], [0.0, 3.4, 0.0]], 0.03);
    brill(&mut b, EMERALD_STONE, 0.24, v(0.0, 3.32, 0.0), NO_TURN);
    b
}

/// Orfèvrerie : l'atelier des Automates, 3,5 m. Une dalle d'émail vert sur
/// une semelle d'or ; à ses quatre coins, des piliers hexagonaux d'or
/// coiffés d'une émeraude ; deux grandes arches d'or s'y croisent en
/// voûte, une émeraude pendue à leur croisée, au-dessus d'une enclume
/// hexagonale d'or vieilli au plateau laqué. Entre les arches, quatre
/// losanges laqués aux couleurs du joueur. Vu de dessus : une croix d'or
/// sur un carré vert, quatre losanges d'équipe.
fn orfevrerie() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    plate(&mut b, GOLD, [4.8, 0.14, 4.8], 0.05, v(0.0, 0.07, 0.0), NO_TURN); // semelle
    plate(&mut b, ENAMEL, [4.5, 0.42, 4.5], 0.14, v(0.0, 0.35, 0.0), NO_TURN); // dalle
    for (x, z) in [(1.2, 1.2), (-1.2, 1.2), (1.2, -1.2), (-1.2, -1.2)] {
        // Losange d'équipe : un carré tourné d'un huitième de tour.
        plate(&mut b, TEAM, [1.05, 0.08, 1.05], 0.03, v(x, 0.58, z), v(0.0, PI / 4.0, 0.0));
    }
    b.mirrored(|b, s| {
        for z in [-1.0, 1.0] {
            hexc(b, GOLD, 0.3, 0.38, 2.0, v(s * 1.95, 1.55, z * 1.95), NO_TURN); // pilier
            octa(b, EMERALD_STONE, 0.22, v(s * 1.95, 2.8, z * 1.95), ONE);
        }
        bar(b, GLOW, [0.05, 0.04, 3.2], v(s * 2.26, 0.4, 0.0)); // filets émeraude
    });
    // Les deux arches, dressées en croix (le tore de la page est dans le plan (x, y)).
    torus(&mut b, GOLD, 2.05, 0.15, PI, Transform::from_xyz(0.0, 0.55, 0.0));
    torus(&mut b, GOLD, 2.05, 0.15, PI, Transform::from_xyz(0.0, 0.55, 0.0).with_rotation(Quat::from_rotation_y(FRAC_PI_2)));
    tube(&mut b, OLD_GOLD, &[[0.0, 2.55, 0.0], [0.0, 2.45, 0.0], [0.0, 2.35, 0.0]], 0.04); // bélière
    brill(&mut b, EMERALD_STONE, 0.3, v(0.0, 2.24, 0.0), NO_TURN);
    hexc(&mut b, OLD_GOLD, 0.75, 0.95, 0.6, v(0.0, 0.85, 0.0), NO_TURN); // enclume
    hexc(&mut b, TEAM, 0.7, 0.7, 0.1, v(0.0, 1.2, 0.0), NO_TURN);
    b
}

/// Aiguillon : défense fixe, 2,6 m. Un pied hexagonal d'émail ourlé d'or et
/// cerclé d'un disque d'équipe, une tour d'or effilée, et une tête qui
/// balaie lentement l'horizon : coque d'or, cabochon d'équipe sur le
/// dessus, longue aiguille d'or pointée vers l'avant avec une émeraude
/// taillée à sa base, et trois lames de couronne derrière.
fn aiguillon() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    hexc(&mut b, ENAMEL, 0.88, 0.96, 0.4, v(0.0, 0.2, 0.0), NO_TURN); // pied
    hexc(&mut b, GOLD, 0.92, 0.92, 0.07, v(0.0, 0.43, 0.0), NO_TURN);
    hexc(&mut b, TEAM, 0.78, 0.84, 0.1, v(0.0, 0.51, 0.0), NO_TURN); // disque d'équipe
    hexc(&mut b, GOLD, 0.3, 0.52, 1.3, v(0.0, 1.2, 0.0), NO_TURN); // tour
    hexc(&mut b, GLOW, 0.36, 0.36, 0.05, v(0.0, 1.5, 0.0), NO_TURN);
    let sweep = Motion::Swing { axis: Vec3::Y, amplitude: 0.7, frequency: 0.09 };
    b.animated(Transform::from_xyz(0.0, 1.95, 0.0), sweep, |b| {
        ball(b, GOLD, 0.42, Vec3::ZERO, v(1.0, 0.7, 1.25)); // coque
        cab(b, TEAM, 0.26, v(0.0, 0.26, 0.05), v(-FRAC_PI_2, 0.0, 0.0)); // cabochon d'équipe
        // L'aiguille : un cône couché vers l'avant (−z).
        b.cone(GOLD, 0.11, 0.0, 1.5, v(0.0, 0.0, -1.2), v(-FRAC_PI_2, 0.0, 0.0));
        brill(b, EMERALD_STONE, 0.17, v(0.0, 0.0, -0.5), v(-FRAC_PI_2, 0.0, 0.0));
        for k in -1..=1 {
            let k = k as f32;
            blade(b, GOLD, v(k * 0.2, 0.15, 0.4), v(0.45, 0.0, -k * 0.35), 0.4);
        }
    });
    b
}

/// Lapidaire : l'atelier où l'on taille les grosses pierres, 3,8 m. Un
/// socle hexagonal d'émail ourlé d'or, au plateau laqué aux couleurs du
/// joueur ; au milieu, la meule : un grand disque d'or vieilli cerclé d'or,
/// rayé de filets émeraude et bordé de perles, qui tourne à plat sur son
/// moyeu. À l'arrière, un pilier d'or coiffé de trois lames de couronne
/// porte un bras coudé ; au bout du bras, une tige tient au-dessus de la
/// meule une émeraude taille brillant, pointe en bas, en cours de taille
/// (elle tourne lentement en pulsant). À l'avant, un présentoir : trois
/// petites pierres taillées sur des tiges d'or. Vu de dessus : un disque
/// d'or qui tourne dans un hexagone d'équipe, barré d'un bras d'or.
fn lapidaire() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    hexc(&mut b, OLD_GOLD, 1.9, 1.95, 0.25, v(0.0, 0.125, 0.0), NO_TURN); // semelle
    hexc(&mut b, ENAMEL, 1.7, 1.85, 0.5, v(0.0, 0.5, 0.0), NO_TURN); // socle
    hexc(&mut b, GOLD, 1.76, 1.76, 0.08, v(0.0, 0.77, 0.0), NO_TURN); // ourlet
    hexc(&mut b, TEAM, 1.45, 1.55, 0.14, v(0.0, 0.86, 0.0), NO_TURN); // plateau d'équipe
    hexc(&mut b, GOLD, 0.32, 0.48, 0.5, v(0.0, 1.15, 0.0), NO_TURN); // moyeu
    // La meule, qui tourne à plat : les filets et les perles montrent qu'elle tourne.
    b.animated(Transform::from_xyz(0.0, 1.46, 0.0), Motion::Spin { axis: Vec3::Y, speed: 1.4 }, |b| {
        b.cylinder(OLD_GOLD, 1.25, 0.12, Vec3::ZERO, NO_TURN);
        b.torus(GOLD, 1.27, 0.07, TAU, Vec3::ZERO, NO_TURN);
        for k in 0..3 {
            plate(b, GLOW, [2.1, 0.03, 0.06], 0.01, v(0.0, 0.07, 0.0), v(0.0, k as f32 * PI / 3.0, 0.0));
        }
        for i in 0..8 {
            let a = i as f32 * TAU / 8.0 + PI / 8.0;
            ball(b, GOLD, 0.08, v(a.cos(), 0.08, a.sin()), ONE);
        }
    });
    // Le pilier, à l'arrière (+z), et sa couronne de trois lames.
    hexc(&mut b, GOLD, 0.2, 0.3, 2.7, v(0.0, 2.25, 1.4), NO_TURN);
    hexc(&mut b, GLOW, 0.29, 0.29, 0.05, v(0.0, 1.5, 1.4), NO_TURN);
    hexc(&mut b, OLD_GOLD, 0.3, 0.3, 0.12, v(0.0, 3.0, 1.4), NO_TURN); // bague
    for k in -1..=1 {
        let k = k as f32;
        blade(&mut b, GOLD, v(k * 0.15, 3.55, 1.45), v(0.3, 0.0, -k * 0.45), 0.5);
    }
    // Le bras coudé, du haut du pilier jusqu'au-dessus de la meule.
    tube(&mut b, GOLD, &[[0.0, 3.4, 1.4], [0.0, 3.6, 0.7], [0.0, 3.0, 0.0]], 0.09);
    ball(&mut b, OLD_GOLD, 0.14, v(0.0, 3.0, 0.0), ONE); // articulation
    b.cylinder(OLD_GOLD, 0.05, 0.75, v(0.0, 2.6, 0.0), NO_TURN); // tige
    // La pierre en cours de taille, pointe en bas, juste au-dessus de la meule.
    b.animated(Transform::from_xyz(0.0, 2.05, 0.0), Motion::Spin { axis: Vec3::Y, speed: 0.5 }, |b| {
        b.pulsing(GEM_PULSE, |b| brill(b, EMERALD_STONE, 0.36, Vec3::ZERO, NO_TURN));
    });
    // Le présentoir, à l'avant (−z) : une barrette d'or vieilli et trois pierres taillées.
    plate(&mut b, OLD_GOLD, [1.5, 0.12, 0.22], 0.03, v(0.0, 1.0, -1.25), NO_TURN);
    for x in [-0.55, 0.0, 0.55] {
        b.cylinder(GOLD, 0.03, 0.3, v(x, 1.2, -1.25), NO_TURN);
        octa(&mut b, EMERALD_STONE, 0.14, v(x, 1.45, -1.25), v(1.0, 1.3, 1.0));
    }
    b
}

/// Volière : l'atelier aérien, 5,3 m. Un socle hexagonal d'émail ourlé
/// d'or, cerclé d'un filet émeraude ; dessus, l'aire d'envol laquée aux
/// couleurs du joueur autour d'un cœur d'émail vert. Six côtes d'or
/// montent en dôme ajouré, serrées à mi-hauteur par une ceinture d'or
/// vieilli, jusqu'à une couronne d'or coiffée d'une émeraude en flèche. Au
/// milieu, un mât d'or. Autour du dôme, sur deux anneaux qui tournent en
/// sens contraires, des guêpes d'or et d'émail (les mêmes que l'Essaim)
/// attendent leur envol. Vu de dessus : un anneau hexagonal d'équipe sous
/// une étoile de côtes d'or, et des guêpes qui tournent autour.
fn voliere() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    hexc(&mut b, OLD_GOLD, 2.4, 2.45, 0.25, v(0.0, 0.125, 0.0), NO_TURN); // semelle
    hexc(&mut b, ENAMEL, 2.2, 2.35, 0.5, v(0.0, 0.5, 0.0), NO_TURN); // socle
    hexc(&mut b, GLOW, 2.3, 2.3, 0.04, v(0.0, 0.45, 0.0), NO_TURN); // filet émeraude
    hexc(&mut b, GOLD, 2.26, 2.26, 0.08, v(0.0, 0.77, 0.0), NO_TURN); // ourlet
    hexc(&mut b, TEAM, 1.75, 1.85, 0.14, v(0.0, 0.86, 0.0), NO_TURN); // aire d'envol
    hexc(&mut b, ENAMEL, 1.2, 1.25, 0.06, v(0.0, 0.95, 0.0), NO_TURN); // son cœur d'émail
    hexc(&mut b, GOLD, 0.1, 0.18, 3.4, v(0.0, 2.6, 0.0), NO_TURN); // mât
    // Les six côtes du dôme, du bord de l'aire jusqu'à la couronne.
    for i in 0..6 {
        let a = i as f32 * TAU / 6.0;
        let (sin, cos) = a.sin_cos();
        let at = |r: f32, y: f32| v(cos * r, y, sin * r);
        b.tube(GOLD, &[at(1.95, 0.9), at(2.2, 2.4), at(1.6, 3.9), at(0.55, 4.55)], 0.08);
        ball(&mut b, GOLD, 0.13, at(1.95, 0.95), ONE); // pied de la côte
    }
    b.torus(OLD_GOLD, 2.18, 0.07, TAU, v(0.0, 2.4, 0.0), NO_TURN); // ceinture
    b.torus(GOLD, 0.58, 0.11, TAU, v(0.0, 4.55, 0.0), NO_TURN); // couronne
    octa(&mut b, EMERALD_STONE, 0.3, v(0.0, 5.0, 0.0), v(1.0, 1.8, 1.0)); // flèche
    // Les guêpes : quatre sur l'anneau du bas, trois sur celui du haut.
    b.animated(Transform::from_xyz(0.0, 2.9, 0.0), Motion::Spin { axis: Vec3::Y, speed: 0.9 }, |b| {
        for i in 0..4 {
            orbiting_wasp(b, i as f32 * TAU / 4.0, 2.75, (i % 2) as f32 * 0.4, 1.0, 1.6);
        }
    });
    b.animated(Transform::from_xyz(0.0, 4.4, 0.0), Motion::Spin { axis: Vec3::Y, speed: -1.3 }, |b| {
        for i in 0..3 {
            orbiting_wasp(b, i as f32 * TAU / 3.0 + 0.5, 1.5, 0.0, -1.0, 1.4);
        }
    });
    b
}

/// Fonderie : le portique où l'on coule les Titans, 5,9 m. Une large dalle
/// hexagonale d'émail sur une semelle d'or ; dessus, le lit de coulée d'or
/// vieilli bordé d'émail du joueur, creusé d'une étoile de rigoles
/// émeraude autour d'une nappe émeraude. De part et d'autre, deux pylônes hexagonaux d'or
/// bagués d'émeraude et couronnés de trois lames ; entre eux, une poutre
/// d'or vieilli au dos d'émail d'équipe, d'où pend le moule : une cloche
/// hexagonale d'or renversée, ourlée d'émail du joueur, d'où file un mince
/// jet émeraude jusqu'à la nappe. Vu de dessus : un portique d'or en H sur
/// un hexagone vert, une cloche d'or au milieu.
fn fonderie() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    hexc(&mut b, OLD_GOLD, 2.95, 3.0, 0.3, v(0.0, 0.15, 0.0), NO_TURN); // semelle
    hexc(&mut b, ENAMEL, 2.7, 2.9, 0.55, v(0.0, 0.575, 0.0), NO_TURN); // dalle
    hexc(&mut b, GOLD, 2.78, 2.78, 0.08, v(0.0, 0.88, 0.0), NO_TURN); // ourlet
    // Lit de coulée d'or vieilli, sur un liseré d'équipe qui déborde (un
    // grand plateau tout en couleur d'équipe écrasait le reste vu de dessus).
    hexc(&mut b, TEAM, 2.2, 2.25, 0.1, v(0.0, 0.95, 0.0), NO_TURN);
    hexc(&mut b, OLD_GOLD, 1.85, 1.95, 0.12, v(0.0, 1.0, 0.0), NO_TURN);
    for k in 0..3 {
        // Rigoles : trois barres en étoile.
        plate(&mut b, GLOW, [3.3, 0.04, 0.16], 0.01, v(0.0, 1.08, 0.0), v(0.0, k as f32 * PI / 3.0, 0.0));
    }
    b.cylinder(EMERALD_STONE, 0.6, 0.06, v(0.0, 1.09, 0.0), NO_TURN); // nappe
    b.mirrored(|b, s| {
        // Pylône : prisme hexagonal effilé, deux bagues émeraude, chapiteau
        // d'or vieilli et trois lames de couronne.
        hexc(b, GOLD, 0.42, 0.6, 4.3, v(s * 2.25, 3.1, 0.0), NO_TURN);
        hexc(b, GLOW, 0.59, 0.59, 0.06, v(s * 2.25, 1.8, 0.0), NO_TURN);
        hexc(b, GLOW, 0.52, 0.52, 0.06, v(s * 2.25, 3.6, 0.0), NO_TURN);
        hexc(b, OLD_GOLD, 0.62, 0.5, 0.35, v(s * 2.25, 5.4, 0.0), NO_TURN); // chapiteau
        for k in -1..=1 {
            let k = k as f32;
            blade(b, GOLD, v(s * 2.25 + k * 0.2, 5.5, 0.0), v(0.0, 0.0, -k * 0.4), 0.55);
        }
        // Contrefort d'émail, côté extérieur.
        plate(b, ENAMEL, [0.35, 1.6, 1.3], 0.08, v(s * 2.6, 1.6, 0.0), v(0.0, 0.0, -s * 0.2));
        // Tige qui tient le moule.
        b.cylinder(OLD_GOLD, 0.05, 0.4, v(s * 0.5, 4.5, 0.0), NO_TURN);
    });
    // La poutre, et son dos d'émail d'équipe (vu de dessus).
    plate(&mut b, OLD_GOLD, [4.5, 0.5, 0.8], 0.08, v(0.0, 4.9, 0.0), NO_TURN);
    plate(&mut b, TEAM, [3.6, 0.08, 0.6], 0.03, v(0.0, 5.18, 0.0), NO_TURN);
    // Le moule : une cloche hexagonale renversée, large en bas.
    hexc(&mut b, GOLD, 0.55, 1.2, 1.3, v(0.0, 3.65, 0.0), NO_TURN);
    hexc(&mut b, TEAM, 1.26, 1.26, 0.12, v(0.0, 3.0, 0.0), NO_TURN); // ourlet d'équipe
    hexc(&mut b, GLOW, 0.57, 0.57, 0.06, v(0.0, 4.3, 0.0), NO_TURN);
    // Le jet de coulée : un mince filet émeraude qui descend jusqu'à la nappe.
    b.cylinder(GLOW, 0.07, 1.9, v(0.0, 2.0, 0.0), NO_TURN);
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

    /// Même garde-fou pour les bâtiments (et la gemme d'ancrage).
    #[test]
    fn buildings_stay_light() {
        let anchor: usize = anchor_gem().iter().map(|p| p.shape.mesh().count_vertices()).sum();
        println!("Gemme d'ancrage : {anchor} sommets");
        assert!(anchor < 2_000);
        for building in building_types() {
            let vertices: usize = building.parts.iter().map(|p| p.shape.mesh().count_vertices()).sum();
            println!("{} : {} sommets, {} pièces", building.name, vertices, building.parts.len());
            assert!(vertices < 14_000, "{} est trop lourd : {vertices} sommets", building.name);
        }
    }

    /// Le palier 1 a les mêmes rôles que ceux des autres factions, et le
    /// champ de résonance est émis par le cœur et le relais.
    #[test]
    fn tier_one_roles() {
        use building::*;
        let types = building_types();
        assert!(types[ECRIN].dropoff && types[ECRIN].supply > 0);
        assert_eq!(types[ECRIN].production[0].unit_type, kind::COLLECTEUR);
        assert!(types[RESONATEUR].supply > 0);
        assert_eq!(types[CREUSET].placement, PlacementRule::OnResource(ResourceKind::Rare));
        assert_eq!(types[ORFEVRERIE].production[0].unit_type, kind::AUTOMATE);
        assert_eq!(types[AIGUILLON].requires, vec![ORFEVRERIE]);
        // Qui émet le champ, qui en a besoin.
        let emitters: Vec<usize> = (0..types.len()).filter(|&i| types[i].field_radius > 0.0).collect();
        assert_eq!(emitters, vec![ECRIN, RESONATEUR]);
        let needy: Vec<usize> = (0..types.len()).filter(|&i| types[i].needs_field).collect();
        assert_eq!(needy, vec![ORFEVRERIE, AIGUILLON, LAPIDAIRE, VOLIERE, FONDERIE]);
        assert!(CONSTRUCTION.builds_alone() && !CONSTRUCTION.consumes_worker());
        // Raccourcis distincts après « B », et aucun ne vaut « B » lui-même.
        let mut keys: Vec<char> = types.iter().map(|t| t.hotkey).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), types.len());
        assert!(!keys.contains(&'b'));
        // Une ligne de coûts par type d'unité.
        assert_eq!(economy_rules(crate::gisements::resource_infos()).units.len(), unit_types().len());
    }

    /// Le palier 2 : chaque unité a un bâtiment qui la produit, chaque
    /// prérequis est un vrai bâtiment du catalogue, et on peut tout
    /// débloquer en partant de l'Écrin (pas de prérequis en boucle).
    #[test]
    fn every_unit_can_be_produced() {
        use building::*;
        let types = building_types();
        for unit in 0..unit_types().len() {
            assert!(types.iter().any(|t| t.production.iter().any(|o| o.unit_type == unit)), "unité {unit} sans bâtiment");
        }
        // Construit tout ce qui devient possible, jusqu'à ne plus rien gagner.
        let mut completed = vec![false; types.len()];
        completed[ECRIN] = true;
        loop {
            let next = (0..types.len()).find(|&t| !completed[t] && types[t].requires.iter().all(|&r| completed[r]));
            let Some(next) = next else { break };
            completed[next] = true;
        }
        assert!(completed.iter().all(|&done| done), "bâtiments inatteignables : {completed:?}");
        for option in types.iter().flat_map(|t| &t.production) {
            assert!(option.requires.iter().all(|r| r.met(&completed)), "option toujours grisée : {option:?}");
        }
        assert_eq!(types[ORFEVRERIE].production[1].requires, vec![Requirement::Building(LAPIDAIRE)]);
        assert_eq!(types[FONDERIE].production[0].unit_type, kind::TITAN);
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
