//! Production d'unités, population, points de ralliement, annulations.
//!
//! Un bâtiment producteur (dont le type a des [`ProductionOption`]) porte une
//! [`ProductionQueue`] : jusqu'à [`QUEUE_LIMIT`] unités payées d'avance, dont
//! la première avance pas après pas. Une unité terminée apparaît au bord du
//! bâtiment et rejoint son [`RallyPoint`] (sur un gisement, un ouvrier se met
//! directement à récolter).
//!
//! **Population** : chaque unité occupe une part de population (données du
//! jeu, [`UnitEconomy::population`]) ; les bâtiments terminés en fournissent
//! ([`crate::buildings::BuildingType::supply`]), dans la limite d'un plafond
//! global réglable ([`PopulationSettings::max`], pour adapter le nombre
//! d'unités à l'appareil). Comme dans les RTS du genre, une unité réserve sa
//! place dès sa mise en file : on ne peut pas en mettre une de plus que ce
//! que la population permet. Si la limite baisse (bâtiment perdu), la file
//! s'arrête jusqu'à ce qu'il y ait de nouveau de la place.
//!
//! Tout se compte par joueur, dans la simulation à pas fixe.

use bevy::prelude::*;

use super::harvest::{HarvestPhase, Harvesting};
use super::{Cost, EconomyRules, Notice, NoticeKind, ResourceNode};
use crate::buildings::{approach_point, completed_types, Building, BuildingCatalog, UnderConstruction};
use crate::collision::StaticObstacle;
use crate::navigation::NavGrid;
use crate::orders::{PlayerCommand, TickCommands};
use crate::players::{Owner, PlayerId, Players};
use crate::simulation::{MoveTarget, TICKS_PER_SECOND};
use crate::units::{Unit, UnitCatalog};

pub use crate::buildings::{ProductionOption, Requirement};

/// Nombre maximal d'unités dans une file de production.
pub const QUEUE_LIMIT: usize = 5;

/// Ce que coûte une unité, combien de temps elle prend et quelle place de
/// population elle occupe (données du jeu, une par type d'unité).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UnitEconomy {
    pub cost: Cost,
    /// Temps de production, en secondes.
    pub train_time: f32,
    pub population: u32,
}

/// Plafond global de population, quel que soit le nombre de bâtiments :
/// la « limite d'unités réglable » du design (performances, iPad).
#[derive(Resource, Debug, Clone, Copy)]
pub struct PopulationSettings {
    pub max: u32,
}

impl Default for PopulationSettings {
    /// 200 par défaut (valeur proposée).
    fn default() -> Self {
        Self { max: 200 }
    }
}

/// Une unité en file, et ce qui a été payé pour elle (remboursé si annulée).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueuedUnit {
    pub unit_type: usize,
    pub paid: Cost,
}

/// La file de production d'un bâtiment.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct ProductionQueue {
    pub items: Vec<QueuedUnit>,
    /// Pas de simulation déjà passés sur la première unité.
    pub progress_ticks: u32,
}

impl ProductionQueue {
    /// Avancement de la première unité, de 0 à 1.
    pub fn progress(&self, rules: &EconomyRules) -> f32 {
        let Some(first) = self.items.first() else { return 0.0 };
        self.progress_ticks as f32 / train_ticks(rules, first.unit_type) as f32
    }
}

/// Où vont les unités produites par un bâtiment.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct RallyPoint {
    pub point: Vec2,
    /// Un gisement : les ouvriers produits vont le récolter.
    pub node: Option<Entity>,
}

/// Durée de production d'un type d'unité, en pas de simulation (au moins 1).
fn train_ticks(rules: &EconomyRules, unit_type: usize) -> u32 {
    let seconds = rules.unit(unit_type).train_time;
    (seconds * TICKS_PER_SECOND as f32).round().max(1.0) as u32
}

/// Recalcule la population de chaque joueur : unités vivantes et unités en
/// file d'un côté, ce que fournissent ses bâtiments terminés de l'autre.
pub(crate) fn count_population(
    rules: Option<Res<EconomyRules>>,
    catalog: Option<Res<BuildingCatalog>>,
    settings: Res<PopulationSettings>,
    mut players: ResMut<Players>,
    units: Query<(&Unit, &Owner)>,
    buildings: Query<(&Building, &Owner, Has<UnderConstruction>, Option<&ProductionQueue>)>,
) {
    let Some(rules) = rules else { return };
    let mut totals: Vec<(u32, u32)> = vec![(0, 0); players.0.len()];
    let mut add = |player: PlayerId, used: u32, cap: u32| {
        let index = usize::from(player.0);
        if totals.len() <= index {
            totals.resize(index + 1, (0, 0));
        }
        totals[index].0 += used;
        totals[index].1 += cap;
    };
    for (unit, owner) in &units {
        add(owner.0, rules.unit(unit.type_index).population, 0);
    }
    for (building, owner, under_construction, queue) in &buildings {
        let queued: u32 = queue.map_or(0, |q| q.items.iter().map(|i| rules.unit(i.unit_type).population).sum());
        let supply = match (&catalog, under_construction) {
            (Some(catalog), false) => catalog.types.get(building.type_index).map_or(0, |t| t.supply),
            _ => 0,
        };
        add(owner.0, queued, supply);
    }
    for (index, (used, cap)) in totals.into_iter().enumerate() {
        let state = players.get_mut(PlayerId(index as u8));
        state.population = used;
        state.population_cap = cap.min(settings.max);
    }
}

/// Applique les ordres de production, d'annulation et de ralliement.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_production_orders(
    mut commands: Commands,
    tick: Res<TickCommands>,
    rules: Option<Res<EconomyRules>>,
    catalog: Option<Res<BuildingCatalog>>,
    mut players: ResMut<Players>,
    all_buildings: Query<(&Building, &Owner, Has<UnderConstruction>)>,
    mut producers: Query<(&Building, &Owner, &mut ProductionQueue), Without<UnderConstruction>>,
    sites: Query<(&Building, &Owner), With<UnderConstruction>>,
    mut notices: MessageWriter<Notice>,
) {
    let (Some(rules), Some(catalog)) = (rules, catalog) else { return };
    for issued in &tick.0 {
        let player = issued.player;
        let mut refuse = |kind: NoticeKind| {
            notices.write(Notice { player, kind });
        };
        match &issued.command {
            PlayerCommand::Train { building, unit_type } => {
                let completed = completed_types(&catalog, player, all_buildings.iter());
                let Ok((kind, owner, mut queue)) = producers.get_mut(*building) else { continue };
                let Some(option) = catalog.types[kind.type_index].production.iter().find(|o| o.unit_type == *unit_type) else {
                    continue;
                };
                if owner.0 != player {
                    continue;
                }
                let economy = rules.unit(*unit_type);
                let state = players.get_mut(player);
                if !option.requires.iter().all(|r| r.met(&completed)) {
                    refuse(NoticeKind::MissingRequirement);
                } else if queue.items.len() >= QUEUE_LIMIT {
                    refuse(NoticeKind::QueueFull);
                } else if state.population + economy.population > state.population_cap {
                    refuse(NoticeKind::PopulationCap);
                } else if !state.stock.try_pay(economy.cost) {
                    refuse(NoticeKind::NotEnoughResources);
                } else {
                    state.population += economy.population;
                    queue.items.push(QueuedUnit { unit_type: *unit_type, paid: economy.cost });
                }
            }
            PlayerCommand::CancelTrain { building, slot } => {
                let Ok((_, owner, mut queue)) = producers.get_mut(*building) else { continue };
                if owner.0 != player || *slot >= queue.items.len() {
                    continue;
                }
                let item = queue.items.remove(*slot);
                if *slot == 0 {
                    queue.progress_ticks = 0;
                }
                let state = players.get_mut(player);
                state.stock += item.paid;
                state.population = state.population.saturating_sub(rules.unit(item.unit_type).population);
            }
            PlayerCommand::SetRally { buildings, point, node } => {
                for &building in buildings {
                    if producers.get(building).is_ok_and(|(_, owner, _)| owner.0 == player) {
                        commands.entity(building).insert(RallyPoint { point: *point, node: *node });
                    }
                }
            }
            PlayerCommand::CancelConstruction { building } => {
                let Ok((kind, owner)) = sites.get(*building) else { continue };
                if owner.0 != player {
                    continue;
                }
                // Remboursement complet (valeur proposée : à revoir avec le combat).
                players.get_mut(player).stock += catalog.types[kind.type_index].cost;
                commands.entity(*building).despawn();
            }
            _ => {}
        }
    }
}

/// Fait avancer la première unité de chaque file, et fait apparaître les
/// unités terminées.
#[allow(clippy::too_many_arguments)]
pub(crate) fn progress_production(
    mut commands: Commands,
    rules: Option<Res<EconomyRules>>,
    units: Option<Res<UnitCatalog>>,
    grid: Option<Res<NavGrid>>,
    players: Res<Players>,
    nodes: Query<(&ResourceNode, &StaticObstacle)>,
    mut producers: Query<
        (Entity, &Owner, &StaticObstacle, &mut ProductionQueue, Option<&RallyPoint>),
        Without<UnderConstruction>,
    >,
) {
    let (Some(rules), Some(units), Some(grid)) = (rules, units, grid) else { return };
    let mut order: Vec<Entity> = producers.iter().filter(|p| !p.3.items.is_empty()).map(|p| p.0).collect();
    order.sort_unstable();
    for entity in order {
        let Ok((_, owner, obstacle, mut queue, rally)) = producers.get_mut(entity) else { continue };
        let state = players.get(owner.0);
        // Limite dépassée (bâtiment de population perdu) : la file attend.
        if state.population > state.population_cap {
            continue;
        }
        let unit_type = queue.items[0].unit_type;
        queue.progress_ticks += 1;
        if queue.progress_ticks < train_ticks(&rules, unit_type) {
            continue;
        }
        queue.items.remove(0);
        queue.progress_ticks = 0;
        let Some(unit) = units.types.get(unit_type) else { continue };

        // L'unité sort du côté de son point de ralliement (ou devant : +z).
        let rect = obstacle.0;
        let goal = rally.map_or(rect.center() + Vec2::new(0.0, rect.half_size().y + 2.0), |r| r.point);
        let exit = approach_point(&grid, goal, unit.footprint_radius, rect);
        let facing = goal - exit;
        let yaw = if facing.length_squared() > 1e-4 { f32::atan2(-facing.x, -facing.y) } else { std::f32::consts::PI };
        let spawned = units.spawn_owned(&mut commands, owner.0, unit_type, exit, yaw);

        let Some(rally) = rally else { continue };
        let harvest_node = rally.node.filter(|_| rules.harvesters.contains(&unit_type));
        match harvest_node.and_then(|n| nodes.get(n).ok().map(|found| (n, found))) {
            Some((node, (resource, node_obstacle))) => {
                commands.entity(spawned).insert(Harvesting {
                    node,
                    kind: resource.kind,
                    node_position: node_obstacle.0.center(),
                    phase: HarvestPhase::ToNode,
                    carrying: None,
                });
            }
            None => {
                commands.entity(spawned).insert(MoveTarget(rally.point));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingType, BuildingsPlugin, ConstructionStyle};
    use crate::economy::{spawn_resource_node, EconomyPlugin, ResourceInfo, ResourceKind, Stock};
    use crate::input::{GameAction, InputContext};
    use crate::navigation::GridFrame;
    use crate::orders::{CommandQueue, OrdersPlugin};
    use crate::players::PlayersPlugin;
    use crate::simulation::{SimPosition, SimulationPlugin};
    use crate::units::{Palette, SurfaceDetail, SurfaceStyle, UnitType};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const ME: PlayerId = PlayerId(0);

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

    /// Une simulation sans fenêtre : un QG de 4 × 4 m (dépôt, 3 de population,
    /// produit l'ouvrier, type 0, 50 de coût, 1 s, 1 de population) et un
    /// bâtiment de 2 × 2 m à 100 de coût. Le joueur a `stock` en réserve.
    fn app(stock: Stock) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<GameAction>()
            .init_resource::<InputContext>()
            .init_resource::<crate::cursor::Pointer>()
            .add_plugins((SimulationPlugin, PlayersPlugin, OrdersPlugin, EconomyPlugin, BuildingsPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 32.0)))
            .insert_resource(EconomyRules {
                resources: [
                    ResourceInfo { name: "P".into(), color: Color::WHITE },
                    ResourceInfo { name: "R".into(), color: Color::WHITE },
                ],
                harvesters: vec![0],
                carry: [5, 4],
                gather_time: [2.0, 1.5],
                units: vec![UnitEconomy { cost: Cost::new(50, 0), train_time: 1.0, population: 1 }],
            });
        app.world_mut().resource_mut::<Players>().get_mut(ME).stock = stock;
        let (mut meshes, mut materials, mut images) = (Assets::default(), Assets::default(), Assets::default());
        let worker = UnitType { name: "Ouvrier".into(), parts: Vec::new(), footprint_radius: 0.5, mass: 1.0, speed: 3.5, hover: None };
        let units = UnitCatalog::new(vec![worker], &palette(), &mut meshes, &mut materials, &mut images);
        let hq = BuildingType {
            name: "QG".into(),
            footprint: UVec2::new(4, 4),
            dropoff: true,
            supply: 3,
            production: vec![ProductionOption { unit_type: 0, hotkey: 'o', requires: Vec::new() }],
            ..default()
        };
        let hut = BuildingType { name: "Hutte".into(), footprint: UVec2::new(2, 2), cost: Cost::new(100, 0), ..default() };
        let catalog = BuildingCatalog::new(vec![hq, hut], vec![0], ConstructionStyle::WorkerStays, &units, &mut meshes, &mut materials);
        let frame = GridFrame::new(Rect::new(-50.0, -50.0, 50.0, 50.0), &default());
        let hq = catalog.spawn(&mut app.world_mut().commands(), frame, ME, 0, IVec2::new(48, 48), None);
        app.world_mut().flush();
        app.insert_resource(catalog).insert_resource(units);
        for _ in 0..3 {
            app.update();
        }
        (app, hq)
    }

    fn order(app: &mut App, command: PlayerCommand) {
        app.world_mut().resource_mut::<CommandQueue>().push(ME, command);
        app.update();
    }

    fn state(app: &App) -> crate::players::PlayerState {
        app.world().resource::<Players>().get(ME)
    }

    fn workers(app: &mut App) -> Vec<Entity> {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<Unit>>().iter(world).collect()
    }

    fn notices(app: &App) -> Vec<NoticeKind> {
        let messages = app.world().resource::<Messages<Notice>>();
        messages.iter_current_update_messages().map(|n| n.kind).collect()
    }

    #[test]
    fn training_pays_waits_and_spawns_at_the_rally_point() {
        let (mut app, hq) = app(Cost::new(120, 0));
        order(&mut app, PlayerCommand::SetRally { buildings: vec![hq], point: Vec2::new(0.0, 8.0), node: None });
        order(&mut app, PlayerCommand::Train { building: hq, unit_type: 0 });
        assert_eq!(state(&app).stock, Cost::new(70, 0), "payé à la mise en file");
        assert_eq!(state(&app).population, 1, "la place est réservée");
        assert!(workers(&mut app).is_empty());
        // 1 s de production, puis le trajet vers le point de ralliement.
        for _ in 0..32 * 4 {
            app.update();
        }
        let list = workers(&mut app);
        assert_eq!(list.len(), 1);
        let position = app.world().get::<SimPosition>(list[0]).unwrap().current;
        assert!(position.distance(Vec2::new(0.0, 8.0)) < 0.5, "l'unité a rejoint le ralliement : {position}");
        assert_eq!(state(&app).population, 1);
    }

    #[test]
    fn refusals_and_cancellation() {
        let (mut app, hq) = app(Cost::new(120, 0));
        order(&mut app, PlayerCommand::Train { building: hq, unit_type: 0 });
        order(&mut app, PlayerCommand::Train { building: hq, unit_type: 0 });
        // Plus que 20 : refusé.
        app.world_mut().resource_mut::<CommandQueue>().push(ME, PlayerCommand::Train { building: hq, unit_type: 0 });
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::NotEnoughResources]);
        // On annule le second : remboursé, la place libérée.
        order(&mut app, PlayerCommand::CancelTrain { building: hq, slot: 1 });
        assert_eq!(state(&app).stock, Cost::new(70, 0));
        assert_eq!(app.world().get::<ProductionQueue>(hq).unwrap().items.len(), 1);
        // Population : le QG fournit 3. Deux de plus passent, la quatrième non.
        app.world_mut().resource_mut::<Players>().get_mut(ME).stock = Cost::new(1000, 0);
        order(&mut app, PlayerCommand::Train { building: hq, unit_type: 0 });
        order(&mut app, PlayerCommand::Train { building: hq, unit_type: 0 });
        app.world_mut().resource_mut::<CommandQueue>().push(ME, PlayerCommand::Train { building: hq, unit_type: 0 });
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::PopulationCap]);
        assert_eq!(state(&app).population, 3);
        // Le plafond global l'emporte sur ce que fournissent les bâtiments.
        app.world_mut().resource_mut::<PopulationSettings>().max = 2;
        app.update();
        assert_eq!(state(&app).population_cap, 2);
    }

    #[test]
    fn queue_is_limited_to_five() {
        let (mut app, hq) = app(Cost::new(1000, 0));
        app.world_mut().resource_mut::<PopulationSettings>().max = 100;
        // Un second QG pour avoir assez de population.
        let frame = app.world().resource::<NavGrid>().frame();
        let catalog = app.world_mut().remove_resource::<BuildingCatalog>().unwrap();
        catalog.spawn(&mut app.world_mut().commands(), frame, ME, 0, IVec2::new(60, 60), None);
        app.world_mut().flush();
        app.insert_resource(catalog);
        for _ in 0..5 {
            order(&mut app, PlayerCommand::Train { building: hq, unit_type: 0 });
        }
        app.world_mut().resource_mut::<CommandQueue>().push(ME, PlayerCommand::Train { building: hq, unit_type: 0 });
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::QueueFull]);
        assert_eq!(app.world().get::<ProductionQueue>(hq).unwrap().items.len(), 5);
    }

    #[test]
    fn rally_on_a_node_sends_workers_harvesting() {
        let (mut app, hq) = app(Cost::new(50, 0));
        let frame = app.world().resource::<NavGrid>().frame();
        let node = spawn_resource_node(&mut app.world_mut().commands(), frame, ResourceKind::Primary, IVec2::new(49, 56), UVec2::new(2, 1), 500, false);
        app.world_mut().flush();
        order(&mut app, PlayerCommand::SetRally { buildings: vec![hq], point: Vec2::new(0.0, 6.5), node: Some(node) });
        order(&mut app, PlayerCommand::Train { building: hq, unit_type: 0 });
        for _ in 0..32 * 10 {
            app.update();
        }
        let worker = workers(&mut app)[0];
        assert!(app.world().get::<crate::economy::harvest::Harvesting>(worker).is_some());
        assert!(state(&app).stock.primary >= 5, "il a déjà rapporté une charge");
    }

    #[test]
    fn buildings_cost_and_cancelled_sites_refund() {
        let (mut app, _) = app(Cost::new(150, 0));
        let builder = app
            .world_mut()
            .spawn((
                Unit { type_index: 0 },
                Owner(ME),
                SimPosition::new(Vec2::new(8.0, 8.0)),
                crate::simulation::MoveSpeed(3.5),
                crate::collision::Collider { radius: 0.5, mass: 1.0, layer: crate::collision::CollisionLayer::Ground },
            ))
            .id();
        order(&mut app, PlayerCommand::Build { type_index: 1, min_cell: IVec2::new(60, 50), builders: vec![builder] });
        assert_eq!(state(&app).stock, Cost::new(50, 0), "débité à la pose");
        // Une seconde : trop cher.
        app.world_mut()
            .resource_mut::<CommandQueue>()
            .push(ME, PlayerCommand::Build { type_index: 1, min_cell: IVec2::new(65, 50), builders: vec![builder] });
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::NotEnoughResources]);
        let site = {
            let world = app.world_mut();
            world.query_filtered::<Entity, With<UnderConstruction>>().single(world).unwrap()
        };
        order(&mut app, PlayerCommand::CancelConstruction { building: site });
        assert_eq!(state(&app).stock, Cost::new(150, 0), "remboursé");
        assert!(app.world().get_entity(site).is_err());
    }
}
