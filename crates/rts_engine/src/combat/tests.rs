//! Tests du combat, sans fenêtre : une simulation minimale, des unités sans
//! modèle, et des fiches de combat écrites pour l'occasion.

use super::*;
use crate::buildings::{BuildingCatalog, BuildingType, BuildingsPlugin, ConstructionStyle};
use crate::collision::CollisionLayer;
use crate::economy::harvest::HarvestPhase;
use crate::economy::production::UnitEconomy;
use crate::economy::{spawn_resource_node, Cost, EconomyPlugin, EconomyRules, ResourceInfo, ResourceKind, ResourceNode};
use crate::input::{GameAction, InputContext};
use crate::navigation::NavGrid;
use crate::orders::{CommandQueue, OrdersPlugin};
use crate::players::PlayersPlugin;
use crate::simulation::SimulationPlugin;
use crate::units::{Palette, SurfaceDetail, SurfaceStyle, UnitCatalog, UnitType};
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

const ME: PlayerId = PlayerId(0);
const THEM: PlayerId = PlayerId(1);

/// Types d'unités des tests (indices des fiches de combat).
const SOLDIER: usize = 0;
const TANK: usize = 1;
const LANCER: usize = 2;
const WORKER: usize = 3;
const DUMMY: usize = 4;
const SNIPER: usize = 5;
const FLYER: usize = 6;

fn rules() -> CombatRules {
    CombatRules {
        units: vec![
            // Soldat : 50 PV, 10 dégâts à 4 m, un coup par seconde.
            CombatStats::unit(50.0, 0.0, &[Category::Infantry], vec![Weapon::new(10.0, 4.0, 1.0, (true, false))]),
            // Blindé : 100 PV, 2 d'armure, 5 dégâts.
            CombatStats::unit(100.0, 2.0, &[Category::Armored], vec![Weapon::new(5.0, 4.0, 1.0, (true, false))]),
            // Lancier : bonus de 20 contre les blindés.
            CombatStats::unit(50.0, 0.0, &[Category::Infantry], vec![Weapon::new(10.0, 4.0, 1.0, (true, true)).with_bonus(Category::Armored, 20.0)]),
            // Ouvrier.
            CombatStats::unit(30.0, 0.0, &[Category::Worker], vec![Weapon::new(3.0, 0.3, 1.5, (true, false))]),
            // Mannequin sans arme.
            CombatStats::unit(20.0, 0.0, &[Category::Infantry], Vec::new()),
            // Tireur d'élite : longue portée, fragile.
            CombatStats::unit(20.0, 0.0, &[Category::Infantry], vec![Weapon::new(6.0, 9.0, 1.0, (true, false))]),
            // Volant.
            CombatStats::unit(40.0, 0.0, &[Category::Air], Vec::new()),
        ],
        buildings: vec![CombatStats::structure(0.0, &[], Vec::new())],
    }
}

fn palette() -> Palette {
    let plain = SurfaceStyle::new(Color::WHITE, 0.0, 0.5, SurfaceDetail::Smooth);
    Palette {
        paint: plain,
        dark: plain,
        steel: plain,
        bare: plain,
        rubber: plain,
        team: plain,
        glow: SurfaceStyle::glowing(Color::WHITE, 2.0),
        engine: SurfaceStyle::glowing(Color::WHITE, 2.0),
        selection: LinearRgba::WHITE,
    }
}

/// Une simulation complète sans fenêtre (navigation, économie, bâtiments,
/// combat). Un bâtiment de test : un abri de 3 × 3 m qui fournit 5 de population.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<GameAction>()
        .init_resource::<InputContext>()
        .init_resource::<crate::cursor::Pointer>()
        .add_plugins((SimulationPlugin, PlayersPlugin, OrdersPlugin, EconomyPlugin, BuildingsPlugin, CombatPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 32.0)))
        .insert_resource(rules())
        .insert_resource(EconomyRules {
            resources: [
                ResourceInfo { name: "P".into(), color: Color::WHITE },
                ResourceInfo { name: "R".into(), color: Color::WHITE },
            ],
            harvesters: vec![WORKER],
            carry: [5, 4],
            gather_time: [2.0, 1.5],
            units: vec![UnitEconomy { cost: Cost::new(50, 0), train_time: 1.0, population: 1 }; 7],
        });
    let (mut meshes, mut materials, mut images) = (Assets::default(), Assets::default(), Assets::default());
    let types = (0..7)
        .map(|i| UnitType { name: format!("Type {i}"), parts: Vec::new(), footprint_radius: 0.5, mass: 1.0, speed: 4.0, hover: None })
        .collect();
    let units = UnitCatalog::new(types, &palette(), &mut meshes, &mut materials, &mut images);
    let shelter = BuildingType { name: "Abri".into(), footprint: UVec2::new(3, 3), max_health: 100.0, supply: 5, ..default() };
    let catalog = BuildingCatalog::new(vec![shelter], vec![WORKER], ConstructionStyle::WorkerStays, &units, &mut meshes, &mut materials);
    app.insert_resource(catalog).insert_resource(units);
    for _ in 0..3 {
        app.update();
    }
    app
}

fn spawn(app: &mut App, player: PlayerId, type_index: usize, at: Vec2) -> Entity {
    let layer = if type_index == FLYER { CollisionLayer::Air } else { CollisionLayer::Ground };
    app.world_mut()
        .spawn((
            Unit { type_index },
            Owner(player),
            SimPosition::new(at),
            MoveSpeed(4.0),
            Collider { radius: 0.5, mass: 1.0, layer },
            Selectable { radius: 0.7, half_extents: None },
        ))
        .id()
}

fn order(app: &mut App, player: PlayerId, command: PlayerCommand) {
    app.world_mut().resource_mut::<CommandQueue>().push(player, command);
}

fn run(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 32.0).round() as usize {
        app.update();
    }
}

fn alive(app: &App, entity: Entity) -> bool {
    app.world().get_entity(entity).is_ok()
}

fn health(app: &App, entity: Entity) -> f32 {
    app.world().get::<Health>(entity).unwrap().current
}

fn position(app: &App, entity: Entity) -> Vec2 {
    app.world().get::<SimPosition>(entity).unwrap().current
}

#[test]
fn damage_bonus_and_armor() {
    let lance = Weapon::new(10.0, 6.0, 1.5, (true, true)).with_bonus(Category::Armored, 25.0);
    let armored = Categories::of(&[Category::Armored, Category::Ranged]);
    let infantry = Categories::of(&[Category::Infantry]);
    assert_eq!(lance.damage_against(armored, 2.0), 33.0, "10 + 25 de bonus − 2 d'armure");
    assert_eq!(lance.damage_against(infantry, 1.0), 9.0, "pas de bonus contre l'infanterie");
    assert_eq!(Weapon::new(1.0, 1.0, 1.0, (true, false)).damage_against(infantry, 5.0), MIN_DAMAGE, "jamais moins de 0,5");
    assert!(!Weapon::new(1.0, 1.0, 1.0, (true, false)).can_hit(true), "une arme au sol ne touche pas l'air");
}

#[test]
fn two_units_fight_to_the_death() {
    let mut app = app();
    // À 3 m l'une de l'autre : elles se voient et sont déjà à portée.
    let soldier = spawn(&mut app, ME, SOLDIER, Vec2::new(0.0, 0.0));
    let tank = spawn(&mut app, THEM, TANK, Vec2::new(3.0, 0.0));
    run(&mut app, 0.5);
    assert!(health(&app, soldier) < 50.0 && health(&app, tank) < 100.0, "elles se tirent dessus d'elles-mêmes");
    run(&mut app, 12.0);
    // Le blindé fait 5 par coup (le soldat meurt au 10e) ; le soldat lui fait
    // 10 − 2 d'armure = 8 par coup, et a eu le temps d'en tirer 10.
    assert!(!alive(&app, soldier), "le soldat est mort");
    assert!(alive(&app, tank), "le blindé a survécu");
    assert_eq!(health(&app, tank), 100.0 - 10.0 * 8.0);
    // Plus d'ennemi : le blindé n'a plus de cible.
    assert_eq!(app.world().get::<Attacker>(tank).unwrap().target, None);
}

#[test]
fn bonus_damage_counters_armor() {
    let mut app = app();
    let lancer = spawn(&mut app, ME, LANCER, Vec2::new(0.0, 0.0));
    let tank = spawn(&mut app, THEM, TANK, Vec2::new(3.0, 0.0));
    run(&mut app, 0.1);
    // Premier coup : 10 + 20 − 2 = 28.
    assert_eq!(health(&app, tank), 72.0);
    run(&mut app, 4.0);
    assert!(!alive(&app, tank), "quatre coups de 28 suffisent");
    assert!(alive(&app, lancer));
}

#[test]
fn an_attacked_unit_always_fights_back_then_returns() {
    let mut app = app();
    let soldier = spawn(&mut app, ME, SOLDIER, Vec2::new(0.0, 0.0));
    // Le tireur est hors de la portée d'acquisition du soldat (4 + 3 = 7 m),
    // mais le soldat est dans la sienne.
    let sniper = spawn(&mut app, THEM, SNIPER, Vec2::new(9.5, 0.0));
    run(&mut app, 0.5);
    assert!(health(&app, soldier) < 50.0, "le tireur a ouvert le feu");
    assert_eq!(app.world().get::<Attacker>(soldier).unwrap().target, Some(sniper), "le soldat riposte");
    run(&mut app, 6.0);
    assert!(!alive(&app, sniper), "le soldat a rattrapé et abattu le tireur");
    // Laisse : il revient à sa position de départ.
    run(&mut app, 4.0);
    assert!(position(&app, soldier).distance(Vec2::ZERO) < 1.0, "revenu à {}", position(&app, soldier));
}

#[test]
fn attack_move_fights_on_the_way_but_a_plain_move_does_not() {
    for attack in [true, false] {
        let mut app = app();
        let soldier = spawn(&mut app, ME, SOLDIER, Vec2::new(-20.0, 0.0));
        // Un ennemi sans arme sur la route.
        let dummy = spawn(&mut app, THEM, DUMMY, Vec2::new(0.0, 2.5));
        let target = Vec2::new(20.0, 0.0);
        let command = if attack {
            PlayerCommand::AttackMove { units: vec![soldier], target }
        } else {
            PlayerCommand::Move { units: vec![soldier], target }
        };
        order(&mut app, ME, command);
        run(&mut app, 16.0);
        assert_eq!(!alive(&app, dummy), attack, "l'ennemi meurt seulement en attaque-déplacement");
        // Dans les deux cas, le soldat est arrivé, et son ordre est terminé.
        assert!(position(&app, soldier).distance(target) < 1.0, "arrivé à {}", position(&app, soldier));
        assert!(app.world().get::<CombatOrder>(soldier).is_none());
    }
}

#[test]
fn units_that_attack_are_targeted_first() {
    let mut app = app();
    let soldier = spawn(&mut app, ME, SOLDIER, Vec2::new(0.0, 0.0));
    let ally = spawn(&mut app, ME, DUMMY, Vec2::new(0.0, -3.5));
    // Tout près du soldat, un ennemi inoffensif ; un peu plus loin, un
    // ennemi armé qui tire sur l'allié sans arme.
    let harmless = spawn(&mut app, THEM, DUMMY, Vec2::new(2.0, 0.0));
    let shooter = spawn(&mut app, THEM, SOLDIER, Vec2::new(-4.0, -4.5));
    run(&mut app, 0.1);
    assert_eq!(app.world().get::<Attacker>(shooter).unwrap().target, Some(ally));
    run(&mut app, 0.5);
    assert_eq!(app.world().get::<Attacker>(soldier).unwrap().target, Some(shooter), "ce qui attaque d'abord");
    assert!(alive(&app, harmless));
}

#[test]
fn explicit_attack_stop_and_hold() {
    let mut app = app();
    let soldier = spawn(&mut app, ME, SOLDIER, Vec2::new(0.0, 0.0));
    // Loin : hors de toute portée d'acquisition.
    let far = spawn(&mut app, THEM, DUMMY, Vec2::new(20.0, 0.0));
    order(&mut app, ME, PlayerCommand::Attack { units: vec![soldier], target: far });
    run(&mut app, 7.0);
    assert!(!alive(&app, far), "poursuivi et détruit");
    assert!(app.world().get::<CombatOrder>(soldier).is_none(), "ordre terminé");
    // Tenir la position : il ne bouge plus, même pour un ennemi tout proche hors de portée.
    let here = position(&app, soldier);
    order(&mut app, ME, PlayerCommand::HoldPosition { units: vec![soldier] });
    let near = spawn(&mut app, THEM, DUMMY, here + Vec2::new(6.0, 0.0));
    run(&mut app, 2.0);
    assert!(position(&app, soldier).distance(here) < 0.05);
    assert!(alive(&app, near));
    // Stop : retour au repos ; il repère alors l'ennemi (6 m < 7 m) et l'attaque.
    order(&mut app, ME, PlayerCommand::Stop { units: vec![soldier] });
    run(&mut app, 4.0);
    assert!(!alive(&app, near));
    // On ne peut pas attaquer une unité alliée.
    let friend = spawn(&mut app, ME, DUMMY, here + Vec2::new(2.0, 0.0));
    order(&mut app, ME, PlayerCommand::Attack { units: vec![soldier], target: friend });
    run(&mut app, 2.0);
    assert_eq!(health(&app, friend), 20.0);
    // Ni donner d'ordre aux unités d'un autre joueur.
    let theirs = spawn(&mut app, THEM, SOLDIER, Vec2::new(-30.0, 30.0));
    order(&mut app, ME, PlayerCommand::AttackMove { units: vec![theirs], target: Vec2::ZERO });
    run(&mut app, 0.2);
    assert!(app.world().get::<CombatOrder>(theirs).is_none());
}

#[test]
fn ground_weapons_cannot_hit_flyers() {
    let mut app = app();
    let soldier = spawn(&mut app, ME, SOLDIER, Vec2::new(0.0, 0.0));
    let lancer = spawn(&mut app, ME, LANCER, Vec2::new(0.0, 1.5));
    let flyer = spawn(&mut app, THEM, FLYER, Vec2::new(3.0, 0.0));
    run(&mut app, 0.1);
    assert_eq!(app.world().get::<Attacker>(soldier).unwrap().target, None, "le soldat ne vise pas l'air");
    assert_eq!(app.world().get::<Attacker>(lancer).unwrap().target, Some(flyer), "le lancier, si");
}

/// Deux exécutions de la même bataille donnent exactement le même résultat.
#[test]
fn battles_are_deterministic() {
    fn battle() -> Vec<(u32, u32, u32)> {
        let mut app = app();
        let mut all = Vec::new();
        for i in 0..20 {
            let (x, z) = ((i % 5) as f32 * 1.6, (i / 5) as f32 * 1.6);
            all.push(spawn(&mut app, ME, [SOLDIER, TANK, LANCER, SNIPER][i % 4], Vec2::new(-12.0 + x, z)));
            all.push(spawn(&mut app, THEM, [SOLDIER, LANCER, TANK, SOLDIER][i % 4], Vec2::new(12.0 - x, z)));
        }
        let (mine, theirs): (Vec<Entity>, Vec<Entity>) = all.iter().partition(|e| app.world().get::<Owner>(**e).unwrap().0 == ME);
        order(&mut app, ME, PlayerCommand::AttackMove { units: mine, target: Vec2::new(12.0, 2.0) });
        order(&mut app, THEM, PlayerCommand::AttackMove { units: theirs, target: Vec2::new(-12.0, 2.0) });
        run(&mut app, 12.0);
        // Pour chaque unité : PV et position, en bits (comparaison exacte).
        all.iter()
            .map(|&e| match app.world().get_entity(e) {
                Ok(entity) => {
                    let p = entity.get::<SimPosition>().unwrap().current;
                    (entity.get::<Health>().unwrap().current.to_bits(), p.x.to_bits(), p.y.to_bits())
                }
                Err(_) => (0, 0, 0),
            })
            .collect()
    }
    let first = battle();
    assert!(first.contains(&(0, 0, 0)), "il y a eu des morts");
    assert!(first.iter().any(|s| *s != (0, 0, 0)), "et des survivants");
    assert_eq!(first, battle());
}

#[test]
fn a_worker_killed_while_harvesting_frees_its_node() {
    let mut app = app();
    let frame = app.world().resource::<NavGrid>().frame();
    let node = spawn_resource_node(&mut app.world_mut().commands(), frame, ResourceKind::Primary, IVec2::new(50, 50), UVec2::new(2, 1), 500, false);
    app.world_mut().flush();
    let worker = spawn(&mut app, ME, WORKER, Vec2::new(1.0, -1.0));
    order(&mut app, ME, PlayerCommand::Harvest { units: vec![worker], node });
    run(&mut app, 1.0);
    let harvesting = app.world().get::<Harvesting>(worker).copied().unwrap();
    assert!(matches!(harvesting.phase, HarvestPhase::Gathering { .. }), "il extrait");
    assert_eq!(app.world().get::<ResourceNode>(node).unwrap().occupant, Some(worker));
    // Un ouvrier qui récolte ne se défend pas (il est occupé).
    let killer = spawn(&mut app, THEM, SOLDIER, Vec2::new(1.0, -4.0));
    run(&mut app, 0.5);
    assert!(app.world().get::<Attacker>(worker).unwrap().target.is_none());
    order(&mut app, THEM, PlayerCommand::Attack { units: vec![killer], target: worker });
    run(&mut app, 4.0);
    assert!(!alive(&app, worker));
    // Le gisement est libéré, et un autre ouvrier peut le prendre.
    assert_eq!(app.world().get::<ResourceNode>(node).unwrap().occupant, None);
    let other = spawn(&mut app, ME, WORKER, Vec2::new(-1.0, -1.0));
    order(&mut app, ME, PlayerCommand::Harvest { units: vec![other], node });
    run(&mut app, 1.0);
    assert_eq!(app.world().get::<ResourceNode>(node).unwrap().occupant, Some(other));
    assert_eq!(app.world().resource::<Players>().get(ME).population, 1, "la population de l'ouvrier mort est libérée");
}

#[test]
fn a_destroyed_building_frees_the_grid_and_its_supply() {
    let mut app = app();
    let frame = app.world().resource::<NavGrid>().frame();
    let catalog = app.world_mut().remove_resource::<BuildingCatalog>().unwrap();
    let shelter = catalog.spawn(&mut app.world_mut().commands(), frame, THEM, 0, IVec2::new(60, 49), None);
    app.world_mut().flush();
    app.insert_resource(catalog);
    run(&mut app, 0.2);
    assert_eq!(app.world().resource::<Players>().get(THEM).population_cap, 5);
    assert!(app.world().resource::<NavGrid>().is_blocked(IVec2::new(61, 50)));
    // Quatre soldats l'attaquent : 100 PV, 10 par coup.
    let soldiers: Vec<Entity> = (0..4).map(|i| spawn(&mut app, ME, SOLDIER, Vec2::new(0.0, -3.0 + i as f32 * 1.5))).collect();
    order(&mut app, ME, PlayerCommand::Attack { units: soldiers, target: shelter });
    run(&mut app, 8.0);
    assert!(!alive(&app, shelter), "l'abri est détruit");
    assert!(!app.world().resource::<NavGrid>().is_blocked(IVec2::new(61, 50)), "sa place est libre");
    assert_eq!(app.world().resource::<Players>().get(THEM).population_cap, 0, "sa population aussi");
}

/// Mesure du coût d'un pas de simulation pendant une bataille de 300 contre
/// 300. Ignoré par défaut (lent en mode debug) ; à lancer en mode optimisé :
/// `cargo test --release -p rts_engine -- --ignored --nocapture battle_300`.
#[test]
#[ignore]
fn battle_300_against_300_step_time() {
    battle_300(false);
}

/// La même bataille, avec le brouillard de guerre (voir `crate::vision`).
#[test]
#[ignore]
fn battle_300_against_300_step_time_with_fog() {
    battle_300(true);
}

fn battle_300(fog: bool) {
    let mut app = app();
    if fog {
        app.add_plugins(crate::vision::VisionPlugin);
    }
    let side = 20;
    let mut mine = Vec::new();
    let mut theirs = Vec::new();
    for i in 0..300 {
        let (x, z) = ((i % side) as f32 * 1.6 - 16.0, (i / side) as f32 * 1.6 - 12.0);
        let kind = [SOLDIER, SOLDIER, LANCER, TANK, SNIPER][i % 5];
        mine.push(spawn(&mut app, ME, kind, Vec2::new(z - 22.0, x)));
        theirs.push(spawn(&mut app, THEM, kind, Vec2::new(22.0 - z, x)));
    }
    order(&mut app, ME, PlayerCommand::AttackMove { units: mine, target: Vec2::new(30.0, 0.0) });
    order(&mut app, THEM, PlayerCommand::AttackMove { units: theirs, target: Vec2::new(-30.0, 0.0) });
    let mut samples = Vec::new();
    for _ in 0..32 * 20 {
        let start = std::time::Instant::now();
        app.update();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let world = app.world_mut();
    let survivors = world.query::<&Unit>().iter(world).count();
    let average = samples.iter().sum::<f64>() / samples.len() as f64;
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    println!(
        "300 contre 300{} : {:.2} ms par pas en moyenne, {:.2} ms au 95e centile, {:.2} ms au pire ; {survivors} survivants après 20 s",
        if fog { " avec brouillard" } else { "" },
        average,
        sorted[sorted.len() * 95 / 100],
        sorted[sorted.len() - 1]
    );
}
