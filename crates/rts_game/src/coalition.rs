//! La Coalition (nom provisoire) : palette, ambiance, unités et bâtiments.
//!
//! Direction artistique : « blindage en couches, peinture écaillée, moteurs
//! brûlants ; une armée industrielle qui tient le terrain ». Palette :
//! métal sombre, acier, sable, orange signal (couleur d'équipe du premier
//! joueur), visières cyan, bandes de danger à chevrons.
//!
//! Les unités sont décrites pièce par pièce, en mètres, pieds au sol (y = 0)
//! et l'avant vers −z. Chaque silhouette doit se reconnaître d'un coup d'œil :
//! le fantassin standard, le tireur grand et fin au long canon, le lancier
//! et son tube sur l'épaule, le faucon penché en avant sur ses réacteurs,
//! l'ouvrier trapu, le char large et bas.
//!
//! Le fantassin, le Faucon, le Lancier et le Bastion reprennent les cotes de
//! la page de direction artistique (`buildCoalition`). Pour recopier ses
//! nombres tels quels, ces pièces sont écrites dans le **repère de la page**
//! (avant vers +z, fantassin de 2,9 m), puis le groupe [`page`] les tourne
//! d'un demi-tour et les ramène à notre échelle.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;
use rts_engine::buildings::{BuildingType, Cost, PlacementRule};
use rts_engine::economy::ResourceKind;
use rts_engine::graphics::{Ambiance, EnvironmentSettings, FogSettings, ParticleSettings, SceneLight};
use rts_engine::units::{
    HoverMotion, MaterialSlot, ModelBuilder, Motion, Palette, PartShape, Pulse, SurfaceDetail, SurfaceStyle, UnitType,
};

// `use Enum::*` : on peut écrire `Paint` au lieu de `MaterialSlot::Paint`.
use MaterialSlot::*;

/// Couleurs de la page de direction artistique.
pub const DARK_METAL: Color = Color::srgb(0x17 as f32 / 255.0, 0x1C as f32 / 255.0, 0x24 as f32 / 255.0);
pub const STEEL: Color = Color::srgb(0x56 as f32 / 255.0, 0x66 as f32 / 255.0, 0x7A as f32 / 255.0);
pub const SAND: Color = Color::srgb(0xCD as f32 / 255.0, 0xBB as f32 / 255.0, 0x90 as f32 / 255.0);
pub const SIGNAL_ORANGE: Color = Color::srgb(0xE8 as f32 / 255.0, 0x73 as f32 / 255.0, 0x2C as f32 / 255.0);
pub const VISOR: Color = Color::srgb(0x4F as f32 / 255.0, 0xD1 as f32 / 255.0, 0xE0 as f32 / 255.0);
const ENGINE_GLOW: Color = Color::srgb(1.0, 0x8A as f32 / 255.0, 0x2A as f32 / 255.0);

/// Matériaux de la Coalition. `team_color` est la couleur du joueur (elle
/// colore aussi les bandes de danger, rayées de noir).
pub fn palette(team_color: Color) -> Palette {
    Palette {
        paint: SurfaceStyle { detail_size: 1.1, ..SurfaceStyle::new(SAND, 0.35, 0.55, SurfaceDetail::WornPanels) },
        dark: SurfaceStyle { detail_size: 1.4, ..SurfaceStyle::new(DARK_METAL, 0.9, 0.5, SurfaceDetail::WornPanels) },
        steel: SurfaceStyle { detail_size: 0.9, ..SurfaceStyle::new(STEEL, 0.85, 0.42, SurfaceDetail::WornPanels) },
        bare: SurfaceStyle::new(Color::srgb(0.52, 0.57, 0.63), 1.0, 0.3, SurfaceDetail::Smooth),
        rubber: SurfaceStyle::new(Color::srgb(0.047, 0.055, 0.067), 0.2, 0.9, SurfaceDetail::Smooth),
        team: SurfaceStyle::new(team_color, 0.4, 0.5, SurfaceDetail::Panels),
        glow: SurfaceStyle::glowing(VISOR, 4.0),
        engine: SurfaceStyle::glowing(ENGINE_GLOW, 4.0),
        selection: LinearRgba::rgb(0.3, 2.2, 0.5),
    }
}

/// Ambiance des cartes de la Coalition : nuit bleutée, lumière principale
/// chaude, contre-jours orange et cyan, braises qui flottent.
pub fn ambiance() -> Ambiance {
    let background = Color::srgb(0.039, 0.063, 0.094); // #0A1018
    Ambiance {
        background,
        exposure: 9.7,
        // La caméra regarde vers −z : la lumière principale vient d'en haut,
        // à gauche et de derrière la caméra, pour éclairer l'avant des unités.
        sun: SceneLight { color: Color::srgb(1.0, 0.94, 0.86), illuminance: 4500.0, from: Vec3::new(-0.5, 1.0, 0.55) },
        fill_lights: vec![
            // Contre-jours : de l'autre côté des unités, ils soulignent leurs contours.
            SceneLight { color: Color::srgb(1.0, 0.54, 0.24), illuminance: 3000.0, from: Vec3::new(-0.8, 0.45, -0.7) },
            SceneLight { color: VISOR, illuminance: 1800.0, from: Vec3::new(0.9, 0.35, -0.5) },
        ],
        environment: EnvironmentSettings {
            sky: Color::srgb(0.55, 0.62, 0.8),
            ground: Color::srgb(0.25, 0.17, 0.1),
            background: Color::srgb(0.03, 0.035, 0.05),
            light_reflections: 1.0,
            intensity: 500.0,
        },
        fog: Some(FogSettings { start: 1.0, end: 2.6 }),
        bloom: 0.18,
        particles: Some(ParticleSettings { color: Color::srgb(1.0, 0.63, 0.31), count: 120, glow: 3.0 }),
    }
}

/// Indices des types dans [`unit_types`], pour s'y référer par leur nom.
pub mod kind {
    pub const TECHNICIEN: usize = 0;
    pub const SENTINELLE: usize = 1;
    pub const LONGUE_VUE: usize = 2;
    pub const LANCIER: usize = 3;
    pub const BASTION: usize = 4;
    pub const FAUCON: usize = 5;
}

/// Les six unités de la Coalition (voir le tableau de GAME_DESIGN.md).
pub fn unit_types() -> Vec<UnitType> {
    vec![
        unit("Technicien", technicien(), 0.6, 1.0, 3.5),
        unit("Sentinelle", sentinelle(), 0.5, 1.0, 4.0),
        unit("Longue-Vue", longue_vue(), 0.45, 1.0, 3.2),
        unit("Lancier", lancier(), 0.55, 1.5, 3.2),
        unit("Bastion", bastion(), 1.8, 12.0, 2.6),
        UnitType {
            hover: Some(HoverMotion { amplitude: 0.12, frequency: 0.5 }),
            ..unit("Faucon", faucon(), 0.5, 1.0, 6.5)
        },
    ]
}

/// `mass` : masse relative pour les collisions, 1 pour un fantassin.
fn unit(name: &str, model: ModelBuilder, footprint_radius: f32, mass: f32, speed: f32) -> UnitType {
    UnitType { name: name.into(), parts: model.build(), footprint_radius, mass, speed, hover: None }
}

// ---------------------------------------------------------------------------
// Animations (purement visuelles, voir `rts_engine::graphics::animation`).
// ---------------------------------------------------------------------------

// Chaque matériau qui pulse a un coût (voir `animate_pulses` dans le moteur) :
// on réserve les pulsations aux réacteurs, ogives et gyrophares, pas aux
// visières que portent toutes les unités. Les flammes, elles, scintillent
// par leur taille (`Motion::Flicker`), ce qui ne coûte presque rien.

/// Réacteurs, échappements, ogives : vibration (15 mises à jour par seconde au plus).
const ENGINE_PULSE: Pulse = Pulse { min: 0.6, max: 1.35, frequency: 2.2 };
/// Gyrophare du Technicien : clignote franchement.
const BEACON_PULSE: Pulse = Pulse { min: 0.1, max: 1.7, frequency: 1.2 };

// ---------------------------------------------------------------------------
// Raccourcis d'écriture.
// ---------------------------------------------------------------------------

/// Raccourci pour écrire les positions.
fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn v2(z: f32, y: f32) -> Vec2 {
    Vec2::new(z, y)
}

/// Placement d'un sous-groupe : position, puis rotation (x, y, z) en radians.
fn at_rot(at: Vec3, rotation: Vec3) -> Transform {
    Transform::from_translation(at).with_rotation(Quat::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z))
}

/// Repère de la page de direction artistique, ramené au nôtre : demi-tour
/// (l'avant passe de +z à −z) et mise à l'échelle (`scale` par axe).
fn page(scale: Vec3) -> Transform {
    Transform::from_rotation(Quat::from_rotation_y(PI)).with_scale(scale)
}

/// Plaque de la page : `[largeur, hauteur, profondeur, biseau]`, centrée en `at`.
/// Un biseau nul donne un pavé net (les `box` de la page).
fn pl(b: &mut ModelBuilder, slot: MaterialSlot, size: [f32; 4], at: Vec3) {
    b.plate_beveled(slot, v(size[0], size[1], size[2]), size[3], at, Vec3::ZERO);
}

/// Plaque tournée de `rotation` (x, y, z).
fn plr(b: &mut ModelBuilder, slot: MaterialSlot, size: [f32; 4], at: Vec3, rotation: Vec3) {
    b.plate_beveled(slot, v(size[0], size[1], size[2]), size[3], at, rotation);
}

/// Cylindre de la page : `[rayon du haut, rayon du bas, hauteur]` (le `cyl` de three.js).
fn cy(b: &mut ModelBuilder, slot: MaterialSlot, size: [f32; 3], at: Vec3, rotation: Vec3) {
    let [top, bottom, height] = size;
    if (top - bottom).abs() < 1e-4 {
        b.cylinder(slot, top, height, at, rotation);
    } else {
        b.cone(slot, bottom, top, height, at, rotation);
    }
}

/// Rotation qui couche un cylindre le long de +z (canons, tubes).
const ALONG_Z: Vec3 = Vec3::new(FRAC_PI_2, 0.0, 0.0);

// ---------------------------------------------------------------------------
// Les unités.
// ---------------------------------------------------------------------------

/// Le fantassin de la page (`trooper`), dans le repère de la page : grosses
/// bottes, genouillères et bandes d'épaulières aux couleurs de l'équipe,
/// épaulières massives (visibles de dessus), casque à visière, sac à dos et
/// ses deux tuyères. 34 pièces.
fn trooper(b: &mut ModelBuilder) {
    b.mirrored(|b, s| {
        pl(b, Dark, [0.42, 0.3, 0.66, 0.05], v(s * 0.3, 0.15, 0.08)); // botte
        pl(b, Steel, [0.4, 0.2, 0.26, 0.04], v(s * 0.3, 0.13, 0.4)); // coque de pointe
        pl(b, Paint, [0.38, 0.62, 0.44, 0.05], v(s * 0.3, 0.62, 0.0)); // tibia
        pl(b, Team, [0.3, 0.26, 0.18, 0.04], v(s * 0.3, 0.98, 0.22)); // genouillère
        pl(b, Glow, [0.04, 0.4, 0.02, 0.0], v(s * 0.3, 0.6, 0.225)); // liseré du tibia
        pl(b, Dark, [0.34, 0.5, 0.4, 0.04], v(s * 0.26, 1.2, 0.0)); // cuisse
        // Épaulière : grosse coque peinte, bordée d'une bande d'équipe.
        b.group(at_rot(v(s * 0.84, 2.14, 0.0), v(0.0, 0.0, -s * 0.22)), |b| {
            pl(b, Paint, [0.6, 0.44, 0.76, 0.1], Vec3::ZERO);
            pl(b, Team, [0.62, 0.1, 0.78, 0.03], v(0.0, -0.2, 0.0));
            pl(b, Steel, [0.3, 0.05, 0.4, 0.01], v(0.0, 0.235, 0.0)); // plaque de dessus
        });
        pl(b, Dark, [0.28, 0.5, 0.32, 0.04], v(s * 0.82, 1.7, 0.0)); // bras
        // Avant-bras levés vers l'avant (le droit un peu plus que le gauche).
        let forearm = v(if s > 0.0 { -1.0 } else { -0.8 }, 0.0, 0.0);
        plr(b, Steel, [0.34, 0.52, 0.42, 0.05], v(s * 0.84, 1.4, 0.3), forearm);
        plr(b, Team, [0.36, 0.14, 0.44, 0.03], v(s * 0.84, 1.52, 0.14), forearm);
        cy(b, Bare, [0.07, 0.07, 0.42], v(s * 0.22, 2.42, -0.52), Vec3::ZERO); // tuyère du sac
    });
    pl(b, Dark, [0.8, 0.3, 0.46, 0.05], v(0.0, 1.44, 0.0)); // bassin
    pl(b, Dark, [0.62, 0.26, 0.52, 0.05], v(0.0, 1.6, 0.0)); // ventre
    pl(b, Steel, [1.0, 0.7, 0.64, 0.08], v(0.0, 1.95, 0.0)); // torse
    pl(b, Paint, [0.72, 0.42, 0.14, 0.05], v(0.0, 2.0, 0.36)); // plastron
    pl(b, Glow, [0.14, 0.06, 0.02, 0.0], v(0.2, 2.06, 0.44)); // voyant
    cy(b, Dark, [0.3, 0.36, 0.14], v(0.0, 2.34, 0.0), Vec3::ZERO); // cou
    pl(b, Steel, [0.46, 0.46, 0.52, 0.09], v(0.0, 2.56, 0.02)); // casque
    pl(b, Glow, [0.36, 0.13, 0.05, 0.0], v(0.0, 2.57, 0.285)); // visière
    pl(b, Paint, [0.48, 0.09, 0.24, 0.03], v(0.0, 2.7, 0.2)); // visière haute
    pl(b, Team, [0.1, 0.2, 0.4, 0.03], v(0.0, 2.82, 0.0)); // crête
    pl(b, Dark, [0.72, 0.72, 0.34, 0.06], v(0.0, 1.98, -0.48)); // sac à dos
    pl(b, Engine, [0.4, 0.04, 0.02, 0.0], v(0.0, 1.8, -0.66)); // évent du sac
}

/// Sentinelle : le fantassin de la page, fusil tenu à la hanche.
fn sentinelle() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(Vec3::splat(0.6)), |b| {
        trooper(b);
        b.group(at_rot(v(0.5, 1.3, 0.75), v(0.0, 0.12, 0.0)), |b| {
            pl(b, Dark, [0.16, 0.24, 1.0, 0.03], Vec3::ZERO); // carcasse
            pl(b, Steel, [0.08, 0.06, 0.8, 0.015], v(0.0, 0.15, 0.05)); // rail
            cy(b, Bare, [0.045, 0.045, 0.6], v(0.0, 0.03, 0.78), ALONG_Z); // canon
            cy(b, Dark, [0.07, 0.07, 0.14], v(0.0, 0.03, 1.1), ALONG_Z); // bouche
            pl(b, Steel, [0.1, 0.3, 0.16, 0.02], v(0.0, -0.24, 0.1)); // chargeur
            pl(b, Glow, [0.02, 0.05, 0.5, 0.0], v(0.085, 0.02, 0.1)); // viseur
            pl(b, Dark, [0.12, 0.2, 0.3, 0.03], v(0.0, -0.03, -0.62)); // crosse
        });
    });
    b
}

/// Longue-Vue (absent de la page) : le fantassin étiré, plus grand et plus
/// fin, un fusil deux fois plus long que celui de la Sentinelle et un petit
/// radar qui tourne sur le sac : on le repère à son canon et à son mouvement.
fn longue_vue() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(v(0.52, 0.68, 0.54)), |b| {
        trooper(b);
        b.group(at_rot(v(0.45, 1.55, 0.8), v(0.0, 0.06, 0.0)), |b| {
            pl(b, Dark, [0.14, 0.2, 1.2, 0.03], Vec3::ZERO); // carcasse
            cy(b, Bare, [0.035, 0.035, 1.8], v(0.0, 0.03, 1.5), ALONG_Z); // long canon
            pl(b, Dark, [0.1, 0.1, 0.22, 0.02], v(0.0, 0.03, 2.45)); // frein de bouche
            cy(b, Steel, [0.065, 0.065, 0.5], v(0.0, 0.2, 0.1), ALONG_Z); // lunette
            cy(b, Glow, [0.05, 0.05, 0.03], v(0.0, 0.2, 0.37), ALONG_Z); // optique
            pl(b, Paint, [0.12, 0.24, 0.4, 0.03], v(0.0, -0.05, -0.75)); // crosse
            pl(b, Team, [0.15, 0.06, 0.3, 0.02], v(0.0, 0.11, -0.2)); // bande d'équipe
        });
        // Radar de repérage sur le sac : une barre qui tourne sur un mât.
        cy(b, Dark, [0.025, 0.025, 0.55], v(-0.25, 2.55, -0.5), Vec3::ZERO); // mât
        b.animated(Transform::from_xyz(-0.25, 2.85, -0.5), Motion::Spin { axis: Vec3::Y, speed: 2.4 }, |b| {
            pl(b, Steel, [0.55, 0.06, 0.12, 0.02], Vec3::ZERO);
            pl(b, Glow, [0.1, 0.05, 0.02, 0.0], v(0.2, 0.0, 0.07));
        });
    });
    b
}

/// Lancier : le fantassin de la page un peu plus massif, lance-missiles sur
/// l'épaule droite (silhouette asymétrique). Le corps du tube est une forme
/// de révolution : tube fin et évasement arrière d'une seule pièce.
fn lancier() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(Vec3::splat(0.66)), |b| {
        trooper(b);
        b.group(Transform::from_xyz(0.66, 2.62, 0.0), |b| {
            // Profil (rayon, position le long du tube) de l'arrière vers l'avant.
            let body = [v2(0.0, -1.09), v2(0.29, -1.09), v2(0.2, -0.83), v2(0.2, 0.85), v2(0.0, 0.85)];
            b.lathe(Steel, &body, 18, Vec3::ZERO, ALONG_Z);
            cy(b, Paint, [0.25, 0.25, 0.32], v(0.0, 0.0, 0.75), ALONG_Z); // manchon avant
            cy(b, Dark, [0.22, 0.3, 0.12], v(0.0, 0.0, -0.9), ALONG_Z); // lèvre arrière
            cy(b, Team, [0.21, 0.21, 0.1], v(0.0, 0.0, 0.3), ALONG_Z); // bague d'équipe
            b.pulsing(ENGINE_PULSE, |b| {
                b.cone(Engine, 0.15, 0.0, 0.32, v(0.0, 0.0, 0.98), ALONG_Z); // ogive brûlante
            });
            pl(b, Dark, [0.1, 0.16, 0.3, 0.02], v(-0.24, 0.1, 0.3)); // viseur
            pl(b, Glow, [0.02, 0.08, 0.1, 0.0], v(-0.3, 0.1, 0.3));
        });
    });
    b
}

/// Faucon : le fantassin léger de la page, penché en avant, deux gros
/// réacteurs dorsaux (formes de révolution) aux flammes animées, ailerons
/// en flèche. Il vole à un mètre du sol.
fn faucon() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let flight = Transform::from_xyz(0.0, 0.9, 0.0).with_rotation(Quat::from_rotation_x(-0.3));
    b.group(flight * page(Vec3::splat(0.56)), |b| {
        trooper(b);
        b.mirrored(|b, s| {
            // Corps du réacteur : profil (rayon, hauteur) du bas vers le haut.
            let pod = [v2(0.0, -0.42), v2(0.14, -0.42), v2(0.21, -0.2), v2(0.21, 0.3), v2(0.12, 0.46), v2(0.0, 0.46)];
            b.lathe(Paint, &pod, 18, v(s * 0.34, 2.0, -0.64), Vec3::ZERO);
            cy(b, Team, [0.215, 0.215, 0.08], v(s * 0.34, 2.1, -0.64), Vec3::ZERO); // bague d'équipe
            b.pulsing(ENGINE_PULSE, |b| {
                cy(b, Engine, [0.13, 0.13, 0.03], v(s * 0.34, 1.575, -0.64), Vec3::ZERO); // tuyère
            });
            // Aileron : profil dessiné vers l'extérieur (x de la page), couché
            // en arrière pour qu'il se voie de dessus.
            let fin = [v2(0.0, 0.0), v2(0.95 * s, 0.18), v2(0.95 * s, 0.42), v2(0.0, 0.52)];
            b.beveled_prism(Paint, &fin, 0.05, 0.012, v(s * 0.52, 1.85, -0.64), v(-0.9, FRAC_PI_2, 0.0));
            pl(b, Team, [0.05, 0.08, 0.06, 0.0], v(s * 1.45, 1.85 + 0.3 * 0.62, -0.64 - 0.3 * 0.78)); // feu de bout d'aile
        });
        // Les deux flammes : un seul groupe animé, dont le pivot est à la
        // hauteur des tuyères. Elles s'allongent et se raccourcissent vers le
        // bas, rabattues vers l'arrière pour dépasser du corps vues de dessus.
        let flicker = Motion::Flicker { axis: Vec3::Y, amplitude: 0.18, frequency: 5.0 };
        let nozzles = Transform::from_xyz(0.0, 1.56, -0.64).with_rotation(Quat::from_rotation_x(0.45));
        b.animated(nozzles, flicker, |b| {
            b.mirrored(|b, s| {
                // Flamme extérieure, puis cœur plus fin : leurs lumières s'additionnent.
                b.cone(Flame, 0.13, 0.0, 1.1, v(s * 0.34, -0.555, 0.0), v(PI, 0.0, 0.0));
                b.cone(Flame, 0.07, 0.0, 0.6, v(s * 0.34, -0.305, 0.0), v(PI, 0.0, 0.0));
            });
        });
    });
    b
}

/// Technicien (absent de la page) : exosquelette de chantier trapu, outil
/// lumineux au bras droit, bandes de danger, gyrophare et durites hydrauliques.
fn technicien() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.mirrored(|b, s| {
        b.plate(Dark, v(0.28, 0.14, 0.42), v(s * 0.27, 0.07, -0.04)) // pied
            .plate(Steel, v(0.22, 0.38, 0.26), v(s * 0.27, 0.33, 0.02)) // jambe
            .cylinder(Bare, 0.04, 0.35, v(s * 0.27, 0.36, 0.17), Vec3::ZERO) // vérin
            .plate_rotated(Team, v(0.2, 0.18, 0.3), v(s * 0.36, 0.62, 0.0), v(0.0, 0.0, -s * 0.2)) // garde de hanche
            .plate(Dark, v(0.18, 0.34, 0.2), v(s * 0.52, 0.84, -0.02)) // bras
            .plate(Paint, v(0.3, 0.22, 0.36), v(s * 0.5, 1.06, 0.0)) // épaule
            .plate_beveled(Hazard, v(0.31, 0.03, 0.37), 0.01, v(s * 0.5, 1.175, 0.0), Vec3::ZERO) // chevrons d'épaule
            // Durite hydraulique du groupe moteur vers l'épaule.
            .tube(Dark, &[v(s * 0.2, 1.05, 0.28), v(s * 0.36, 1.2, 0.2), v(s * 0.5, 1.12, 0.08)], 0.025);
    });
    b.plate(Dark, v(0.5, 0.16, 0.34), v(0.0, 0.56, 0.0)) // bassin
        .plate(Paint, v(0.72, 0.46, 0.52), v(0.0, 0.86, 0.0)) // châssis
        .plate(Steel, v(0.5, 0.2, 0.1), v(0.0, 0.84, -0.29)) // plaque ventrale
        .plate_beveled(Hazard, v(0.52, 0.08, 0.02), 0.005, v(0.0, 0.7, -0.27), Vec3::ZERO) // chevrons du ventre
        .plate(Steel, v(0.3, 0.22, 0.26), v(0.0, 1.16, -0.12)) // tête
        .plate(Glow, v(0.24, 0.06, 0.02), v(0.0, 1.17, -0.255)) // visière
        .plate(Dark, v(0.56, 0.4, 0.28), v(0.0, 0.95, 0.38)) // groupe moteur
        .plate_beveled(Hazard, v(0.5, 0.12, 0.02), 0.005, v(0.0, 0.84, 0.525), Vec3::ZERO) // chevrons arrière
        .plate(Team, v(0.5, 0.04, 0.3), v(0.0, 1.16, 0.38)) // capot d'équipe
        .cylinder(Dark, 0.02, 0.5, v(-0.2, 1.4, 0.42), Vec3::ZERO) // mât
        .lathe(Bare, &[v2(0.0, -0.15), v2(0.05, -0.15), v2(0.05, 0.08), v2(0.035, 0.15), v2(0.0, 0.15)], 12, v(0.52, 0.66, -0.2), v(-FRAC_PI_2, 0.0, 0.0)) // outil
        .plate(Steel, v(0.12, 0.1, 0.22), v(-0.52, 0.66, -0.12)); // pince
    b.pulsing(ENGINE_PULSE, |b| {
        b.cone(Engine, 0.035, 0.0, 0.12, v(0.52, 0.66, -0.41), v(-FRAC_PI_2, 0.0, 0.0)); // pointe lumineuse
    });
    b.pulsing(BEACON_PULSE, |b| {
        b.sphere(Engine, 0.05, v(-0.2, 1.68, 0.42)); // gyrophare
    });
    b
}

/// Bastion : le char de la page. Chenilles à galets et maillons, jupes
/// latérales et glacis rayés de chevrons de danger, caisse et tourelle en
/// prismes biseautés. La tourelle, avec son long canon, balaie lentement
/// l'horizon (groupe animé).
fn bastion() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.group(page(Vec3::splat(0.68)), |b| {
        b.mirrored(|b, s| {
            // Chenille vue de côté (z, y) : avant (+z dans la page) relevé.
            let track = [v2(-2.6, 0.3), v2(-2.25, 0.0), v2(2.2, 0.0), v2(2.75, 0.4), v2(2.55, 0.88), v2(-2.45, 0.88)];
            b.beveled_prism(Rubber, &track, 0.8, 0.04, v(s * 1.5, 0.0, 0.0), Vec3::ZERO);
            for wheel in 0..7 {
                let z = -2.1 + wheel as f32 * 0.7;
                cy(b, Bare, [0.3, 0.3, 0.12], v(s * 1.93, 0.38, z), v(0.0, 0.0, FRAC_PI_2)); // galet
            }
            for link in 0..14 {
                pl(b, Dark, [0.84, 0.05, 0.2, 0.0], v(s * 1.5, 0.94, -2.3 + link as f32 * 0.36)); // maillon
            }
            pl(b, Steel, [0.5, 0.12, 5.0, 0.04], v(s * 1.5, 1.22, 0.05)); // garde-boue
            pl(b, Paint, [0.24, 0.34, 4.7, 0.05], v(s * 1.92, 1.02, 0.05)); // jupe latérale
            pl(b, Hazard, [0.26, 0.36, 0.9, 0.03], v(s * 1.93, 1.02, 1.6)); // chevrons de la jupe
            pl(b, Team, [0.06, 0.12, 2.0, 0.0], v(s * 1.225, 1.25, 0.0)); // liseré d'équipe
            pl(b, Dark, [0.4, 0.3, 0.9, 0.04], v(s * 1.5, 1.44, -1.5)); // coffre
            pl(b, Glow, [0.3, 0.16, 0.08, 0.02], v(s * 0.85, 0.98, 2.64)); // phare
            cy(b, Dark, [0.1, 0.12, 0.5], v(s * 0.8, 1.45, -2.55), ALONG_Z); // échappement
            b.pulsing(ENGINE_PULSE, |b| cy(b, Engine, [0.085, 0.085, 0.02], v(s * 0.8, 1.45, -2.81), ALONG_Z));
        });
        // Caisse vue de côté : glacis incliné à l'avant.
        let hull = [v2(-2.55, 0.5), v2(2.3, 0.5), v2(2.78, 0.95), v2(1.6, 1.5), v2(-2.3, 1.5), v2(-2.62, 1.1)];
        b.beveled_prism(Steel, &hull, 2.3, 0.06, Vec3::ZERO, Vec3::ZERO);
        plr(b, Paint, [1.9, 0.08, 1.2, 0.03], v(0.0, 1.3, 2.2), v(0.446, 0.0, 0.0)); // plaque du glacis
        plr(b, Hazard, [0.5, 0.09, 1.22, 0.02], v(0.0, 1.305, 2.2), v(0.446, 0.0, 0.0)); // chevrons du glacis
        pl(b, Dark, [1.9, 0.14, 1.4, 0.04], v(0.0, 1.6, -1.5)); // plage arrière
        for bar in 0..5 {
            pl(b, Bare, [1.5, 0.03, 0.12, 0.0], v(0.0, 1.685, -2.0 + bar as f32 * 0.25)); // grille moteur
        }

        // La tourelle balaie lentement de ±26° (une période d'environ 16 s).
        let sweep = Motion::Swing { axis: Vec3::Y, amplitude: 0.45, frequency: 0.06 };
        b.animated(Transform::from_xyz(0.0, 1.56, -0.15), sweep, |b| {
            cy(b, Dark, [1.05, 1.1, 0.16], v(0.0, 0.08, 0.0), Vec3::ZERO); // couronne
            let turret = [v2(-1.55, 0.14), v2(1.05, 0.14), v2(1.55, 0.5), v2(0.95, 1.0), v2(-1.3, 1.0), v2(-1.65, 0.62)];
            b.beveled_prism(Steel, &turret, 1.9, 0.06, Vec3::ZERO, Vec3::ZERO);
            b.mirrored(|b, s| {
                plr(b, Paint, [0.16, 0.55, 1.7, 0.05], v(s * 1.08, 0.58, -0.1), v(0.0, s * 0.06, 0.0)); // joue
                plr(b, Hazard, [0.17, 0.2, 0.6, 0.02], v(s * 1.09, 0.4, -0.9), v(0.0, s * 0.06, 0.0)); // chevrons
                for q in 0..3 {
                    cy(b, Dark, [0.07, 0.07, 0.3], v(s * 1.24, 0.86, 0.2 + q as f32 * 0.2), v(0.9, 0.0, 0.0)); // fumigène
                }
                cy(b, Dark, [0.015, 0.02, 1.9], v(s * 0.8, 2.0, -1.25), Vec3::ZERO); // antenne
            });
            pl(b, Team, [1.7, 0.04, 0.28, 0.01], v(0.0, 1.02, 0.55)); // bande d'équipe sur le toit
            pl(b, Dark, [0.8, 0.6, 0.45, 0.08], v(0.0, 0.55, 1.5)); // masque du canon
            cy(b, Bare, [0.14, 0.17, 2.9], v(0.0, 0.55, 3.1), ALONG_Z); // canon
            cy(b, Paint, [0.21, 0.21, 1.0], v(0.0, 0.55, 2.4), ALONG_Z); // manchon
            cy(b, Team, [0.19, 0.19, 0.12], v(0.0, 0.55, 3.7), ALONG_Z); // bague d'équipe
            pl(b, Dark, [0.4, 0.32, 0.55, 0.05], v(0.0, 0.55, 4.6)); // frein de bouche
            cy(b, Dark, [0.32, 0.36, 0.22], v(-0.5, 1.17, -0.45), Vec3::ZERO); // tourelleau
            cy(b, Steel, [0.26, 0.26, 0.05], v(-0.5, 1.3, -0.45), Vec3::ZERO); // trappe
            cy(b, Dark, [0.035, 0.035, 0.9], v(-0.5, 1.44, 0.0), ALONG_Z); // mitrailleuse
            pl(b, Dark, [0.14, 0.16, 0.3, 0.02], v(-0.5, 1.42, -0.35)); // boîtier
            pl(b, Dark, [0.36, 0.2, 0.2, 0.03], v(0.55, 1.16, 0.3)); // viseur
            pl(b, Glow, [0.28, 0.08, 0.02, 0.0], v(0.55, 1.16, 0.41));
            pl(b, Dark, [1.5, 0.45, 0.45, 0.05], v(0.0, 0.6, -1.85)); // coffre arrière
            pl(b, Paint, [1.2, 0.06, 1.4, 0.02], v(0.25, 1.09, -0.35)); // plaque de toit
        });
    });
    b
}

// ---------------------------------------------------------------------------
// Les bâtiments (palier 1 de l'arbre de la Coalition, voir GAME_DESIGN.md).
// ---------------------------------------------------------------------------

/// Indices des bâtiments dans [`building_types`].
pub mod building {
    pub const QUARTIER_GENERAL: usize = 0;
    pub const HABITAT: usize = 1;
    pub const EXTRACTEUR: usize = 2;
    pub const CASERNE: usize = 3;
    pub const TOURELLE: usize = 4;
}

/// Les bâtiments du palier 1. Les tailles se comparent aux unités : un
/// fantassin fait 1 m de large, le Bastion 3,6 m ; le Quartier général
/// (7 × 7 m, plus de 6 m de haut avec son radar) domine nettement la base.
///
/// Chiffres provisoires (PV, temps en secondes, coût en ressource principale
/// et rare), inspirés des ratios de SC2, à ajuster en test. L'Extracteur se
/// pose exactement sur une source de ressource rare (4 × 4 m, comme elle).
pub fn building_types() -> Vec<BuildingType> {
    use building::*;
    vec![
        building_type("Quartier général", (7, 7), 1500.0, 70.0, (400, 0), &[], 'q', quartier_general()),
        building_type("Module d'habitat", (3, 3), 400.0, 20.0, (100, 0), &[], 'h', habitat()),
        BuildingType {
            placement: PlacementRule::OnResource(ResourceKind::Rare),
            ..building_type("Extracteur", (4, 4), 500.0, 22.0, (75, 0), &[], 'e', extracteur())
        },
        building_type("Caserne", (5, 6), 1000.0, 45.0, (150, 0), &[QUARTIER_GENERAL], 'c', caserne()),
        building_type("Tourelle", (2, 2), 250.0, 18.0, (100, 0), &[CASERNE], 't', tourelle()),
    ]
}

/// `cost` : (ressource principale, ressource rare).
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
        cost: Cost { primary: cost.0, rare: cost.1 },
        requires: requires.to_vec(),
        hotkey,
        parts: model.build(),
        ..Default::default()
    }
}

/// Gyrophares et balises des bâtiments : clignotement lent.
const BEACON_SLOW: Pulse = Pulse { min: 0.15, max: 1.6, frequency: 0.7 };

/// Poutre cylindrique de `from` à `to` (pylônes, treillis).
fn strut(b: &mut ModelBuilder, slot: MaterialSlot, from: Vec3, to: Vec3, radius: f32) {
    let axis = to - from;
    let transform = Transform::from_translation((from + to) / 2.0).with_rotation(Quat::from_rotation_arc(Vec3::Y, axis.normalize()));
    b.add(slot, PartShape::Cylinder { radius, height: axis.length() }, transform);
}

/// Quartier général : le cœur de la base. Socle sombre, grande coque d'acier
/// aux arêtes chanfreinées, étage peint portant une plateforme d'atterrissage
/// cerclée de la couleur d'équipe, tour de commandement à bandeau lumineux et
/// radar qui tourne, pylônes d'angle à gyrophares. Vu de dessus : un grand
/// carré, un disque d'équipe et deux bandes d'équipe sur le toit.
fn quartier_general() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    // Socle et tablier de l'entrée.
    b.plate_beveled(Dark, v(6.8, 0.4, 6.8), 0.12, v(0.0, 0.2, 0.0), Vec3::ZERO);
    b.plate_beveled(Hazard, v(3.4, 0.05, 0.7), 0.01, v(0.0, 0.42, -3.0), Vec3::ZERO);
    // Coque principale : profil (z, y) aux arêtes hautes chanfreinées.
    let hull = [v2(-2.9, 0.4), v2(2.9, 0.4), v2(2.9, 1.7), v2(2.3, 2.6), v2(-2.3, 2.6), v2(-2.9, 1.7)];
    b.beveled_prism(Steel, &hull, 5.8, 0.08, Vec3::ZERO, Vec3::ZERO);
    b.mirrored(|b, s| {
        // Blindage incliné sur les flancs, bordé d'une bande de chevrons.
        b.plate_beveled(Paint, v(0.3, 1.5, 5.0), 0.06, v(s * 2.95, 1.25, 0.0), v(0.0, 0.0, s * 0.22));
        b.plate_beveled(Hazard, v(0.32, 0.22, 5.02), 0.02, v(s * 3.02, 0.62, 0.0), v(0.0, 0.0, s * 0.22));
        // Bandes d'équipe sur le toit de la coque, bien visibles de dessus.
        b.plate_beveled(Team, v(0.5, 0.06, 4.4), 0.02, v(s * 1.9, 2.63, 0.0), Vec3::ZERO);
        // Feux de l'entrée.
        b.plate(Glow, v(0.35, 0.12, 0.05), v(s * 1.25, 2.2, -2.94));
    });
    // Porte du hangar en façade, surmontée de chevrons.
    b.plate_beveled(Dark, v(2.4, 1.5, 0.12), 0.03, v(0.0, 1.15, -2.92), Vec3::ZERO);
    for slat in 0..5 {
        b.plate(Steel, v(2.3, 0.05, 0.04), v(0.0, 0.6 + slat as f32 * 0.28, -2.99));
    }
    b.plate_beveled(Hazard, v(2.7, 0.22, 0.1), 0.02, v(0.0, 2.0, -2.95), Vec3::ZERO);
    // Grilles d'aération à l'avant du toit.
    for bar in 0..6 {
        b.plate(Bare, v(2.6, 0.04, 0.1), v(0.0, 2.64, -2.0 + bar as f32 * 0.2));
    }
    // Étage peint, à l'arrière.
    b.plate_beveled(Paint, v(3.6, 1.0, 3.4), 0.1, v(0.0, 3.1, 0.7), Vec3::ZERO);
    b.plate(Glow, v(3.0, 0.14, 0.04), v(0.0, 3.25, -1.01)); // baie vitrée
    // Plateforme d'atterrissage cerclée de la couleur d'équipe, et ses balises.
    b.cylinder(Team, 1.35, 0.06, v(0.5, 3.63, 0.9), Vec3::ZERO);
    b.cylinder(Dark, 1.15, 0.1, v(0.5, 3.65, 0.9), Vec3::ZERO);
    b.plate(Paint, v(0.9, 0.02, 0.18), v(0.5, 3.71, 0.9));
    b.plate(Paint, v(0.18, 0.02, 0.9), v(0.5, 3.71, 0.9));
    b.pulsing(BEACON_SLOW, |b| {
        for (x, z) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            b.sphere(Engine, 0.06, v(0.5 + x * 1.2, 3.72, 0.9 + z * 1.2));
        }
    });
    // Tour de commandement, à gauche, avec son bandeau de fenêtres.
    b.cylinder(Steel, 0.75, 2.6, v(-1.9, 3.9, 1.7), Vec3::ZERO);
    b.cylinder(Glow, 0.77, 0.18, v(-1.9, 4.7, 1.7), Vec3::ZERO);
    b.cylinder(Dark, 0.9, 0.25, v(-1.9, 5.3, 1.7), Vec3::ZERO);
    b.cylinder(Team, 0.92, 0.08, v(-1.9, 5.45, 1.7), Vec3::ZERO);
    b.cylinder(Dark, 0.06, 0.8, v(-1.9, 5.8, 1.7), Vec3::ZERO);
    // Radar : une parabole qui tourne au sommet de la tour.
    b.animated(Transform::from_xyz(-1.9, 6.2, 1.7), Motion::Spin { axis: Vec3::Y, speed: 0.9 }, |b| {
        let dish = [v2(0.0, -0.05), v2(0.35, 0.0), v2(0.6, 0.12), v2(0.72, 0.28)];
        b.lathe(Steel, &dish, 16, v(0.0, 0.0, -0.1), v(-1.1, 0.0, 0.0));
        b.cylinder(Glow, 0.05, 0.3, v(0.0, 0.1, -0.35), v(-1.1, 0.0, 0.0));
    });
    // Pylônes d'angle et leurs gyrophares.
    for (x, z) in [(-3.1, -3.1), (3.1, -3.1), (-3.1, 3.1), (3.1, 3.1)] {
        b.cylinder(Steel, 0.22, 2.6, v(x, 1.3, z), Vec3::ZERO);
        b.cylinder(Dark, 0.3, 0.2, v(x, 2.65, z), Vec3::ZERO);
    }
    b.pulsing(BEACON_PULSE, |b| {
        for (x, z) in [(-3.1, -3.1), (3.1, -3.1), (-3.1, 3.1), (3.1, 3.1)] {
            b.sphere(Engine, 0.13, v(x, 2.85, z));
        }
    });
    // Antennes et caisses sur le toit.
    b.cylinder(Dark, 0.03, 1.4, v(1.8, 4.3, 2.0), Vec3::ZERO);
    b.cylinder(Dark, 0.03, 1.0, v(1.5, 4.1, 2.2), Vec3::ZERO);
    b.plate(Steel, v(0.7, 0.4, 0.5), v(2.3, 2.8, 2.4));
    b
}

/// Module d'habitat : petit module octogonal peint, coiffé d'un dôme d'acier,
/// cerclé d'un bandeau d'équipe et d'une ceinture de hublots lumineux. Vu de
/// dessus : un disque, reconnaissable entre tous.
fn habitat() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let ring = |r: f32, y0: f32, y1: f32| [v2(0.0, y0), v2(r, y0), v2(r, y1), v2(0.0, y1)];
    b.faceted(Dark, &ring(1.45, 0.0, 0.3), 8, Vec3::ZERO, Vec3::ZERO); // socle
    b.faceted(Paint, &ring(1.22, 0.3, 1.45), 8, Vec3::ZERO, Vec3::ZERO); // corps
    b.faceted(Glow, &ring(1.24, 0.95, 1.07), 8, Vec3::ZERO, Vec3::ZERO); // hublots
    b.faceted(Team, &ring(1.28, 1.42, 1.56), 8, Vec3::ZERO, Vec3::ZERO); // bandeau d'équipe
    // Le dôme : un profil (rayon, hauteur) qui s'arrondit vers le sommet.
    let dome = [v2(1.15, 1.56), v2(1.08, 1.85), v2(0.88, 2.1), v2(0.55, 2.27), v2(0.0, 2.34)];
    b.lathe(Steel, &dome, 24, Vec3::ZERO, Vec3::ZERO);
    b.cylinder(Dark, 0.32, 0.12, v(0.0, 2.36, 0.0), Vec3::ZERO); // trappe
    b.pulsing(BEACON_SLOW, |b| {
        b.sphere(Engine, 0.08, v(0.0, 2.47, 0.0));
    });
    // Sas d'entrée en façade, chevrons au-dessus.
    b.plate_beveled(Steel, v(0.95, 1.15, 0.4), 0.05, v(0.0, 0.88, -1.18), Vec3::ZERO);
    b.plate(Dark, v(0.6, 0.9, 0.05), v(0.0, 0.78, -1.39));
    b.plate_beveled(Hazard, v(0.95, 0.16, 0.42), 0.01, v(0.0, 1.5, -1.18), Vec3::ZERO);
    // Bouches d'aération sur les côtés.
    b.mirrored(|b, s| {
        b.plate_beveled(Steel, v(0.35, 0.45, 0.6), 0.04, v(s * 1.2, 0.6, 0.3), Vec3::ZERO);
        b.plate(Bare, v(0.04, 0.3, 0.45), v(s * 1.39, 0.6, 0.3));
    });
    b
}

/// Extracteur : tour de forage en treillis au-dessus d'un puits lumineux,
/// tête de forage qui tourne, deux cuves cerclées de la couleur d'équipe à
/// l'arrière, reliées au puits par des durites. Vu de dessus : un carré de
/// treillis et deux disques côte à côte.
fn extracteur() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.plate_beveled(Dark, v(3.8, 0.35, 3.8), 0.1, v(0.0, 0.175, 0.0), Vec3::ZERO);
    b.plate_beveled(Hazard, v(3.0, 0.05, 0.5), 0.01, v(0.0, 0.37, -1.6), Vec3::ZERO);
    // Le puits : margelle d'acier et cœur lumineux (la ressource rare).
    b.cylinder(Steel, 0.75, 0.3, v(0.0, 0.5, -0.5), Vec3::ZERO);
    b.pulsing(ENGINE_PULSE, |b| {
        b.cylinder(Glow, 0.55, 0.06, v(0.0, 0.66, -0.5), Vec3::ZERO);
    });
    // Treillis : quatre montants qui se resserrent vers le haut, et des entretoises.
    let foot = 1.0;
    let head = 0.32;
    let top = 3.9;
    for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        strut(&mut b, Steel, v(x * foot, 0.35, -0.5 + z * foot), v(x * head, top, -0.5 + z * head), 0.07);
    }
    for level in 1..4 {
        let t = level as f32 / 4.0;
        let half = foot + (head - foot) * t;
        let y = 0.35 + (top - 0.35) * t;
        let slot = if level == 2 { Team } else { Dark };
        b.plate(slot, v(half * 2.0 + 0.1, 0.1, 0.1), v(0.0, y, -0.5 - half));
        b.plate(slot, v(half * 2.0 + 0.1, 0.1, 0.1), v(0.0, y, -0.5 + half));
        b.plate(slot, v(0.1, 0.1, half * 2.0 + 0.1), v(-half, y, -0.5));
        b.plate(slot, v(0.1, 0.1, half * 2.0 + 0.1), v(half, y, -0.5));
    }
    b.plate_beveled(Paint, v(0.9, 0.35, 0.9), 0.05, v(0.0, top + 0.15, -0.5), Vec3::ZERO); // chapeau
    b.pulsing(BEACON_PULSE, |b| {
        b.sphere(Engine, 0.09, v(0.0, top + 0.42, -0.5));
    });
    // Tête de forage : arbre et collerettes qui tournent.
    b.animated(Transform::from_xyz(0.0, 2.0, -0.5), Motion::Spin { axis: Vec3::Y, speed: 3.0 }, |b| {
        b.cylinder(Bare, 0.11, 3.2, Vec3::ZERO, Vec3::ZERO);
        for k in 0..3 {
            b.plate(Steel, v(0.55, 0.08, 0.14), v(0.0, -1.0 + k as f32 * 0.9, 0.0));
        }
    });
    // Deux cuves à l'arrière.
    b.mirrored(|b, s| {
        let tank = [v2(0.0, 0.35), v2(0.62, 0.35), v2(0.62, 1.9), v2(0.5, 2.15), v2(0.25, 2.27), v2(0.0, 2.3)];
        b.lathe(Paint, &tank, 20, v(s * 0.95, 0.0, 1.15), Vec3::ZERO);
        b.cylinder(Team, 0.64, 0.18, v(s * 0.95, 1.55, 1.15), Vec3::ZERO);
        b.cylinder(Dark, 0.64, 0.1, v(s * 0.95, 0.75, 1.15), Vec3::ZERO);
        b.tube(Dark, &[v(s * 0.6, 1.1, 0.75), v(s * 0.45, 1.3, 0.2), v(s * 0.25, 0.75, -0.2)], 0.07);
    });
    b
}

/// Caserne : long hangar à toit en voûte, nervuré, une bande d'équipe sur le
/// faîte, grande porte en façade cerclée de chevrons, projecteurs et groupe
/// électrogène à l'arrière. Vu de dessus : un rectangle rayé de nervures.
fn caserne() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    b.plate_beveled(Dark, v(4.8, 0.3, 5.8), 0.1, v(0.0, 0.15, 0.0), Vec3::ZERO);
    // Voûte : profil dans le plan (x, y) de la façade, extrudé en profondeur.
    // `beveled_prism` dessine le profil en (z, y) et l'extrude le long de x :
    // on le tourne d'un quart de tour pour que la voûte regarde l'avant.
    let arch = |w: f32, h: f32| [v2(-w, 0.3), v2(w, 0.3), v2(w, 1.5), v2(w * 0.72, h * 0.85), v2(w * 0.3, h), v2(-w * 0.3, h), v2(-w * 0.72, h * 0.85), v2(-w, 1.5)];
    b.beveled_prism(Paint, &arch(2.15, 3.1), 5.0, 0.07, v(0.0, 0.0, 0.1), v(0.0, FRAC_PI_2, 0.0));
    // Nervures d'acier sombre tous les mètres : bien lisibles de dessus.
    for k in 0..6 {
        b.beveled_prism(Dark, &arch(2.22, 3.17), 0.16, 0.03, v(0.0, 0.0, -2.3 + k as f32 * 0.98), v(0.0, FRAC_PI_2, 0.0));
    }
    b.plate_beveled(Team, v(0.9, 0.08, 4.9), 0.02, v(0.0, 3.12, 0.1), Vec3::ZERO); // faîte d'équipe
    // Façade : pignon d'acier, porte, chevrons.
    b.beveled_prism(Steel, &arch(2.1, 3.05), 0.12, 0.03, v(0.0, 0.0, -2.42), v(0.0, FRAC_PI_2, 0.0));
    b.plate_beveled(Dark, v(2.0, 1.9, 0.1), 0.03, v(0.0, 1.25, -2.5), Vec3::ZERO);
    for slat in 0..6 {
        b.plate(Steel, v(1.9, 0.05, 0.04), v(0.0, 0.5 + slat as f32 * 0.3, -2.56));
    }
    b.mirrored(|b, s| {
        b.plate_beveled(Hazard, v(0.22, 2.1, 0.12), 0.02, v(s * 1.12, 1.25, -2.5), Vec3::ZERO);
        // Projecteurs de façade et feux de position.
        b.plate_beveled(Dark, v(0.3, 0.2, 0.25), 0.03, v(s * 1.55, 2.25, -2.5), Vec3::ZERO);
        b.plate(Glow, v(0.24, 0.12, 0.04), v(s * 1.55, 2.25, -2.63));
        // Annexes latérales : modules d'acier aux fenêtres lumineuses.
        b.plate_beveled(Steel, v(0.5, 1.0, 2.2), 0.06, v(s * 2.2, 0.8, 0.6), Vec3::ZERO);
        b.plate(Glow, v(0.04, 0.14, 1.6), v(s * 2.46, 1.0, 0.6));
        b.plate_beveled(Team, v(0.52, 0.08, 2.22), 0.02, v(s * 2.2, 1.33, 0.6), Vec3::ZERO);
    });
    b.plate_beveled(Hazard, v(2.4, 0.22, 0.12), 0.02, v(0.0, 2.32, -2.5), Vec3::ZERO);
    // Arrière : groupe électrogène et ses échappements.
    b.plate_beveled(Dark, v(2.4, 1.1, 0.7), 0.06, v(0.0, 0.85, 2.85), Vec3::ZERO);
    b.mirrored(|b, s| {
        b.cylinder(Dark, 0.12, 0.6, v(s * 0.7, 1.65, 2.85), Vec3::ZERO);
        b.pulsing(ENGINE_PULSE, |b| {
            b.cylinder(Engine, 0.09, 0.03, v(s * 0.7, 1.96, 2.85), Vec3::ZERO);
        });
    });
    b.cylinder(Dark, 0.03, 1.4, v(1.2, 3.2, 1.6), Vec3::ZERO); // antenne
    b
}

/// Tourelle : petite défense fixe. Socle octogonal cerclé de chevrons, fût
/// d'acier, tête biseautée à deux canons qui balaie l'horizon, plaque
/// d'équipe sur le dessus.
fn tourelle() -> ModelBuilder {
    let mut b = ModelBuilder::new();
    let ring = |r: f32, y0: f32, y1: f32| [v2(0.0, y0), v2(r, y0), v2(r, y1), v2(0.0, y1)];
    b.faceted(Dark, &ring(0.95, 0.0, 0.35), 8, Vec3::ZERO, Vec3::ZERO);
    b.faceted(Hazard, &ring(0.97, 0.3, 0.42), 8, Vec3::ZERO, Vec3::ZERO);
    b.cylinder(Steel, 0.45, 0.7, v(0.0, 0.75, 0.0), Vec3::ZERO);
    b.cylinder(Dark, 0.55, 0.12, v(0.0, 1.12, 0.0), Vec3::ZERO);
    // La tête balaie lentement (±40°).
    let sweep = Motion::Swing { axis: Vec3::Y, amplitude: 0.7, frequency: 0.09 };
    b.animated(Transform::from_xyz(0.0, 1.18, 0.0), sweep, |b| {
        // Profil (z, y) de la tête : avant (−z) chanfreiné.
        let head = [v2(-0.75, 0.05), v2(0.55, 0.05), v2(0.6, 0.45), v2(-0.35, 0.6), v2(-0.75, 0.3)];
        b.beveled_prism(Steel, &head, 0.95, 0.05, Vec3::ZERO, Vec3::ZERO);
        b.plate_beveled(Team, v(0.6, 0.05, 0.55), 0.02, v(0.0, 0.55, 0.1), v(-0.15, 0.0, 0.0));
        b.mirrored(|b, s| {
            b.cylinder(Bare, 0.06, 1.0, v(s * 0.2, 0.28, -1.1), ALONG_Z);
            b.cylinder(Dark, 0.09, 0.18, v(s * 0.2, 0.28, -1.55), ALONG_Z);
            b.plate(Paint, v(0.12, 0.4, 0.7), v(s * 0.53, 0.3, 0.05));
        });
        b.plate(Glow, v(0.3, 0.08, 0.04), v(0.0, 0.4, -0.66));
        b.plate(Dark, v(0.5, 0.3, 0.3), v(0.0, 0.3, 0.7)); // boîtier arrière
    });
    b
}
