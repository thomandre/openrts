//! Tests du brouillard de guerre, sans fenêtre : d'abord le calcul seul
//! (fonction [`FogOfWar::update`]), puis dans une simulation minimale avec le combat.

use super::*;
use crate::buildings::BuildingsPlugin;
use crate::collision::{Collider, CollisionLayer};
use crate::combat::{Category, CombatOrder, CombatPlugin, CombatStats, Weapon};
use crate::buildings::Health;
use crate::economy::EconomyPlugin;
use crate::input::{GameAction, InputContext};
use crate::orders::{CommandQueue, OrdersPlugin, PlayerCommand};
use crate::players::PlayersPlugin;
use crate::selection::Selectable;
use crate::simulation::{MoveSpeed, SimulationPlugin};
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

const ME: PlayerId = PlayerId(0);
const THEM: PlayerId = PlayerId(1);

/// Une source de vision ponctuelle.
fn eye(player: PlayerId, at: Vec2, radius: f32) -> VisionSource {
    VisionSource { player, area: Rect::from_center_size(at, Vec2::ZERO), radius, level: None }
}

fn small_map() -> FogOfWar {
    FogOfWar::new(Rect::new(-20.0, -20.0, 20.0, 20.0))
}

#[test]
fn cells_go_from_unexplored_to_visible_then_explored() {
    let mut fog = small_map();
    assert_eq!(fog.at(ME, Vec2::ZERO), CellVision::Unexplored);
    fog.update(&[eye(ME, Vec2::new(0.5, 0.5), 3.0)], &[]);
    // Un disque de 3 m autour de la source, pas au-delà.
    assert!(fog.is_visible(ME, Vec2::new(2.5, 0.5)));
    assert!(fog.is_visible(ME, Vec2::new(0.5, -2.4)));
    assert!(!fog.is_visible(ME, Vec2::new(4.1, 0.5)));
    assert!(!fog.is_visible(ME, Vec2::new(3.0, 3.0)), "le coin du carré n'est pas dans le disque");
    // L'autre joueur ne voit rien.
    assert!(!fog.is_visible(THEM, Vec2::new(0.5, 0.5)));
    // La source s'en va : ce qu'elle voyait devient « déjà vu ».
    fog.update(&[], &[]);
    assert_eq!(fog.at(ME, Vec2::new(0.5, 0.5)), CellVision::Explored);
    assert_eq!(fog.at(ME, Vec2::new(10.0, 10.0)), CellVision::Unexplored);
    assert!(fog.is_explored(ME, Vec2::new(0.5, 0.5)));
    // Hors de la carte : jamais vu, sans plantage.
    assert_eq!(fog.at(ME, Vec2::new(100.0, 0.0)), CellVision::Unexplored);
}

#[test]
fn nobody_sees_higher_than_their_level() {
    use crate::terrain::{Plateau, Terrain, DEFAULT_LEVEL_HEIGHT};
    let bounds = Rect::new(-20.0, -20.0, 20.0, 20.0);
    // Un plateau de niveau 1 au nord (z > 0).
    let plateau = Plateau::new(1, [Vec2::new(-30.0, 0.0), Vec2::new(30.0, 0.0), Vec2::new(30.0, 30.0), Vec2::new(-30.0, 30.0)]);
    let terrain = Terrain::new(bounds, DEFAULT_LEVEL_HEIGHT, vec![plateau], Vec::new());
    let mut fog = small_map();
    fog.set_terrain(&terrain);
    let low = VisionSource { level: Some(0), ..eye(ME, Vec2::new(0.5, -1.5), 6.0) };
    let high = VisionSource { level: Some(1), ..eye(THEM, Vec2::new(0.5, 1.5), 6.0) };
    fog.update(&[low, high], &[]);
    // En bas, on ne voit pas le haut ; en haut, on voit le bas.
    assert!(fog.is_visible(ME, Vec2::new(0.5, -4.5)));
    assert!(!fog.is_visible(ME, Vec2::new(0.5, 0.5)));
    assert!(fog.is_visible(THEM, Vec2::new(0.5, -2.5)));
    // Un volant voit tout.
    fog.update(&[eye(ME, Vec2::new(0.5, -1.5), 6.0)], &[]);
    assert!(fog.is_visible(ME, Vec2::new(0.5, 2.5)));
}

#[test]
fn a_building_sees_around_its_whole_footprint() {
    let mut fog = small_map();
    let rect = Rect::new(-2.0, -2.0, 2.0, 2.0);
    fog.update(&[VisionSource { player: ME, area: rect, radius: 3.0, level: None }], &[]);
    assert!(fog.is_visible(ME, Vec2::new(4.5, 1.5)), "3 m à droite du bord");
    assert!(fog.is_visible(ME, Vec2::new(-1.5, -4.5)), "3 m sous le bord");
    assert!(!fog.is_visible(ME, Vec2::new(5.5, 0.0)));
    // Un rectangle est visible dès qu'une de ses cases l'est.
    assert!(fog.is_rect_visible(ME, Rect::new(4.0, 4.0, 8.0, 8.0)) == fog.is_visible(ME, Vec2::new(4.5, 4.5)));
    assert!(fog.is_rect_visible(ME, Rect::new(4.0, -1.0, 9.0, 1.0)));
    assert!(!fog.is_rect_visible(ME, Rect::new(8.0, 8.0, 12.0, 12.0)));
}

#[test]
fn enemy_buildings_are_remembered_until_seen_gone() {
    let mut fog = small_map();
    let base = Entity::from_raw_u32(7).unwrap();
    let rect = Rect::new(8.0, 8.0, 12.0, 12.0);
    let building = RememberedBuilding { owner: THEM, type_index: 3, rect, progress: Some(0.5) };
    let near = eye(ME, Vec2::new(6.0, 6.0), 5.0);
    // Aperçu à moitié construit.
    fog.update(&[near], &[(base, building)]);
    assert_eq!(fog.remembers(ME, base), Some(&building));
    assert!(fog.remembers(THEM, base).is_none(), "on ne se souvient pas de ses propres bâtiments");
    // Hors de vue, il s'achève : le souvenir reste tel qu'au dernier aperçu.
    let finished = RememberedBuilding { progress: None, ..building };
    fog.update(&[], &[(base, finished)]);
    assert_eq!(fog.remembers(ME, base).map(|b| b.progress), Some(Some(0.5)));
    // Détruit hors de vue : on s'en souvient toujours.
    fog.update(&[], &[]);
    assert!(fog.remembers(ME, base).is_some());
    assert_eq!(fog.remembered(ME).count(), 1);
    // On revoit l'emplacement, vide : oublié.
    fog.update(&[near], &[]);
    assert!(fog.remembers(ME, base).is_none());
}

#[test]
fn disabled_fog_shows_everything() {
    let mut fog = small_map();
    fog.update(&[], &[]);
    fog.set_enabled(false);
    assert!(fog.is_visible(ME, Vec2::new(10.0, 10.0)));
    assert!(fog.sees_entity(ME, THEM, Vec2::new(10.0, 10.0), None));
    assert_eq!(fog.seen_mask(Vec2::ZERO, None), u32::MAX);
}

#[test]
fn same_sources_in_any_order_give_the_same_grid() {
    // `0..40u32` : en `u8` (le type de `PlayerId`), `i * 7` déborderait.
    let sources: Vec<VisionSource> = (0..40u32).map(|i| eye(PlayerId((i % 2) as u8), Vec2::new(i as f32 - 20.0, (i * 7 % 30) as f32 - 15.0), 2.0 + (i % 5) as f32)).collect();
    let mut a = small_map();
    a.update(&sources, &[]);
    let mut reversed = sources.clone();
    reversed.reverse();
    let mut b = small_map();
    b.update(&reversed, &[]);
    assert_eq!(a.cells(ME), b.cells(ME));
    assert_eq!(a.cells(THEM), b.cells(THEM));
}

// ---------------------------------------------------------------------------
// Dans la simulation.
// ---------------------------------------------------------------------------

const SNIPER: usize = 0;
const DUMMY: usize = 1;

/// Simulation sans fenêtre, avec le combat et le brouillard. Le tireur
/// d'élite porte à 9 m mais ne voit qu'à 4 m : il lui faut un éclaireur.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<GameAction>()
        .init_resource::<InputContext>()
        .init_resource::<crate::cursor::Pointer>()
        .add_plugins((SimulationPlugin, PlayersPlugin, OrdersPlugin, EconomyPlugin, BuildingsPlugin, CombatPlugin, VisionPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 32.0)))
        .insert_resource(CombatRules {
            units: vec![
                CombatStats::unit(20.0, 0.0, &[Category::Infantry], vec![Weapon::new(6.0, 9.0, 1.0, (true, false))]).with_sight(4.0),
                CombatStats::unit(50.0, 0.0, &[Category::Infantry], Vec::new()).with_sight(6.0),
            ],
            buildings: Vec::new(),
        });
    for _ in 0..3 {
        app.update();
    }
    app
}

fn spawn(app: &mut App, player: PlayerId, type_index: usize, at: Vec2) -> Entity {
    app.world_mut()
        .spawn((
            Unit { type_index },
            Owner(player),
            SimPosition::new(at),
            MoveSpeed(4.0),
            Collider { radius: 0.5, mass: 1.0, layer: CollisionLayer::Ground },
            Selectable { radius: 0.7, half_extents: None },
        ))
        .id()
}

fn run(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 32.0).round() as usize {
        app.update();
    }
}

#[test]
fn units_only_shoot_what_their_player_sees() {
    let mut app = app();
    let sniper = spawn(&mut app, ME, SNIPER, Vec2::new(0.0, 0.0));
    app.world_mut().entity_mut(sniper).insert(CombatOrder::Hold);
    let target = spawn(&mut app, THEM, DUMMY, Vec2::new(8.0, 0.0));
    run(&mut app, 2.0);
    assert_eq!(app.world().get::<Health>(target).unwrap().current, 50.0, "cible à portée mais invisible : pas de tir");
    assert!(!app.world().resource::<FogOfWar>().is_visible(ME, Vec2::new(8.0, 0.0)));
    // Un éclaireur s'approche : la cible devient visible, le tireur tire.
    spawn(&mut app, ME, DUMMY, Vec2::new(4.0, 2.0));
    run(&mut app, 1.0);
    assert!(app.world().get::<Health>(target).unwrap().current < 50.0, "cible révélée par l'éclaireur");
}

#[test]
fn attack_orders_need_a_visible_target() {
    let mut app = app();
    let sniper = spawn(&mut app, ME, SNIPER, Vec2::new(-10.0, 0.0));
    let hidden = spawn(&mut app, THEM, DUMMY, Vec2::new(10.0, 0.0));
    run(&mut app, 0.2);
    app.world_mut().resource_mut::<CommandQueue>().push(ME, PlayerCommand::Attack { units: vec![sniper], target: hidden });
    run(&mut app, 0.2);
    assert!(app.world().get::<CombatOrder>(sniper).is_none(), "on ne vise pas ce qu'on ne voit pas");
    // Avec le brouillard coupé, le même ordre est accepté.
    app.world_mut().resource_mut::<VisionSettings>().enabled = false;
    run(&mut app, 0.2);
    app.world_mut().resource_mut::<CommandQueue>().push(ME, PlayerCommand::Attack { units: vec![sniper], target: hidden });
    run(&mut app, 0.2);
    assert_eq!(app.world().get::<CombatOrder>(sniper), Some(&CombatOrder::Attack(hidden)));
}

#[test]
fn a_target_lost_in_the_fog_is_searched_where_it_vanished() {
    let mut app = app();
    let sniper = spawn(&mut app, ME, SNIPER, Vec2::new(0.0, 0.0));
    let target = spawn(&mut app, THEM, DUMMY, Vec2::new(3.0, 0.0));
    run(&mut app, 0.2);
    app.world_mut().resource_mut::<CommandQueue>().push(ME, PlayerCommand::Attack { units: vec![sniper], target });
    run(&mut app, 0.2);
    assert_eq!(app.world().get::<CombatOrder>(sniper), Some(&CombatOrder::Attack(target)));
    // La cible s'éloigne d'un bond hors de vue (on la déplace à la main).
    app.world_mut().get_mut::<SimPosition>(target).unwrap().current = Vec2::new(15.0, 0.0);
    run(&mut app, 0.2);
    match app.world().get::<CombatOrder>(sniper) {
        Some(CombatOrder::AttackMove { point, .. }) => assert!(point.distance(Vec2::new(15.0, 0.0)) < 1.0, "va voir là où elle a disparu : {point}"),
        other => panic!("attendu une attaque-déplacement, obtenu {other:?}"),
    }
}

/// Coût du calcul de la vision avec 300 unités par joueur, en mode optimisé :
/// `cargo test --release -p rts_engine -- --ignored --nocapture vision_cost`.
#[test]
#[ignore]
fn vision_cost_300_against_300() {
    let mut fog = FogOfWar::new(Rect::new(-50.0, -50.0, 50.0, 50.0));
    let side = 20;
    let mut sources = Vec::new();
    for i in 0..300 {
        let (x, z) = ((i % side) as f32 * 1.6 - 16.0, (i / side) as f32 * 1.6 - 12.0);
        // Rayons de vision de 8 à 12 m, plus le rayon de l'unité.
        let radius = 8.5 + (i % 5) as f32;
        sources.push(eye(ME, Vec2::new(z - 22.0, x), radius));
        sources.push(eye(THEM, Vec2::new(22.0 - z, x), radius));
    }
    // Une vingtaine de bâtiments par joueur.
    let mut buildings = Vec::new();
    for i in 0..40u32 {
        let at = Vec2::new((i % 10) as f32 * 8.0 - 40.0, if i < 20 { -40.0 } else { 40.0 } + (i % 2) as f32 * 5.0);
        let owner = if i < 20 { ME } else { THEM };
        let building = RememberedBuilding { owner, type_index: 0, rect: Rect::from_center_size(at, Vec2::splat(4.0)), progress: None };
        buildings.push((Entity::from_raw_u32(1000 + i).unwrap(), building));
        sources.push(VisionSource { player: owner, area: building.rect, radius: 8.0, level: None });
    }
    let runs = 2000;
    let start = std::time::Instant::now();
    for _ in 0..runs {
        fog.update(&sources, &buildings);
    }
    let per_update = start.elapsed().as_secs_f64() * 1000.0 / runs as f64;
    println!(
        "Vision, 600 unités et 40 bâtiments : {per_update:.3} ms par calcul, soit {:.3} ms par pas (un calcul tous les {UPDATE_TICKS} pas)",
        per_update / UPDATE_TICKS as f64
    );
}
