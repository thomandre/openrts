//! IA adverse basique : elle récolte, construit, produit et attaque par vagues.
//!
//! Elle ne connaît aucune faction : tout ce qu'elle sait vient des données
//! (catalogues, règles économiques). Son bâtiment principal est le dépôt
//! (`dropoff`), ses ouvriers sont les récolteurs, un bâtiment de population
//! est un type qui fournit de la population (`supply`), un bâtiment de
//! production est un type qui produit autre chose que des ouvriers, et un
//! bâtiment d'extraction se pose sur une source (`OnResource`). La même IA
//! joue donc la Garde, les Sylves ou le Collectif, chacun avec son style de
//! construction.
//!
//! Comme un joueur, elle n'agit que par la file des ordres
//! ([`CommandQueue`]) : ses ordres sont revérifiés par la simulation (coût,
//! prérequis, emplacement), et elle n'a aucun avantage caché. Elle tourne
//! dans la simulation à pas fixe et ne lit que l'état de la simulation, dans
//! un ordre fixe (entités triées) : la même partie donne les mêmes décisions,
//! ce qu'exigera le multijoueur en lockstep.
//!
//! Son plan, réévalué toutes les demi-secondes :
//!
//! 1. les ouvriers inoccupés récoltent (trois par bâtiment d'extraction) ;
//! 2. dépenses, par priorité : population (avant d'être bloquée), ouvriers
//!    jusqu'à [`AiSettings::workers`], un premier bâtiment de production,
//!    l'extraction de la ressource rare, un second bâtiment de production
//!    quand l'argent s'accumule, puis des unités de combat ;
//! 3. armée : elle défend sa base contre les intrus, et, passé
//!    [`AiSettings::first_attack_secs`], attaque dès qu'elle a réuni une
//!    vague (de plus en plus grosse) ; les renforts rejoignent l'attaque.
//!
//! Toutes les valeurs de [`AiSettings`] sont des propositions, à régler en
//! jouant.

use bevy::prelude::*;

use crate::buildings::field::ConstructionFields;
use crate::buildings::{completed_types, Building, BuildingCatalog, ConstructionTask, Footprint, PlacementRule, PlannedSite, UnderConstruction};
use crate::combat::CombatOrder;
use crate::economy::harvest::Harvesting;
use crate::economy::production::ProductionQueue;
use crate::economy::{EconomyRules, ResourceKind, ResourceNode};
use crate::navigation::{GridFrame, NavGrid};
use crate::orders::{CommandQueue, PlayerCommand};
use crate::players::{Owner, PlayerId, Players};
use crate::simulation::{MoveTarget, SimPosition, SimSet, TICKS_PER_SECOND};
use crate::units::Unit;

/// Les réglages de l'IA (valeurs proposées).
#[derive(Resource, Debug, Clone)]
pub struct AiSettings {
    /// Nombre d'ouvriers visé.
    pub workers: usize,
    /// Ouvriers par bâtiment d'extraction.
    pub workers_per_extractor: usize,
    /// Nombre maximal de bâtiments de production.
    pub max_production: usize,
    /// Pas de première attaque avant ce délai (secondes de partie).
    pub first_attack_secs: f32,
    /// Taille de la première vague, et ce que chaque vague ajoute.
    pub first_wave: usize,
    pub wave_growth: usize,
    /// Distance (mètres) à ses bâtiments sous laquelle un ennemi est un intrus.
    pub defense_radius: f32,
    /// Intervalle entre deux réflexions, en pas de simulation.
    pub think_every: u32,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            workers: 16,
            workers_per_extractor: 3,
            max_production: 2,
            first_attack_secs: 150.0,
            first_wave: 10,
            wave_growth: 4,
            defense_radius: 16.0,
            // Deux fois par seconde (la simulation fait 32 pas par seconde).
            think_every: 16,
        }
    }
}

/// Un joueur tenu par l'IA, et ce dont elle se souvient d'une réflexion à l'autre.
#[derive(Debug, Clone)]
pub struct AiPlayer {
    pub player: PlayerId,
    /// Taille de la prochaine vague.
    wave: usize,
    /// Une vague est en route : les renforts la rejoignent.
    attacking: bool,
    /// Bâtiments dont le point de ralliement est déjà posé.
    rallied: Vec<Entity>,
    /// Compteur des unités de combat commandées (pour varier l'armée).
    trained: usize,
}

impl AiPlayer {
    pub fn new(player: PlayerId) -> Self {
        Self { player, wave: 0, attacking: false, rallied: Vec::new(), trained: 0 }
    }
}

/// Les joueurs tenus par l'IA (aucun par défaut).
#[derive(Resource, Debug, Clone, Default)]
pub struct AiPlayers(pub Vec<AiPlayer>);

pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiPlayers>()
            .init_resource::<AiSettings>()
            // Après le combat : elle voit l'état final du pas, et ses ordres
            // partent au pas suivant, comme ceux d'un joueur.
            .add_systems(FixedUpdate, think.after(SimSet::Combat));
    }
}

/// Une unité vue par l'IA.
struct UnitView {
    entity: Entity,
    owner: PlayerId,
    type_index: usize,
    position: Vec2,
    harvesting: Option<Entity>,
    /// Occupée : construction, emplacement promis ou déplacement en cours.
    busy: bool,
    /// Sous un ordre de combat.
    fighting: bool,
}

/// Un bâtiment vu par l'IA.
struct BuildingView {
    entity: Entity,
    owner: PlayerId,
    type_index: usize,
    center: Vec2,
    rect: Rect,
    done: bool,
    queued: usize,
}

type UnitQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Unit, &'static Owner, &'static SimPosition, Option<&'static Harvesting>, Has<ConstructionTask>, Has<PlannedSite>, Has<MoveTarget>, Has<CombatOrder>),
>;
type BuildingQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static Building, &'static Owner, &'static Footprint, Has<UnderConstruction>, Option<&'static ProductionQueue>)>;

fn think(
    mut ticks: Local<u32>,
    mut ais: ResMut<AiPlayers>,
    settings: Res<AiSettings>,
    (catalog, rules, grid): (Option<Res<BuildingCatalog>>, Option<Res<EconomyRules>>, Option<Res<NavGrid>>),
    players: Res<Players>,
    mut orders: ResMut<CommandQueue>,
    units: UnitQuery,
    buildings: BuildingQuery,
    nodes: Query<(Entity, &ResourceNode, &Footprint)>,
    plans: Query<(&PlannedSite, &Owner)>,
) {
    *ticks += 1;
    if ais.0.is_empty() || !(*ticks).is_multiple_of(settings.think_every.max(1)) {
        return;
    }
    let (Some(catalog), Some(rules), Some(grid)) = (catalog, rules, grid) else { return };
    let frame = grid.frame();
    let seconds = *ticks as f32 / TICKS_PER_SECOND as f32;

    // Instantané de la partie, trié par entité (ordre reproductible).
    let mut unit_views: Vec<UnitView> = units
        .iter()
        .map(|(entity, unit, owner, position, harvesting, task, planned, moving, fighting)| UnitView {
            entity,
            owner: owner.0,
            type_index: unit.type_index,
            position: position.current,
            harvesting: harvesting.map(|h| h.node),
            busy: task || planned || moving,
            fighting,
        })
        .collect();
    unit_views.sort_by_key(|u| u.entity);
    let mut building_views: Vec<BuildingView> = buildings
        .iter()
        .map(|(entity, building, owner, footprint, under_construction, queue)| {
            let rect = frame.rect(footprint.min_cell, footprint.size);
            BuildingView {
                entity,
                owner: owner.0,
                type_index: building.type_index,
                center: rect.center(),
                rect,
                done: !under_construction,
                queued: queue.map_or(0, |q| q.items.len()),
            }
        })
        .collect();
    building_views.sort_by_key(|b| b.entity);
    let mut node_list: Vec<(Entity, ResourceNode, Footprint)> = nodes.iter().map(|(e, n, f)| (e, *n, *f)).collect();
    node_list.sort_by_key(|n| n.0);

    for ai in &mut ais.0 {
        let view = Snapshot { catalog: &catalog, rules: &rules, grid: &grid, frame, units: &unit_views, buildings: &building_views, nodes: &node_list };
        let field_iter = buildings.iter().map(|(_, b, o, f, uc, _)| (b, o, f, uc));
        let fields = ConstructionFields::of_player(&catalog, frame, ai.player, field_iter);
        let completed = completed_types(&catalog, ai.player, buildings.iter().map(|(_, b, o, _, uc, _)| (b, o, uc)));
        let planned: Vec<(usize, Rect)> =
            plans.iter().filter(|(_, o)| o.0 == ai.player).map(|(p, _)| (p.type_index, catalog.plan_rect(frame, p))).collect();
        let state = players.get(ai.player);
        let mut plan = Plan { ai, view, settings: &settings, completed, fields, planned, stock: state.stock, orders: &mut orders };
        plan.run(seconds, state.population, state.population_cap, state.faction);
    }
}

/// Ce que l'IA voit de la partie.
struct Snapshot<'a> {
    catalog: &'a BuildingCatalog,
    rules: &'a EconomyRules,
    grid: &'a NavGrid,
    frame: GridFrame,
    units: &'a [UnitView],
    buildings: &'a [BuildingView],
    nodes: &'a [(Entity, ResourceNode, Footprint)],
}

/// Une réflexion de l'IA pour un joueur.
struct Plan<'a> {
    ai: &'a mut AiPlayer,
    view: Snapshot<'a>,
    settings: &'a AiSettings,
    completed: Vec<bool>,
    fields: ConstructionFields,
    /// Emplacements promis à ses ouvriers en route : (type, rectangle).
    planned: Vec<(usize, Rect)>,
    /// Ce qu'il lui reste à dépenser pendant cette réflexion.
    stock: crate::economy::Stock,
    orders: &'a mut CommandQueue,
}

impl Plan<'_> {
    fn run(&mut self, seconds: f32, population: u32, population_cap: u32, faction: usize) {
        let player = self.ai.player;
        let catalog = self.view.catalog;
        let mine: Vec<&BuildingView> = self.view.buildings.iter().filter(|b| b.owner == player).collect();
        // Son bâtiment principal : le premier dépôt terminé.
        let Some(hq) = mine.iter().find(|b| b.done && catalog.types[b.type_index].dropoff).map(|b| (b.entity, b.center)) else {
            // Plus de base : tout ce qui reste attaque.
            self.attack_all(true);
            return;
        };
        let workers: Vec<&UnitView> =
            self.view.units.iter().filter(|u| u.owner == player && self.view.rules.harvesters.contains(&u.type_index)).collect();

        self.harvest(&workers, &mine, hq.1);

        // Types de sa faction, par rôle.
        let own_types: Vec<usize> = (0..catalog.types.len()).filter(|&t| catalog.faction_of(t) == faction).collect();
        let is_harvester = |u: usize| self.view.rules.harvesters.contains(&u);
        let producer_types: Vec<usize> =
            own_types.iter().copied().filter(|&t| catalog.types[t].production.iter().any(|o| !is_harvester(o.unit_type))).collect();
        // Une copie : les fermetures ci-dessous empruntent la liste, et
        // `self` reste libre pour les ordres.
        let planned = self.planned.clone();
        let count = |types: &[usize]| {
            mine.iter().filter(|b| types.contains(&b.type_index)).count() + planned.iter().filter(|(t, _)| types.contains(t)).count()
        };
        let pending = |filter: &dyn Fn(usize) -> bool| {
            mine.iter().any(|b| !b.done && filter(b.type_index)) || planned.iter().any(|(t, _)| filter(*t))
        };

        // 1. Population : un bâtiment de plus avant d'être bloquée.
        let margin = 4 + 3 * count(&producer_types) as u32;
        let supply_pending = pending(&|t| catalog.types[t].supply > 0);
        if population + margin >= population_cap && !supply_pending {
            let supply = own_types
                .iter()
                .copied()
                .filter(|&t| catalog.types[t].supply > 0 && !catalog.types[t].dropoff && catalog.requirements_met(t, &self.completed))
                .min_by_key(|&t| catalog.types[t].cost.primary);
            if let Some(supply) = supply {
                // Elle garde de quoi le payer : rien d'autre ne passe avant.
                if !self.build(supply, hq.1, &workers) {
                    return;
                }
            }
        }

        // 2. Ouvriers.
        let queued_workers: usize = mine.iter().filter(|b| b.entity == hq.0).map(|b| b.queued).sum();
        if workers.len() + queued_workers < self.settings.workers && queued_workers == 0 {
            let worker_type = catalog.types[mine.iter().find(|b| b.entity == hq.0).unwrap().type_index]
                .production
                .iter()
                .find(|o| is_harvester(o.unit_type))
                .map(|o| o.unit_type);
            if let Some(unit_type) = worker_type {
                self.train(hq.0, unit_type, population, population_cap);
            }
        }

        // 3. Bâtiments de production (et ce qu'ils exigent).
        let producers = count(&producer_types);
        let want_producer = producers == 0 || (producers < self.settings.max_production && self.stock.primary >= 300);
        if want_producer && !pending(&|t| producer_types.contains(&t)) {
            let ready = producer_types.iter().copied().filter(|&t| catalog.requirements_met(t, &self.completed)).min_by_key(|&t| catalog.types[t].cost.primary);
            // Sinon, ce qu'il manque pour en poser un.
            let missing = producer_types
                .iter()
                .flat_map(|&t| catalog.types[t].requires.clone())
                .find(|&r| !self.completed[r] && catalog.requirements_met(r, &self.completed) && !pending(&|t| t == r));
            if let Some(type_index) = ready.or(missing) {
                self.build(type_index, hq.1, &workers);
            }
        }

        // 4. Extraction de la ressource rare, une fois la production lancée.
        let extractors: Vec<usize> =
            own_types.iter().copied().filter(|&t| matches!(catalog.types[t].placement, PlacementRule::OnResource(_))).collect();
        if producers > 0
            && count(&extractors) == 0
            && let Some(&extractor) = extractors.iter().find(|&&t| catalog.requirements_met(t, &self.completed))
        {
            self.build_extractor(extractor, hq.1, &workers);
        }

        // 5. Unités de combat.
        let rally = self.rally_point(hq.1);
        for building in mine.iter().filter(|b| b.done && producer_types.contains(&b.type_index)) {
            if !self.ai.rallied.contains(&building.entity) {
                self.ai.rallied.push(building.entity);
                self.orders.push(player, PlayerCommand::SetRally { buildings: vec![building.entity], point: rally, node: None });
            }
            if building.queued >= 2 {
                continue;
            }
            let options: Vec<usize> = catalog.types[building.type_index]
                .production
                .iter()
                .filter(|o| !is_harvester(o.unit_type) && o.requires.iter().all(|r| r.met(&self.completed)))
                .map(|o| o.unit_type)
                .filter(|&u| self.stock.covers(self.view.rules.unit(u).cost))
                .collect();
            if !options.is_empty() {
                let unit_type = options[self.ai.trained % options.len()];
                if self.train(building.entity, unit_type, population, population_cap) {
                    self.ai.trained += 1;
                }
            }
        }

        self.command_army(seconds, &mine, hq.1);
    }

    /// Les ouvriers inoccupés récoltent : d'abord les bâtiments d'extraction
    /// qui manquent de bras, puis le gisement de ressource principale le
    /// moins fréquenté près de la base.
    fn harvest(&mut self, workers: &[&UnitView], mine: &[&BuildingView], hq: Vec2) {
        let player = self.ai.player;
        let idle: Vec<&UnitView> = workers.iter().copied().filter(|u| u.harvesting.is_none() && !u.busy).collect();
        let on_node = |node: Entity| workers.iter().filter(|u| u.harvesting == Some(node)).count();
        // Les sources couvertes par un de ses bâtiments d'extraction terminés.
        let sources: Vec<Entity> = self
            .view
            .nodes
            .iter()
            .filter(|(_, node, _)| node.extractor.is_some_and(|e| mine.iter().any(|b| b.entity == e && b.done)) && node.amount > 0)
            .map(|(e, ..)| *e)
            .collect();
        let mut idle = idle.into_iter();
        for &source in &sources {
            // Un ouvrier de plus par réflexion au plus : l'état sera relu.
            if on_node(source) < self.settings.workers_per_extractor {
                // Un inoccupé, sinon un récolteur de ressource principale.
                let worker = idle.next().map(|u| u.entity).or_else(|| {
                    workers
                        .iter()
                        .find(|u| u.harvesting.is_some_and(|n| !sources.contains(&n)) && !u.busy)
                        .map(|u| u.entity)
                });
                if let Some(worker) = worker {
                    self.orders.push(player, PlayerCommand::Harvest { units: vec![worker], node: source });
                }
            }
        }
        let primary: Vec<(Entity, Vec2)> = self
            .view
            .nodes
            .iter()
            .filter(|(_, node, _)| node.kind == ResourceKind::Primary && node.amount > 0)
            .map(|(e, _, f)| (*e, self.view.frame.rect(f.min_cell, f.size).center()))
            .filter(|(_, center)| center.distance(hq) < 20.0)
            .collect();
        let mut assigned: Vec<(Entity, usize)> = primary.iter().map(|(e, _)| (*e, on_node(*e))).collect();
        for worker in idle {
            // Le moins fréquenté ; à égalité, le plus proche de la base.
            let best = assigned
                .iter_mut()
                .zip(&primary)
                .min_by(|(a, pa), (b, pb)| a.1.cmp(&b.1).then(pa.1.distance(hq).total_cmp(&pb.1.distance(hq))))
                .map(|(slot, _)| slot);
            let Some(slot) = best else { break };
            slot.1 += 1;
            self.orders.push(player, PlayerCommand::Harvest { units: vec![worker.entity], node: slot.0 });
        }
    }

    /// Commande une unité si elle a de quoi la payer et la loger.
    fn train(&mut self, building: Entity, unit_type: usize, population: u32, population_cap: u32) -> bool {
        let economy = self.view.rules.unit(unit_type);
        if population + economy.population > population_cap || !self.stock.try_pay(economy.cost) {
            return false;
        }
        self.orders.push(self.ai.player, PlayerCommand::Train { building, unit_type });
        true
    }

    /// Pose un bâtiment près de la base. `false` s'il n'a pas de quoi le
    /// payer (l'argent est alors gardé pour lui).
    fn build(&mut self, type_index: usize, hq: Vec2, workers: &[&UnitView]) -> bool {
        let cost = self.view.catalog.types[type_index].cost;
        if !self.stock.covers(cost) {
            return false;
        }
        if let Some(min_cell) = self.find_spot(type_index, hq) {
            self.stock.try_pay(cost);
            let builders = workers.iter().filter(|u| !u.busy).map(|u| u.entity).collect();
            info!("IA {:?} : construit {}", self.ai.player, self.view.catalog.types[type_index].name);
            self.orders.push(self.ai.player, PlayerCommand::Build { type_index, min_cell, builders });
            self.planned.push((type_index, self.view.frame.rect(min_cell, self.view.catalog.types[type_index].footprint)));
        }
        true
    }

    /// Pose le bâtiment d'extraction sur la source libre la plus proche de la base.
    fn build_extractor(&mut self, type_index: usize, hq: Vec2, workers: &[&UnitView]) {
        let building = &self.view.catalog.types[type_index];
        if !self.stock.covers(building.cost) {
            return;
        }
        let PlacementRule::OnResource(kind) = building.placement else { return };
        let source = self
            .view
            .nodes
            .iter()
            .filter(|(_, node, footprint)| node.kind == kind && node.needs_extractor && node.extractor.is_none() && footprint.size == building.footprint)
            .map(|(_, _, footprint)| (self.view.frame.rect(footprint.min_cell, footprint.size).center().distance(hq), footprint.min_cell))
            .filter(|(distance, _)| *distance < 20.0)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, min_cell)) = source {
            self.stock.try_pay(building.cost);
            info!("IA {:?} : construit {}", self.ai.player, building.name);
            let builders = workers.iter().filter(|u| !u.busy).map(|u| u.entity).collect();
            self.orders.push(self.ai.player, PlayerCommand::Build { type_index, min_cell, builders });
            self.planned.push((type_index, self.view.frame.rect(min_cell, building.footprint)));
        }
    }

    /// Un emplacement libre pour le type `type_index`, sur des cercles de
    /// plus en plus grands autour de la base, à l'écart des gisements et
    /// avec une case libre tout autour (pour que les unités circulent).
    fn find_spot(&self, type_index: usize, hq: Vec2) -> Option<IVec2> {
        let view = &self.view;
        let footprint = view.catalog.types[type_index].footprint;
        let deposits: Vec<Vec2> = view.nodes.iter().map(|(_, _, f)| view.frame.rect(f.min_cell, f.size).center()).collect();
        // Les gisements sont d'un côté de la base : on construit de l'autre.
        let away = deposits.iter().filter(|d| d.distance(hq) < 16.0).fold(Vec2::ZERO, |sum, d| sum + (hq - *d)).normalize_or(Vec2::Y);
        let start = away.to_angle();
        for radius in 7..26 {
            let steps = 8 + radius * 2;
            for step in 0..steps {
                // Alternativement de part et d'autre de la direction préférée.
                let offset = (step + 1) / 2;
                let sign = if step % 2 == 0 { 1.0 } else { -1.0 };
                let angle = start + sign * offset as f32 * std::f32::consts::TAU / steps as f32;
                let point = hq + Vec2::from_angle(angle) * radius as f32;
                if deposits.iter().any(|d| d.distance(point) < 5.5) {
                    continue;
                }
                let min_cell = view.frame.min_cell_centered(point, footprint);
                let rect = view.frame.rect(min_cell, footprint);
                let margin = Rect::from_corners(rect.min - 1.0, rect.max + 1.0);
                if self.planned.iter().any(|(_, r)| !r.intersect(margin).is_empty())
                    || view.buildings.iter().any(|b| !b.rect.intersect(margin).is_empty())
                    || !view.grid.area_free(min_cell - IVec2::ONE, footprint + UVec2::splat(2))
                {
                    continue;
                }
                let nodes = view.nodes.iter().map(|(e, n, f)| (*e, n, f));
                if view.catalog.check_placement(type_index, min_cell, view.grid, &self.completed, &self.fields, nodes).is_ok() {
                    return Some(min_cell);
                }
            }
        }
        None
    }

    /// Où se rassemblent les unités produites : devant la base, côté ennemi.
    fn rally_point(&self, hq: Vec2) -> Vec2 {
        let target = self.enemy_target(hq).unwrap_or(Vec2::ZERO);
        hq + (target - hq).normalize_or(Vec2::Y) * 10.0
    }

    /// La cible d'une attaque : le bâtiment ennemi le plus proche, sinon
    /// l'unité ennemie la plus proche.
    fn enemy_target(&self, from: Vec2) -> Option<Vec2> {
        let player = self.ai.player;
        let nearest = |points: &mut dyn Iterator<Item = Vec2>| points.min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)));
        nearest(&mut self.view.buildings.iter().filter(|b| b.owner != player).map(|b| b.center))
            .or_else(|| nearest(&mut self.view.units.iter().filter(|u| u.owner != player).map(|u| u.position)))
    }

    /// L'armée : défense, puis attaque par vagues.
    fn command_army(&mut self, seconds: f32, mine: &[&BuildingView], hq: Vec2) {
        let player = self.ai.player;
        let army: Vec<&UnitView> =
            self.view.units.iter().filter(|u| u.owner == player && !self.view.rules.harvesters.contains(&u.type_index)).collect();
        if self.ai.wave == 0 {
            self.ai.wave = self.settings.first_wave;
        }
        // Défense : un ennemi près de ses bâtiments.
        let radius = self.settings.defense_radius;
        let intruder = self
            .view
            .units
            .iter()
            .filter(|u| u.owner != player)
            .find(|u| mine.iter().any(|b| b.center.distance(u.position) < radius + b.rect.half_size().max_element()));
        if let Some(intruder) = intruder {
            let defenders: Vec<Entity> = army.iter().filter(|u| !u.fighting).map(|u| u.entity).collect();
            if !defenders.is_empty() {
                self.orders.push(player, PlayerCommand::AttackMove { units: defenders, target: intruder.position });
            }
            return;
        }
        // Une vague décimée rentre se regrouper.
        if self.ai.attacking && army.len() < 3 {
            self.ai.attacking = false;
        }
        let Some(target) = self.enemy_target(hq) else { return };
        if !self.ai.attacking && seconds >= self.settings.first_attack_secs && army.len() >= self.ai.wave {
            self.ai.attacking = true;
            self.ai.wave += self.settings.wave_growth;
            info!("IA {:?} : vague de {} unités vers {target}", player, army.len());
            let units = army.iter().map(|u| u.entity).collect();
            self.orders.push(player, PlayerCommand::AttackMove { units, target });
        } else if self.ai.attacking {
            // Renforts et unités arrivées au bout : vers la cible suivante.
            let idle: Vec<Entity> = army.iter().filter(|u| !u.fighting && !u.busy).map(|u| u.entity).collect();
            if !idle.is_empty() {
                self.orders.push(player, PlayerCommand::AttackMove { units: idle, target });
            }
        }
    }

    /// Plus de base : toute l'armée attaque ce qu'elle trouve.
    fn attack_all(&mut self, only_idle: bool) {
        let player = self.ai.player;
        let Some(first) = self.view.units.iter().find(|u| u.owner == player) else { return };
        let Some(target) = self.enemy_target(first.position) else { return };
        let units: Vec<Entity> =
            self.view.units.iter().filter(|u| u.owner == player && (!only_idle || (!u.fighting && !u.busy))).map(|u| u.entity).collect();
        if !units.is_empty() {
            self.orders.push(player, PlayerCommand::AttackMove { units, target });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingType, BuildingsPlugin, ConstructionStyle, ProductionOption};
    use crate::combat::CombatPlugin;
    use crate::economy::production::UnitEconomy;
    use crate::economy::{spawn_resource_node, Cost, EconomyPlugin, ResourceInfo};
    use crate::input::{GameAction, InputContext};
    use crate::orders::OrdersPlugin;
    use crate::players::PlayersPlugin;
    use crate::simulation::SimulationPlugin;
    use crate::units::{Palette, SurfaceDetail, SurfaceStyle, UnitCatalog, UnitType};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const AI: PlayerId = PlayerId(1);
    const WORKER: usize = 0;
    const SOLDIER: usize = 1;

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

    /// Une base tenue par l'IA : un dépôt qui produit des ouvriers, deux
    /// ouvriers inoccupés, une ligne de gisements, de quoi construire.
    /// L'IA connaît aussi une caserne (soldats) et un abri (population).
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<GameAction>()
            .init_resource::<InputContext>()
            .init_resource::<crate::cursor::Pointer>()
            .add_plugins((SimulationPlugin, PlayersPlugin, OrdersPlugin, EconomyPlugin, BuildingsPlugin, CombatPlugin, AiPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 32.0)))
            .insert_resource(EconomyRules {
                resources: [ResourceInfo { name: "P".into(), color: Color::WHITE }, ResourceInfo { name: "R".into(), color: Color::WHITE }],
                harvesters: vec![WORKER],
                carry: [5, 4],
                gather_time: [2.0, 1.5],
                units: vec![UnitEconomy { cost: Cost::new(50, 0), train_time: 4.0, population: 1 }; 2],
            })
            .insert_resource(AiPlayers(vec![AiPlayer::new(AI)]));
        let (mut meshes, mut materials, mut images) = (Assets::default(), Assets::default(), Assets::default());
        let unit = |name: &str| UnitType { name: name.into(), parts: Vec::new(), footprint_radius: 0.5, mass: 1.0, speed: 4.0, hover: None };
        let units = UnitCatalog::new(vec![unit("Ouvrier"), unit("Soldat")], &palette(), &mut meshes, &mut materials, &mut images);
        let option = |unit_type| ProductionOption { unit_type, hotkey: 'a', requires: Vec::new() };
        let hq = BuildingType {
            name: "Dépôt".into(),
            footprint: UVec2::new(4, 4),
            max_health: 500.0,
            build_time: 5.0,
            dropoff: true,
            supply: 6,
            production: vec![option(WORKER)],
            ..default()
        };
        let barracks = BuildingType { name: "Caserne".into(), cost: Cost::new(150, 0), dropoff: false, supply: 0, production: vec![option(SOLDIER)], ..hq.clone() };
        let house = BuildingType { name: "Abri".into(), footprint: UVec2::new(2, 2), cost: Cost::new(100, 0), dropoff: false, supply: 8, production: Vec::new(), ..hq.clone() };
        let catalog = BuildingCatalog::new(vec![hq, barracks, house], vec![WORKER], ConstructionStyle::WorkerStays, &units, &mut meshes, &mut materials);
        app.insert_resource(catalog).insert_resource(units);
        app.world_mut().resource_mut::<Players>().get_mut(AI).stock = Cost::new(600, 0);
        // Quelques pas : la grille de navigation est créée au premier pas de simulation.
        for _ in 0..3 {
            app.update();
        }
        let frame = app.world().resource::<NavGrid>().frame();
        let world = app.world_mut();
        world.resource_scope(|world, catalog: Mut<BuildingCatalog>| {
            world.resource_scope(|world, units: Mut<UnitCatalog>| {
                let mut commands = world.commands();
                catalog.spawn(&mut commands, frame, AI, 0, frame.min_cell_centered(Vec2::ZERO, UVec2::new(4, 4)), None);
                for x in [-4.0, -2.0, 0.0, 2.0, 4.0] {
                    spawn_resource_node(&mut commands, frame, ResourceKind::Primary, frame.min_cell_centered(Vec2::new(x, -6.5), UVec2::new(2, 1)), UVec2::new(2, 1), 1000, false);
                }
                for x in [-1.0, 1.0] {
                    units.spawn_owned(&mut commands, AI, WORKER, Vec2::new(x, 3.5), 0.0);
                }
            });
        });
        app.world_mut().flush();
        app
    }

    /// Ce que l'IA a bâti : ouvriers, soldats, bâtiments par type, stock.
    fn summary(app: &mut App) -> (usize, usize, Vec<usize>, Cost) {
        let world = app.world_mut();
        let units: Vec<usize> = world.query::<&Unit>().iter(world).map(|u| u.type_index).collect();
        let mut buildings = vec![0; 3];
        for building in world.query::<&Building>().iter(world) {
            buildings[building.type_index] += 1;
        }
        let stock = world.resource::<Players>().get(AI).stock;
        (units.iter().filter(|&&t| t == WORKER).count(), units.iter().filter(|&&t| t == SOLDIER).count(), buildings, stock)
    }

    #[test]
    fn ai_harvests_builds_and_trains_deterministically() {
        let mut first = app();
        // Une minute de partie.
        for _ in 0..32 * 60 {
            first.update();
        }
        let (workers, soldiers, buildings, _) = summary(&mut first);
        assert!(workers > 2, "l'IA aurait dû produire des ouvriers ({workers})");
        assert!(buildings[1] >= 1, "l'IA aurait dû poser une caserne : {buildings:?}");
        assert!(soldiers >= 1, "l'IA aurait dû produire des soldats");
        {
            let world = first.world_mut();
            assert!(world.query::<&Harvesting>().iter(world).count() >= 2, "les ouvriers devraient récolter");
        }
        // La même partie rejouée donne exactement le même résultat.
        let mut second = app();
        for _ in 0..32 * 60 {
            second.update();
        }
        assert_eq!(summary(&mut first), summary(&mut second));
    }
}
