//! Le jeu : contenu au-dessus du moteur `rts_engine`.
//!
//! Pour l'instant : une carte plate et deux joueurs. Le joueur humain, au
//! nord, joue la Garde (par défaut), les Sylves ou le Collectif, qui
//! décide de l'ambiance et du sol ; l'adversaire, au sud, joue une autre
//! faction (pas encore d'IA). Chacun a sa base de départ et une dizaine
//! d'unités de présentation.
//!
//! Variables d'environnement utiles en développement :
//! - `OPENRTS_FACTION=sylves` (ou `collectif`, `garde` ; l'ancien nom `coalition` reste accepté) : faction du joueur ;
//! - `OPENRTS_ENEMY=garde` (ou `sylves`, `collectif`) : faction de
//!   l'adversaire ; par défaut, la suivante (Garde → Sylves → Collectif → Garde) ;
//! - `OPENRTS_WINDOWED=1` : fenêtre au lieu du plein écran, souris libre ;
//! - `OPENRTS_QUALITY=bas` (ou `haut`) : qualité graphique ;
//! - `OPENRTS_STRESS=1` : lance le test de charge (300 unités) au démarrage ;
//! - `OPENRTS_SCREENSHOT=/chemin/capture.png` : enregistre une capture après
//!   quelques secondes, affiche les FPS moyens, puis quitte ;
//! - `OPENRTS_ZOOM=12` : distance de départ de la caméra, de 10 à 70 m ;
//! - `OPENRTS_DEMO=buildings` : scène de démonstration des bâtiments
//!   (Garde, Sylves avec `OPENRTS_FACTION=sylves`, Collectif avec
//!   `OPENRTS_FACTION=collectif`) ;
//! - `OPENRTS_DEMO=economy` : l'économie en marche (récolte, production) ;
//! - `OPENRTS_DEMO=battle` : deux armées qui s'affrontent (300 contre 300
//!   avec `OPENRTS_STRESS=1`) ;
//! - `OPENRTS_POP_MAX=100` : plafond global de population.

mod base;
mod base_collectif;
mod base_sylves;
mod battle;
mod combat;
mod garde;
mod collectif;
mod collectif_or_rose;
mod gisements;
mod map;
mod side;
mod sylves;

use std::path::PathBuf;

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, WindowMode};
use rts_engine::buildings::BuildingCatalog;
use rts_engine::camera::RtsCamera;
use rts_engine::collision::MapBounds;
use rts_engine::combat::CombatRules;
use rts_engine::cursor::CursorCaptureSettings;
use rts_engine::devtools::AutoScreenshot;
use rts_engine::economy::production::PopulationSettings;
use rts_engine::economy::{Cost, EconomyRules};
use rts_engine::players::{PlayerId, Players};
use rts_engine::graphics::{Ambiance, GraphicsQuality, ReflectionPanels};
use rts_engine::input::GameAction;
use rts_engine::navigation::{GridFrame, NavSettings};
use rts_engine::units::{Palette, UnitCatalog, UnitType};
use rts_engine::RtsEnginePlugin;

use map::{GroundPattern, GroundStyle};
use side::{Side, StartPlan};

/// Côté de la carte, en mètres.
const MAP_SIZE: f32 = 100.0;

/// Le joueur humain.
pub(crate) const HUMAN: PlayerId = PlayerId(0);

/// Le joueur adverse (pas encore d'IA : ses unités attendent, et ripostent).
pub(crate) const ENEMY: PlayerId = PlayerId(1);

/// Bleu froid : la couleur d'équipe du second joueur, bien distincte de
/// l'orange signal du premier et des lueurs cyan, turquoise ou émeraude des factions.
const COLD_BLUE: Color = Color::srgb(0x2E as f32 / 255.0, 0x6C as f32 / 255.0, 0xF0 as f32 / 255.0);

/// Couleur d'équipe de chaque joueur, dans l'ordre des numéros.
const TEAM_COLORS: [Color; 2] = [garde::SIGNAL_ORANGE, COLD_BLUE];

/// Une faction jouable : ses types d'unités et de bâtiments, ses règles,
/// ses couleurs. La ressource `Faction` est celle du joueur humain (elle
/// décide aussi de l'ambiance et du sol) ; [`EnemyFaction`], celle de
/// l'adversaire.
///
/// `#[derive(Resource)]` : la faction est rangée dans le monde de Bevy comme
/// une ressource, que les systèmes de démarrage lisent avec `Res<Faction>`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
enum Faction {
    Garde,
    Sylves,
    Collectif,
    /// Essai de direction artistique « tout or rose » du Collectif.
    CollectifOrRose,
}

impl Faction {
    /// `OPENRTS_FACTION=sylves` choisit les Sylves, `collectif` le Collectif ;
    /// tout le reste, la Garde (y compris l'ancien nom `coalition`).
    fn from_env() -> Self {
        std::env::var("OPENRTS_FACTION").ok().and_then(|f| Self::parse(&f)).unwrap_or(Faction::Garde)
    }

    fn parse(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "garde" | "coalition" => Some(Faction::Garde),
            "sylves" => Some(Faction::Sylves),
            "collectif" => Some(Faction::Collectif),
            "collectif-or-rose" => Some(Faction::CollectifOrRose),
            _ => None,
        }
    }

    /// La faction adverse par défaut : la suivante, pour que les deux camps
    /// diffèrent. L'essai « or rose », sans bâtiments, affronte lui-même.
    fn default_enemy(self) -> Self {
        match self {
            Faction::Garde => Faction::Sylves,
            Faction::Sylves => Faction::Collectif,
            Faction::Collectif => Faction::Garde,
            Faction::CollectifOrRose => Faction::CollectifOrRose,
        }
    }

    /// Ajoute les bâtiments de la faction (numéro `id` des catalogues) au
    /// catalogue commun ; renvoie l'indice du premier, ou `None` pour une
    /// faction sans bâtiments.
    fn add_buildings(self, catalog: &mut BuildingCatalog, id: usize, units: &UnitCatalog, meshes: &mut Assets<Mesh>) -> Option<usize> {
        let first = match self {
            // Les Techniciens construisent, et restent sur le chantier jusqu'au bout.
            Faction::Garde => catalog.add_faction(
                id,
                garde::building_types(),
                vec![garde::kind::TECHNICIEN],
                rts_engine::buildings::ConstructionStyle::WorkerStays,
                units,
                meshes,
            ),
            // Les Pousses construisent en devenant le bâtiment.
            Faction::Sylves => catalog.add_faction(id, sylves::building_types(), vec![sylves::kind::POUSSE], sylves::CONSTRUCTION, units, meshes),
            // Les Collecteurs sertissent à distance, et une gemme d'ancrage
            // flotte au-dessus de chaque chantier.
            Faction::Collectif => {
                let first =
                    catalog.add_faction(id, collectif::building_types(), vec![collectif::kind::COLLECTEUR], collectif::CONSTRUCTION, units, meshes);
                catalog.set_anchor_marker(id, &collectif::anchor_gem(), units, meshes);
                first
            }
            Faction::CollectifOrRose => return None,
        };
        Some(first)
    }

    /// Les règles économiques de la faction (`None` : pas d'économie).
    fn economy_rules(self) -> Option<EconomyRules> {
        let resources = gisements::resource_infos();
        match self {
            Faction::Garde => Some(garde::economy_rules(resources)),
            Faction::Sylves => Some(sylves::economy_rules(resources)),
            Faction::Collectif => Some(collectif::economy_rules(resources)),
            Faction::CollectifOrRose => None,
        }
    }

    /// La base de départ d'une partie normale (types locaux).
    fn start_plan(self) -> Option<StartPlan> {
        // Deux bâtiments de population (trois Résonateurs pour le Collectif) :
        // l'armée de présentation occupe déjà plus que les 15 de population
        // du bâtiment principal.
        let pair = vec![Vec2::new(-8.5, -8.5), Vec2::new(8.5, -8.5)];
        match self {
            Faction::Garde => {
                use garde::{building::*, kind::*};
                Some(StartPlan { hq: QUARTIER_GENERAL, supply: HABITAT, supply_at: pair, worker: TECHNICIEN })
            }
            Faction::Sylves => {
                use sylves::{building::*, kind::*};
                Some(StartPlan { hq: ARBRE_MERE, supply: SOUCHE, supply_at: pair, worker: POUSSE })
            }
            Faction::Collectif => {
                use collectif::{building::*, kind::*};
                let supply_at = vec![Vec2::new(-8.5, -8.5), Vec2::new(8.5, -8.5), Vec2::new(14.0, -12.0)];
                Some(StartPlan { hq: ECRIN, supply: RESONATEUR, supply_at, worker: COLLECTEUR })
            }
            Faction::CollectifOrRose => None,
        }
    }

    fn ambiance(self) -> Ambiance {
        match self {
            Faction::Garde => garde::ambiance(),
            Faction::Sylves => sylves::ambiance(),
            Faction::Collectif => collectif::ambiance(),
            Faction::CollectifOrRose => collectif_or_rose::ambiance(),
        }
    }

    /// Panneaux de reflet en plus de l'ambiance (vitrine de bijoutier du Collectif).
    fn reflection_panels(self) -> Option<ReflectionPanels> {
        match self {
            Faction::Garde | Faction::Sylves => None,
            Faction::Collectif => Some(collectif::reflection_panels()),
            Faction::CollectifOrRose => Some(collectif_or_rose::reflection_panels()),
        }
    }

    fn ground(self) -> GroundStyle {
        match self {
            Faction::Garde => GroundStyle::rock(),
            Faction::Sylves => sylves::ground(),
            Faction::Collectif => collectif::ground(),
            Faction::CollectifOrRose => collectif_or_rose::ground(),
        }
    }

    fn ground_pattern(self) -> GroundPattern {
        match self {
            Faction::Garde | Faction::Sylves => GroundPattern::Natural,
            Faction::Collectif => collectif::ground_pattern(),
            Faction::CollectifOrRose => collectif_or_rose::ground_pattern(),
        }
    }

    fn palette(self, team_color: Color) -> Palette {
        match self {
            Faction::Garde => garde::palette(team_color),
            Faction::Sylves => sylves::palette(team_color),
            Faction::Collectif => collectif::palette(team_color),
            Faction::CollectifOrRose => collectif_or_rose::palette(team_color),
        }
    }

    /// Les fiches de combat (PV, armure, armes) des unités et des bâtiments.
    fn combat_rules(self) -> CombatRules {
        match self {
            Faction::Garde => combat::garde(),
            Faction::Sylves => combat::sylves(),
            Faction::Collectif | Faction::CollectifOrRose => combat::collectif(),
        }
    }

    fn unit_types(self) -> Vec<UnitType> {
        match self {
            Faction::Garde => garde::unit_types(),
            Faction::Sylves => sylves::unit_types(),
            Faction::Collectif => collectif::unit_types(),
            Faction::CollectifOrRose => collectif_or_rose::unit_types(),
        }
    }

    /// L'armée de démonstration : (type d'unité, position au sol).
    fn army(self) -> Vec<(usize, Vec2)> {
        match self {
            Faction::Garde => {
                use garde::kind::*;
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

/// La faction de l'adversaire (`OPENRTS_ENEMY`).
#[derive(Resource, Debug, Clone, Copy)]
struct EnemyFaction(Faction);

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
    let enemy = std::env::var("OPENRTS_ENEMY").ok().and_then(|f| Faction::parse(&f)).unwrap_or(faction.default_enemy());

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
    .insert_resource(EnemyFaction(enemy))
    .insert_resource(faction.ambiance())
    .add_systems(Startup, (setup_world, spawn_units))
    .add_systems(Update, base::focus_camera);

    // `OPENRTS_POP_MAX=100` : plafond global de population (200 par défaut),
    // pour adapter le nombre d'unités à l'appareil.
    if let Some(max) = std::env::var("OPENRTS_POP_MAX").ok().and_then(|m| m.parse().ok()) {
        app.insert_resource(PopulationSettings { max });
    }
    if let Some(panels) = faction.reflection_panels() {
        app.insert_resource(panels);
    }
    if let Some(path) = screenshot {
        app.insert_resource(AutoScreenshot { path, delay_secs: 14.0 })
            .add_plugins((FrameTimeDiagnosticsPlugin::default(), LogDiagnosticsPlugin::default()));
    }
    if battle::requested() {
        // La bataille fait elle-même son test de charge (300 contre 300).
        app.add_plugins(battle::BattlePlugin);
    } else if stress {
        app.add_systems(Startup, |mut actions: MessageWriter<GameAction>| {
            actions.write(GameAction::SpawnStressTest);
        });
    }
    match (faction, base::demo_requested()) {
        (Faction::Garde, Some(base::Demo::Buildings)) => {
            app.add_plugins(base::DemoPlugin);
        }
        (Faction::Sylves, Some(base::Demo::Buildings)) => {
            app.add_plugins(base_sylves::DemoPlugin);
        }
        (Faction::Collectif, Some(base::Demo::Buildings)) => {
            app.add_plugins(base_collectif::DemoPlugin);
        }
        _ => {}
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
    // Partie normale : la caméra regarde la base du joueur et son armée.
    if base::demo_requested().is_none() && !battle::requested() {
        let focus = side::match_focus();
        rts_camera.focus = Vec3::new(focus.x, 0.0, focus.y);
    }
    commands.spawn((Camera3d::default(), rts_camera.transform(), rts_camera));
}

fn spawn_units(
    mut commands: Commands,
    faction: Res<Faction>,
    enemy: Res<EnemyFaction>,
    bounds: Res<MapBounds>,
    nav: Res<NavSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Les catalogues communs : la faction du joueur est la faction 0,
    // celle de l'adversaire la 1 (si elle est différente). La palette donne
    // l'orange signal à la couleur d'équipe ; `set_team_colors` donne
    // ensuite à chaque joueur la sienne, dans toutes les factions.
    let mut units = UnitCatalog::new(faction.unit_types(), &faction.palette(garde::SIGNAL_ORANGE), &mut meshes, &mut materials, &mut images);
    let enemy_id = if enemy.0 == *faction {
        0
    } else {
        units.add_faction(enemy.0.unit_types(), &enemy.0.palette(garde::SIGNAL_ORANGE), &mut meshes, &mut materials, &mut images)
    };
    units.set_team_colors(&TEAM_COLORS, &mut materials, &mut images);
    let mut sides = [(HUMAN, *faction, 0), (ENEMY, enemy.0, enemy_id)].map(|(player, faction, id)| Side {
        player,
        faction,
        faction_id: id,
        first_unit: units.first_type(id),
        first_building: 0,
        mirrored: player != HUMAN,
    });

    // Bâtiments et règles, une fois par faction présente.
    let mut buildings = BuildingCatalog::empty(&units, &mut meshes, &mut materials);
    let mut economy: Option<EconomyRules> = None;
    let mut combat = CombatRules::default();
    let distinct = if enemy_id == 0 { 1 } else { 2 };
    for i in 0..distinct {
        let side = sides[i];
        let first_building = side.faction.add_buildings(&mut buildings, side.faction_id, &units, &mut meshes).unwrap_or(buildings.types.len());
        combat.merge(side.faction.combat_rules(), side.first_unit, first_building);
        if let Some(rules) = side.faction.economy_rules() {
            // Les premières règles servent de base (ressources communes).
            let base = economy.get_or_insert_with(|| EconomyRules { units: Vec::new(), harvesters: Vec::new(), ..rules.clone() });
            base.merge(rules, side.first_unit);
        }
        for side in sides.iter_mut().filter(|s| s.faction_id == i) {
            side.first_building = first_building;
        }
    }
    // Les champs de résonance du Collectif, dessinés au sol pendant la pose.
    if sides.iter().any(|s| s.faction == Faction::Collectif) {
        buildings.set_construction_field(collectif::FIELD_COLOR, &mut meshes, &mut materials);
    }
    let has_buildings = !buildings.types.is_empty();
    let mut players = Players::default();
    for side in &sides {
        let state = players.get_mut(side.player);
        state.faction = side.faction_id;
        // 50 de ressource principale au départ (valeur proposée).
        state.stock = Cost::new(50, 0);
    }
    let frame = GridFrame::new(bounds.0, &nav);
    let deposits = gisements::DepositVisuals::new(&mut meshes, &mut materials);

    match (battle::requested(), base::demo_requested()) {
        // `OPENRTS_DEMO=battle` : deux armées face à face (voir `battle`).
        (true, _) => {
            let stress = std::env::var_os("OPENRTS_STRESS").is_some();
            battle::spawn(&mut commands, sides, &units, has_buildings.then_some(&buildings), frame, stress);
        }
        // Scènes de démonstration : le joueur seul, sa faction en vitrine.
        (false, Some(demo)) => {
            let stock = match (*faction, demo) {
                (Faction::Garde, base::Demo::Buildings) => {
                    base::spawn_demo(&mut commands, &buildings, &units, frame);
                    50
                }
                (Faction::Garde, base::Demo::Economy) => {
                    base::spawn_economy_demo(&mut commands, &buildings, &units, &deposits, frame);
                    400
                }
                // De quoi payer le bâtiment que l'ouvrier de la scène part poser.
                (Faction::Sylves, _) => {
                    base_sylves::spawn_demo(&mut commands, &buildings, &units, &deposits, frame);
                    300
                }
                (Faction::Collectif, _) => {
                    base_collectif::spawn_demo(&mut commands, &buildings, &units, &deposits, frame);
                    300
                }
                (Faction::CollectifOrRose, _) => 50,
            };
            players.get_mut(HUMAN).stock = Cost::new(stock, 0);
        }
        // Partie normale : chaque camp, sa base et son armée.
        (false, None) => {
            for side in sides {
                side::spawn_start(&mut commands, side, &buildings, &units, &deposits, frame);
            }
        }
    }

    if has_buildings {
        commands.insert_resource(buildings);
    }
    if let Some(economy) = economy {
        commands.insert_resource(economy);
    }
    commands.insert_resource(combat);
    commands.insert_resource(players);
    commands.insert_resource(units);
}
