//! Le jeu : contenu au-dessus du moteur `rts_engine`.
//!
//! Pour l'instant : une carte plate, et une faction affichée à la fois — la
//! Coalition (nom provisoire, par défaut), les Sylves ou le Collectif — avec
//! son ambiance, son sol et une dizaine de ses unités sur le terrain.
//!
//! Variables d'environnement utiles en développement :
//! - `OPENRTS_FACTION=sylves` (ou `collectif`, `coalition`) : faction affichée ;
//! - `OPENRTS_WINDOWED=1` : fenêtre au lieu du plein écran, souris libre ;
//! - `OPENRTS_QUALITY=bas` (ou `haut`) : qualité graphique ;
//! - `OPENRTS_STRESS=1` : lance le test de charge (300 unités) au démarrage ;
//! - `OPENRTS_SCREENSHOT=/chemin/capture.png` : enregistre une capture après
//!   quelques secondes, affiche les FPS moyens, puis quitte ;
//! - `OPENRTS_ZOOM=12` : distance de départ de la caméra, de 10 à 70 m ;
//! - `OPENRTS_DEMO=buildings` : scène de démonstration des bâtiments (Coalition).

mod base;
mod coalition;
mod collectif;
mod collectif_or_rose;
mod gisements;
mod map;
mod sylves;

use std::path::PathBuf;

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, WindowMode};
use rts_engine::camera::RtsCamera;
use rts_engine::collision::MapBounds;
use rts_engine::cursor::CursorCaptureSettings;
use rts_engine::devtools::AutoScreenshot;
use rts_engine::graphics::{Ambiance, GraphicsQuality, ReflectionPanels};
use rts_engine::input::GameAction;
use rts_engine::navigation::{GridFrame, NavSettings};
use rts_engine::units::{Palette, UnitCatalog, UnitType};
use rts_engine::RtsEnginePlugin;

use map::{GroundPattern, GroundStyle};

/// Côté de la carte, en mètres.
const MAP_SIZE: f32 = 100.0;

/// La faction affichée. Une seule à la fois pour l'instant : elle décide de
/// l'ambiance, du sol, des matériaux et des types d'unités du catalogue.
///
/// `#[derive(Resource)]` : la faction est rangée dans le monde de Bevy comme
/// une ressource, que les systèmes de démarrage lisent avec `Res<Faction>`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
enum Faction {
    Coalition,
    Sylves,
    Collectif,
    /// Essai de direction artistique « tout or rose » du Collectif.
    CollectifOrRose,
}

impl Faction {
    /// `OPENRTS_FACTION=sylves` choisit les Sylves, `collectif` le Collectif ;
    /// tout le reste, la Coalition.
    fn from_env() -> Self {
        match std::env::var("OPENRTS_FACTION").map(|f| f.to_lowercase()).as_deref() {
            Ok("sylves") => Faction::Sylves,
            Ok("collectif") => Faction::Collectif,
            Ok("collectif-or-rose") => Faction::CollectifOrRose,
            _ => Faction::Coalition,
        }
    }

    fn ambiance(self) -> Ambiance {
        match self {
            Faction::Coalition => coalition::ambiance(),
            Faction::Sylves => sylves::ambiance(),
            Faction::Collectif => collectif::ambiance(),
            Faction::CollectifOrRose => collectif_or_rose::ambiance(),
        }
    }

    /// Panneaux de reflet en plus de l'ambiance (vitrine de bijoutier du Collectif).
    fn reflection_panels(self) -> Option<ReflectionPanels> {
        match self {
            Faction::Coalition | Faction::Sylves => None,
            Faction::Collectif => Some(collectif::reflection_panels()),
            Faction::CollectifOrRose => Some(collectif_or_rose::reflection_panels()),
        }
    }

    fn ground(self) -> GroundStyle {
        match self {
            Faction::Coalition => GroundStyle::rock(),
            Faction::Sylves => sylves::ground(),
            Faction::Collectif => collectif::ground(),
            Faction::CollectifOrRose => collectif_or_rose::ground(),
        }
    }

    fn ground_pattern(self) -> GroundPattern {
        match self {
            Faction::Coalition | Faction::Sylves => GroundPattern::Natural,
            Faction::Collectif => collectif::ground_pattern(),
            Faction::CollectifOrRose => collectif_or_rose::ground_pattern(),
        }
    }

    fn palette(self, team_color: Color) -> Palette {
        match self {
            Faction::Coalition => coalition::palette(team_color),
            Faction::Sylves => sylves::palette(team_color),
            Faction::Collectif => collectif::palette(team_color),
            Faction::CollectifOrRose => collectif_or_rose::palette(team_color),
        }
    }

    fn unit_types(self) -> Vec<UnitType> {
        match self {
            Faction::Coalition => coalition::unit_types(),
            Faction::Sylves => sylves::unit_types(),
            Faction::Collectif => collectif::unit_types(),
            Faction::CollectifOrRose => collectif_or_rose::unit_types(),
        }
    }

    /// L'armée de démonstration : (type d'unité, position au sol).
    fn army(self) -> Vec<(usize, Vec2)> {
        match self {
            Faction::Coalition => {
                use coalition::kind::*;
                vec![
                    (BASTION, Vec2::new(2.5, -3.5)),
                    (LANCIER, Vec2::new(-2.5, -2.5)),
                    (LANCIER, Vec2::new(-4.5, -3.0)),
                    (LONGUE_VUE, Vec2::new(-6.5, -1.0)),
                    (SENTINELLE, Vec2::new(-3.0, 1.0)),
                    (SENTINELLE, Vec2::new(-1.5, 1.6)),
                    (SENTINELLE, Vec2::new(0.0, 1.0)),
                    (SENTINELLE, Vec2::new(1.5, 1.6)),
                    (FAUCON, Vec2::new(5.0, 1.0)),
                    (FAUCON, Vec2::new(7.0, -0.5)),
                    (TECHNICIEN, Vec2::new(-5.5, 2.5)),
                ]
            }
            Faction::Sylves => {
                use sylves::kind::*;
                vec![
                    (GARDIEN, Vec2::new(2.8, -1.8)),
                    (LANCEUR, Vec2::new(-5.5, -2.2)),
                    (RONCIER, Vec2::new(-3.2, 0.8)),
                    (RONCIER, Vec2::new(-1.4, 1.8)),
                    (RONCIER, Vec2::new(0.4, 0.9)),
                    (RONCIER, Vec2::new(-2.2, -1.0)),
                    (SOURCIER, Vec2::new(-0.6, -1.4)),
                    (SAMARE, Vec2::new(5.5, 1.0)),
                    (SAMARE, Vec2::new(7.2, -1.0)),
                    (POUSSE, Vec2::new(-6.0, 2.2)),
                    (POUSSE, Vec2::new(-4.8, 3.0)),
                    (POUSSE, Vec2::new(3.0, 2.5)),
                ]
            }
            Faction::Collectif => {
                use collectif::kind::*;
                vec![
                    (TITAN, Vec2::new(3.0, -1.2)),
                    (ESSAIMEUR, Vec2::new(-3.0, -4.0)),
                    (AUTOMATE, Vec2::new(-2.6, 1.0)),
                    (AUTOMATE, Vec2::new(-1.2, 1.8)),
                    (AUTOMATE, Vec2::new(0.2, 1.0)),
                    (PERCEUR, Vec2::new(-5.2, -1.4)),
                    (PERCEUR, Vec2::new(6.2, 0.6)),
                    (ESSAIM, Vec2::new(7.0, -4.0)),
                    (ESSAIM, Vec2::new(3.4, 2.8)),
                    (COLLECTEUR, Vec2::new(-6.6, 2.6)),
                    (COLLECTEUR, Vec2::new(-4.6, 3.3)),
                ]
            }
            // Une unité de chaque type, serrées pour tenir dans une vue rapprochée.
            // Mêmes indices que le Collectif (le Fauve est à la place du Perceur).
            Faction::CollectifOrRose => {
                use collectif::kind::*;
                vec![
                    (TITAN, Vec2::new(3.6, -0.2)),
                    (ESSAIMEUR, Vec2::new(-3.6, -0.3)),
                    (AUTOMATE, Vec2::new(-1.0, 3.0)),
                    (PERCEUR, Vec2::new(5.8, 3.2)),
                    (ESSAIM, Vec2::new(2.2, 3.6)),
                    (COLLECTEUR, Vec2::new(-5.6, 3.4)),
                ]
            }
        }
    }
}

fn main() {
    // Mode développeur : `OPENRTS_WINDOWED=1 cargo run` ouvre le jeu dans une
    // fenêtre et laisse la souris libre, pratique pour tester à côté d'un éditeur.
    let windowed = std::env::var_os("OPENRTS_WINDOWED").is_some();
    let screenshot = std::env::var_os("OPENRTS_SCREENSHOT").map(PathBuf::from);
    let stress = std::env::var_os("OPENRTS_STRESS").is_some();
    let quality = match std::env::var("OPENRTS_QUALITY").as_deref() {
        Ok("bas" | "low") => GraphicsQuality::Low,
        _ => GraphicsQuality::High,
    };
    let faction = Faction::from_env();

    let half = MAP_SIZE / 2.0;
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "openRTS".into(),
            mode: if windowed {
                WindowMode::Windowed
            } else {
                // Plein écran sans bordure, sur l'écran principal.
                WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
            },
            // Pour mesurer les FPS, on ne se cale pas sur le rafraîchissement de l'écran.
            present_mode: if screenshot.is_some() { PresentMode::AutoNoVsync } else { PresentMode::AutoVsync },
            ..default()
        }),
        ..default()
    }))
    .add_plugins(RtsEnginePlugin)
    .insert_resource(CursorCaptureSettings {
        enabled: !windowed,
        ..default()
    })
    .insert_resource(MapBounds(Rect::new(-half, -half, half, half)))
    .insert_resource(quality)
    .insert_resource(faction)
    .insert_resource(faction.ambiance())
    .add_systems(Startup, (setup_world, spawn_units))
    .add_systems(Update, base::focus_camera);

    if let Some(panels) = faction.reflection_panels() {
        app.insert_resource(panels);
    }
    if let Some(path) = screenshot {
        app.insert_resource(AutoScreenshot { path, delay_secs: 14.0 })
            .add_plugins((FrameTimeDiagnosticsPlugin::default(), LogDiagnosticsPlugin::default()));
    }
    if stress {
        app.add_systems(Startup, |mut actions: MessageWriter<GameAction>| {
            actions.write(GameAction::SpawnStressTest);
        });
    }
    if faction == Faction::Coalition && base::demo_requested() {
        app.add_plugins(base::DemoPlugin);
    }
    app.run();
}

fn setup_world(
    mut commands: Commands,
    faction: Res<Faction>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let ground = faction.ground();
    map::spawn_ground(&mut commands, &mut meshes, &mut materials, &mut images, MAP_SIZE, &ground, faction.ground_pattern());

    // La caméra RTS. Le moteur lui ajoute HDR, tone mapping, bloom, brouillard
    // et IBL selon la qualité graphique et l'ambiance.
    let half = MAP_SIZE / 2.0;
    let mut rts_camera = RtsCamera {
        bounds: Rect::new(-half, -half, half, half),
        ..default()
    };
    // `OPENRTS_ZOOM=12` : distance de départ de la caméra (pratique pour les captures).
    if let Some(distance) = std::env::var("OPENRTS_ZOOM").ok().and_then(|d| d.parse::<f32>().ok()) {
        rts_camera.distance = distance.clamp(rts_camera.min_distance, rts_camera.max_distance);
    }
    commands.spawn((Camera3d::default(), rts_camera.transform(), rts_camera));
}

fn spawn_units(
    mut commands: Commands,
    faction: Res<Faction>,
    bounds: Res<MapBounds>,
    nav: Res<NavSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Le premier joueur a l'orange signal comme couleur d'équipe, quelle que soit sa faction.
    let palette = faction.palette(coalition::SIGNAL_ORANGE);
    let catalog = UnitCatalog::new(faction.unit_types(), &palette, &mut meshes, &mut materials, &mut images);

    // Bâtiments : la Coalition seulement pour l'instant (base de départ, ou
    // scène de démonstration avec `OPENRTS_DEMO=buildings`).
    if *faction == Faction::Coalition {
        let buildings = base::building_catalog(&catalog, &mut meshes, &mut materials);
        let frame = GridFrame::new(bounds.0, &nav);
        let demo = base::demo_requested();
        if demo {
            base::spawn_demo(&mut commands, &buildings, &catalog, frame);
        } else {
            let deposits = gisements::DepositVisuals::new(&mut meshes, &mut materials);
            base::spawn_start(&mut commands, &buildings, &catalog, &deposits, frame);
        }
        commands.insert_resource(buildings);
        commands.insert_resource(coalition::economy_rules(gisements::resource_infos()));
        if demo {
            commands.insert_resource(catalog);
            return;
        }
    }

    // Une dizaine d'unités variées, tournées vers la caméra (+z). L'essai
    // « or rose » les montre de trois quarts : les bijoux s'y lisent mieux.
    let yaw = match *faction {
        Faction::CollectifOrRose => std::f32::consts::PI + 0.75,
        _ => std::f32::consts::PI,
    };
    for (type_index, pos) in faction.army() {
        catalog.spawn_facing(&mut commands, type_index, pos, yaw);
    }

    commands.insert_resource(catalog);
}
