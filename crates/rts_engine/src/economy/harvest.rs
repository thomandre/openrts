//! Récolte : les ouvriers vont au gisement, extraient, rapportent au dépôt
//! le plus proche, et recommencent.
//!
//! Chaque ouvrier en récolte porte un composant [`Harvesting`], petit
//! automate à quatre états ([`HarvestPhase`]) :
//!
//! 1. **ToNode** : il marche vers son gisement ;
//! 2. **Waiting** : il est arrivé mais un autre ouvrier extrait déjà ; il
//!    attend à côté, ou part vers un gisement voisin libre ;
//! 3. **Gathering** : il extrait pendant un temps fixe (en pas de simulation) ;
//! 4. **Returning** : il rapporte sa charge au dépôt le plus proche, qui la
//!    verse au stock de son joueur, puis repart.
//!
//! Un gisement n'accepte qu'**un** ouvrier à la fois ([`ResourceNode::occupant`]) :
//! c'est ce qui crée la saturation des RTS du genre. Deux ouvriers par
//! gisement l'occupent presque sans pause (l'un extrait pendant que l'autre
//! fait l'aller-retour) ; au-delà de trois, les suivants attendent. Un
//! gisement épuisé disparaît ; ses ouvriers passent au voisin le plus proche.
//!
//! Une source de ressource rare ne se récolte que couverte d'un bâtiment
//! d'extraction terminé, du même joueur.
//!
//! Tout se passe dans la simulation à pas fixe, sans aléatoire : ouvriers
//! parcourus dans l'ordre de leurs identifiants, égalités départagées par
//! identifiant. Seul l'affichage de la charge portée vit dans `Update`.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::{EconomyRules, ResourceKind, ResourceNode};
use crate::buildings::{
    approach_point, distance_to_rect, Building, BuildingCatalog, ConstructionTask, UnderConstruction, CONTACT_MARGIN,
};
use crate::collision::{Collider, StaticObstacle};
use crate::graphics::shapes::merge_parts;
use crate::navigation::{MoveGroup, NavGrid, NavPath};
use crate::orders::{owned_units, PlayerCommand, TickCommands};
use crate::players::{Owner, PlayerId, Players};
use crate::simulation::{MoveTarget, SimPosition, TICKS_PER_SECOND};
use crate::units::{PartShape, Unit};

/// Où en est un ouvrier dans son cycle de récolte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarvestPhase {
    ToNode,
    Waiting,
    /// Extraction en cours : nombre de pas restants.
    Gathering { ticks_left: u32 },
    Returning,
}

/// Un ouvrier en récolte.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Harvesting {
    /// Le gisement visé.
    pub node: Entity,
    /// La ressource récoltée (pour trouver un voisin si le gisement s'épuise).
    pub kind: ResourceKind,
    /// Centre du gisement visé, retenu pour chercher un voisin s'il disparaît.
    pub node_position: Vec2,
    pub phase: HarvestPhase,
    /// La charge portée : ressource et quantité.
    pub carrying: Option<(ResourceKind, u32)>,
}

/// Distance (en plus de son rayon) sous laquelle un ouvrier arrivé près d'un
/// gisement occupé s'arrête pour attendre son tour.
const WAIT_MARGIN: f32 = 1.6;
/// Rayon (mètres, autour du gisement visé) dans lequel un ouvrier qui attend
/// cherche un gisement voisin libre.
const SWITCH_RADIUS: f32 = 3.5;
/// Rayon dans lequel on cherche un remplaçant à un gisement épuisé.
const REPLACE_RADIUS: f32 = 12.0;

/// Ce que la récolte sait d'un gisement pendant un pas.
#[derive(Debug, Clone, Copy)]
struct NodeView {
    entity: Entity,
    kind: ResourceKind,
    rect: Rect,
    /// Récoltable par ce joueur (quantité restante, extracteur terminé…).
    harvestable_by: Option<PlayerId>,
    any_player: bool,
    occupant: Option<Entity>,
}

impl NodeView {
    fn usable_by(&self, player: PlayerId) -> bool {
        self.any_player || self.harvestable_by == Some(player)
    }
}

/// Applique les ordres de récolte : seuls les ouvriers du joueur (types
/// listés dans [`EconomyRules::harvesters`]) les acceptent.
pub(crate) fn apply_harvest_orders(
    mut commands: Commands,
    tick: Res<TickCommands>,
    rules: Option<Res<EconomyRules>>,
    nodes: Query<(&ResourceNode, &StaticObstacle)>,
    workers: Query<(&Owner, &Unit, Option<&Harvesting>)>,
) {
    let Some(rules) = rules else { return };
    for issued in &tick.0 {
        let PlayerCommand::Harvest { units, node } = &issued.command else { continue };
        let Ok((resource, obstacle)) = nodes.get(*node) else { continue };
        let owner = |e: Entity| workers.get(e).ok().filter(|(_, unit, _)| rules.harvesters.contains(&unit.type_index)).map(|(o, ..)| *o);
        for worker in owned_units(units, issued.player, owner) {
            let carrying = workers.get(worker).ok().and_then(|(.., h)| h.and_then(|h| h.carrying));
            commands
                .entity(worker)
                .insert(Harvesting {
                    node: *node,
                    kind: resource.kind,
                    node_position: obstacle.0.center(),
                    // Un ouvrier chargé rapporte d'abord sa charge.
                    phase: if carrying.is_some() { HarvestPhase::Returning } else { HarvestPhase::ToNode },
                    carrying,
                })
                .remove::<(ConstructionTask, MoveTarget, NavPath, MoveGroup)>();
        }
    }
}

/// Fait avancer le cycle de récolte de chaque ouvrier d'un pas.
#[allow(clippy::too_many_arguments)]
pub(crate) fn harvest(
    mut commands: Commands,
    rules: Option<Res<EconomyRules>>,
    grid: Option<Res<NavGrid>>,
    building_catalog: Option<Res<BuildingCatalog>>,
    mut players: ResMut<Players>,
    mut nodes: Query<(Entity, &mut ResourceNode, &StaticObstacle)>,
    buildings: Query<(Entity, &Building, &Owner, &StaticObstacle, Has<UnderConstruction>)>,
    mut workers: Query<(Entity, &Owner, &SimPosition, &Collider, &mut Harvesting, Has<MoveTarget>)>,
) {
    let (Some(rules), Some(grid)) = (rules, grid) else { return };

    // Les gisements, en lecture, dans un ordre fixe.
    let mut views: Vec<NodeView> = nodes
        .iter()
        .map(|(entity, node, obstacle)| {
            // Une source n'est récoltable que par le propriétaire de son
            // bâtiment d'extraction, une fois celui-ci terminé.
            let extractor_owner = node
                .extractor
                .and_then(|e| buildings.get(e).ok())
                .filter(|(.., under_construction)| !under_construction)
                .map(|(_, _, owner, ..)| owner.0);
            NodeView {
                entity,
                kind: node.kind,
                rect: obstacle.0,
                harvestable_by: if node.amount > 0 { extractor_owner } else { None },
                any_player: node.amount > 0 && !node.needs_extractor,
                occupant: node.occupant,
            }
        })
        .collect();
    views.sort_unstable_by_key(|v| v.entity);

    // Un occupant qui n'extrait plus ce gisement (ordre reçu, disparu) le libère.
    for view in &mut views {
        if let Some(occupant) = view.occupant {
            let still = workers
                .get(occupant)
                .is_ok_and(|(.., h, _)| h.node == view.entity && matches!(h.phase, HarvestPhase::Gathering { .. }));
            if !still {
                view.occupant = None;
            }
        }
    }

    // Les dépôts terminés (rectangle, propriétaire), dans un ordre fixe.
    let mut dropoffs: Vec<(Entity, Rect, PlayerId)> = buildings
        .iter()
        .filter(|(_, building, _, _, under_construction)| {
            !under_construction
                && building_catalog.as_ref().is_some_and(|c| c.types.get(building.type_index).is_some_and(|t| t.dropoff))
        })
        .map(|(entity, _, owner, obstacle, _)| (entity, obstacle.0, owner.0))
        .collect();
    dropoffs.sort_unstable_by_key(|d| d.0);

    // Quantités prélevées pendant ce pas, appliquées à la fin.
    let mut taken: Vec<(Entity, u32)> = Vec::new();
    let mut order: Vec<Entity> = workers.iter().map(|(e, ..)| e).collect();
    order.sort_unstable();

    for entity in order {
        let Ok((_, owner, position, collider, mut job, moving)) = workers.get_mut(entity) else { continue };
        let (player, pos, radius) = (owner.0, position.current, collider.radius);
        // Au contact : à moins de `CONTACT_MARGIN` du bord. Un ouvrier arrêté
        // un peu plus loin (bousculé, coin encombré) compte aussi, plutôt que
        // de repartir sans fin vers un point qu'il ne peut pas atteindre.
        let touch = radius + if moving { CONTACT_MARGIN } else { 0.75 };
        let mut stop = false;
        let mut go_to: Option<Vec2> = None;

        match job.phase {
            HarvestPhase::Returning => {
                let nearest = dropoffs
                    .iter()
                    .filter(|d| d.2 == player)
                    .map(|d| (distance_to_rect(pos, d.1), d.0, d.1))
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                match nearest {
                    // Pas de dépôt : on attend sur place, chargé.
                    None => stop = true,
                    Some((distance, ..)) if distance <= touch => {
                        if let Some((kind, amount)) = job.carrying.take() {
                            players.get_mut(player).stock.deposit(kind, amount);
                        }
                        job.phase = HarvestPhase::ToNode;
                        stop = true;
                    }
                    Some((_, _, rect)) => {
                        if !moving {
                            go_to = Some(approach_point(&grid, pos, radius, rect));
                        }
                    }
                }
            }
            HarvestPhase::ToNode | HarvestPhase::Waiting => {
                // Le gisement est-il toujours là et récoltable ? Sinon, le
                // plus proche (de l'ancien) de la même ressource.
                let current = views.iter().position(|v| v.entity == job.node && v.usable_by(player));
                let index = match current {
                    Some(i) => Some(i),
                    None => nearest_node(&views, job.kind, player, job.node_position, REPLACE_RADIUS, None),
                };
                let Some(mut index) = index else {
                    // Plus rien à récolter alentour : l'ouvrier s'arrête.
                    commands.entity(entity).remove::<Harvesting>();
                    continue;
                };
                let occupied = |v: &NodeView| v.occupant.is_some_and(|o| o != entity);
                let mut distance = distance_to_rect(pos, views[index].rect);
                // Gisement occupé et nous tout près : un voisin libre ?
                if occupied(&views[index]) && distance <= radius + WAIT_MARGIN {
                    let center = views[index].rect.center();
                    if let Some(free) = nearest_node(&views, job.kind, player, center, SWITCH_RADIUS, Some(entity)) {
                        index = free;
                        distance = distance_to_rect(pos, views[index].rect);
                        stop = true;
                    }
                }
                let view = &mut views[index];
                if job.node != view.entity {
                    job.node = view.entity;
                    job.node_position = view.rect.center();
                    job.phase = HarvestPhase::ToNode;
                }
                if distance <= touch && !occupied(view) {
                    // Au contact d'un gisement libre : on le prend et on extrait.
                    view.occupant = Some(entity);
                    let seconds = rules.gather_time[view.kind.index()];
                    let ticks = (seconds * TICKS_PER_SECOND as f32).round().max(1.0) as u32;
                    job.phase = HarvestPhase::Gathering { ticks_left: ticks };
                    stop = true;
                } else if distance <= radius + WAIT_MARGIN && occupied(view) {
                    job.phase = HarvestPhase::Waiting;
                    stop = true;
                } else {
                    job.phase = HarvestPhase::ToNode;
                    if !moving || stop {
                        stop = false;
                        go_to = Some(approach_point(&grid, pos, radius, view.rect));
                    }
                }
            }
            HarvestPhase::Gathering { ticks_left } => {
                if ticks_left > 1 {
                    job.phase = HarvestPhase::Gathering { ticks_left: ticks_left - 1 };
                } else {
                    // Fin de l'extraction : on prend ce qui reste, au plus une charge.
                    if let Some(view) = views.iter_mut().find(|v| v.entity == job.node) {
                        view.occupant = None;
                        let (_, node, _) = nodes.get(job.node).expect("gisement présent dans la liste");
                        let already = taken.iter().filter(|t| t.0 == job.node).map(|t| t.1).sum::<u32>();
                        let amount = rules.carry[view.kind.index()].min(node.amount - already);
                        if amount > 0 {
                            taken.push((job.node, amount));
                            job.carrying = Some((view.kind, amount));
                        }
                    }
                    job.phase = if job.carrying.is_some() { HarvestPhase::Returning } else { HarvestPhase::ToNode };
                }
            }
        }

        let mut entity_commands = commands.entity(entity);
        if let Some(goal) = go_to {
            entity_commands.insert(MoveTarget(goal)).remove::<MoveGroup>();
        } else if stop && moving {
            entity_commands.remove::<(MoveTarget, NavPath, MoveGroup)>();
        }
    }

    // On écrit l'état des gisements : occupants et quantités prélevées.
    for view in &views {
        let Ok((entity, mut node, _)) = nodes.get_mut(view.entity) else { continue };
        let removed: u32 = taken.iter().filter(|t| t.0 == entity).map(|t| t.1).sum();
        // `set_if_neq`-like : on n'écrit que ce qui change (sinon Bevy
        // signalerait un changement à chaque pas, et les visuels se recalculeraient).
        if node.occupant != view.occupant {
            node.occupant = view.occupant;
        }
        if removed > 0 {
            node.amount -= removed;
            // Un gisement vidé disparaît (et libère la place) ; une source
            // épuisée reste, sous son bâtiment d'extraction.
            if node.amount == 0 && !node.needs_extractor {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Le gisement récoltable par `player`, de la ressource `kind`, le plus
/// proche de `from` à moins de `radius` mètres ; avec `free_for`, seulement
/// ceux qui ne sont pas occupés par un autre ouvrier que lui.
fn nearest_node(
    views: &[NodeView],
    kind: ResourceKind,
    player: PlayerId,
    from: Vec2,
    radius: f32,
    free_for: Option<Entity>,
) -> Option<usize> {
    views
        .iter()
        .enumerate()
        .filter(|(_, v)| v.kind == kind && v.usable_by(player))
        .filter(|(_, v)| free_for.is_none_or(|me| v.occupant.is_none_or(|o| o == me)))
        .map(|(i, v)| (distance_to_rect(from, v.rect), v.entity, i))
        .filter(|(d, ..)| *d <= radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(.., i)| i)
}

// ---------------------------------------------------------------------------
// Affichage : la charge portée.
// ---------------------------------------------------------------------------

/// La charge visible d'un ouvrier (enfant de son entité).
#[derive(Component, Debug)]
pub(crate) struct CarriedLoad(ResourceKind);

/// Maillages et matériaux des charges, créés au premier besoin.
#[derive(Resource)]
pub(crate) struct CarryAssets {
    meshes: [Handle<Mesh>; 2],
    materials: [Handle<StandardMaterial>; 2],
}

/// Montre la charge portée (un éclat de la couleur de la ressource, tenu
/// devant l'ouvrier) et la retire une fois déposée.
pub(crate) fn show_carried_loads(
    mut commands: Commands,
    rules: Option<Res<EconomyRules>>,
    assets: Option<Res<CarryAssets>>,
    // `Option` : absents dans les tests sans rendu.
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    workers: Query<(Entity, Option<&Harvesting>, Option<&Children>), With<Unit>>,
    loads: Query<&CarriedLoad>,
) {
    let (Some(rules), Some(mut meshes), Some(mut materials)) = (rules, meshes, materials) else { return };
    let Some(assets) = assets else {
        // Premier passage : un cristal taillé pour la ressource principale,
        // une capsule pour la rare.
        let gem = PartShape::Faceted {
            profile: vec![Vec2::new(0.0, -0.18), Vec2::new(0.13, -0.05), Vec2::new(0.11, 0.1), Vec2::new(0.0, 0.2)],
            sides: 5,
        };
        let capsule = PartShape::Cylinder { radius: 0.11, height: 0.3 };
        let material = |kind: ResourceKind| StandardMaterial {
            base_color: Color::BLACK,
            emissive: rules.resource(kind).color.to_linear() * 2.5,
            ..default()
        };
        commands.insert_resource(CarryAssets {
            meshes: [
                meshes.add(merge_parts([(&gem, Transform::IDENTITY)])),
                meshes.add(merge_parts([(&capsule, Transform::from_rotation(Quat::from_rotation_x(0.4)))])),
            ],
            materials: [materials.add(material(ResourceKind::Primary)), materials.add(material(ResourceKind::Rare))],
        });
        return;
    };
    for (worker, job, children) in &workers {
        let wanted = job.and_then(|j| j.carrying).map(|(kind, _)| kind);
        let mut shown = None;
        for child in children.into_iter().flatten() {
            if let Ok(load) = loads.get(*child) {
                if Some(load.0) == wanted {
                    shown = Some(load.0);
                } else {
                    commands.entity(*child).despawn();
                }
            }
        }
        if let (Some(kind), None) = (wanted, shown) {
            let i = kind.index();
            commands.entity(worker).with_children(|parent| {
                parent.spawn((
                    CarriedLoad(kind),
                    Mesh3d(assets.meshes[i].clone()),
                    MeshMaterial3d(assets.materials[i].clone()),
                    // Devant l'ouvrier (son avant est −z), à hauteur de poitrine.
                    Transform::from_xyz(0.0, 0.3, -0.5).with_scale(Vec3::splat(1.6)),
                    NotShadowCaster,
                ));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::{BuildingType, BuildingsPlugin, ConstructionStyle, PlacementRule};
    use crate::economy::{spawn_resource_node, EconomyPlugin, ResourceInfo};
    use crate::input::{GameAction, InputContext};
    use crate::orders::{CommandQueue, OrdersPlugin};
    use crate::players::PlayersPlugin;
    use crate::simulation::{MoveSpeed, SimulationPlugin};
    use crate::units::{Palette, SurfaceDetail, SurfaceStyle, UnitCatalog};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const DEPOT: usize = 0;
    const EXTRACTOR: usize = 1;

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

    /// Une simulation sans fenêtre, avec un dépôt de 4 × 4 m terminé centré
    /// en (0, 0) et un catalogue (dépôt, extracteur sur ressource rare).
    fn app() -> App {
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
            });
        let (mut meshes, mut materials, mut images) = (Assets::default(), Assets::default(), Assets::default());
        let units = UnitCatalog::new(Vec::new(), &palette(), &mut meshes, &mut materials, &mut images);
        let depot = BuildingType { name: "Dépôt".into(), footprint: UVec2::new(4, 4), dropoff: true, ..default() };
        let extractor = BuildingType {
            name: "Puits".into(),
            footprint: UVec2::new(2, 2),
            placement: PlacementRule::OnResource(ResourceKind::Rare),
            ..default()
        };
        let catalog = BuildingCatalog::new(vec![depot, extractor], vec![0], ConstructionStyle::WorkerStays, &units, &mut meshes, &mut materials);
        let frame = crate::navigation::GridFrame::new(Rect::new(-50.0, -50.0, 50.0, 50.0), &default());
        catalog.spawn(&mut app.world_mut().commands(), frame, PlayerId(0), DEPOT, IVec2::new(48, 48), None);
        app.insert_resource(catalog);
        app.world_mut().flush();
        // Quelques pas : la grille de navigation est créée au premier.
        for _ in 0..3 {
            app.update();
        }
        app
    }

    /// Un gisement de `size` cases en `min_cell` (la case (50, 50) a son coin en (0, 0)).
    fn node(app: &mut App, kind: ResourceKind, min_cell: IVec2, size: UVec2, amount: u32) -> Entity {
        let frame = app.world().resource::<NavGrid>().frame();
        let entity =
            spawn_resource_node(&mut app.world_mut().commands(), frame, kind, min_cell, size, amount, kind == ResourceKind::Rare);
        app.world_mut().flush();
        entity
    }

    fn worker(app: &mut App, pos: Vec2) -> Entity {
        app.world_mut()
            .spawn((
                Unit { type_index: 0 },
                Owner(PlayerId(0)),
                SimPosition::new(pos),
                MoveSpeed(3.5),
                Collider { radius: 0.6, mass: 1.0, layer: crate::collision::CollisionLayer::Ground },
            ))
            .id()
    }

    fn order(app: &mut App, units: Vec<Entity>, node: Entity) {
        app.world_mut().resource_mut::<CommandQueue>().push(PlayerId(0), PlayerCommand::Harvest { units, node });
    }

    fn run(app: &mut App, seconds: f32) {
        for _ in 0..(seconds * 32.0) as usize {
            app.update();
        }
    }

    fn stock(app: &App) -> crate::economy::Stock {
        app.world().resource::<Players>().get(PlayerId(0)).stock
    }

    #[test]
    fn a_worker_completes_harvest_cycles() {
        let mut app = app();
        // Gisement de 2 × 1 m, à 4 m au-dessus (en z) du dépôt.
        let patch = node(&mut app, ResourceKind::Primary, IVec2::new(49, 56), UVec2::new(2, 1), 1000);
        let w = worker(&mut app, Vec2::new(0.0, 4.0));
        order(&mut app, vec![w], patch);
        // Un aller-retour : 2 s d'extraction, deux trajets de 2 à 3 m.
        run(&mut app, 4.0);
        assert!(app.world().get::<Harvesting>(w).is_some());
        run(&mut app, 16.0);
        let collected = stock(&app).primary;
        assert!(collected >= 15 && collected.is_multiple_of(5), "récolté : {collected}");
        // Ce qui est dans le stock et dans les bras manque au gisement.
        let carried = app.world().get::<Harvesting>(w).unwrap().carrying.map_or(0, |c| c.1);
        let left = app.world().get::<ResourceNode>(patch).unwrap().amount;
        assert_eq!(left + collected + carried, 1000);
    }

    #[test]
    fn a_depleted_node_vanishes_and_workers_move_to_the_next() {
        let mut app = app();
        let small = node(&mut app, ResourceKind::Primary, IVec2::new(49, 56), UVec2::new(2, 1), 5);
        let other = node(&mut app, ResourceKind::Primary, IVec2::new(52, 56), UVec2::new(2, 1), 100);
        let w = worker(&mut app, Vec2::new(0.0, 4.0));
        order(&mut app, vec![w], small);
        run(&mut app, 20.0);
        assert!(app.world().get_entity(small).is_err(), "le petit gisement est épuisé");
        assert!(stock(&app).primary > 5, "l'ouvrier a continué sur le voisin");
        assert_eq!(app.world().get::<Harvesting>(w).unwrap().node, other);
    }

    #[test]
    fn workers_take_turns_on_a_node() {
        let mut app = app();
        let patch = node(&mut app, ResourceKind::Primary, IVec2::new(49, 56), UVec2::new(2, 1), 1000);
        let workers: Vec<Entity> = (0..4).map(|i| worker(&mut app, Vec2::new(-1.5 + i as f32, 3.5))).collect();
        order(&mut app, workers.clone(), patch);
        let mut most = 0;
        for _ in 0..32 * 20 {
            app.update();
            let world = app.world();
            let gathering = workers
                .iter()
                .filter(|&&w| matches!(world.get::<Harvesting>(w).map(|h| h.phase), Some(HarvestPhase::Gathering { .. })))
                .count();
            most = most.max(gathering);
        }
        assert_eq!(most, 1, "jamais deux ouvriers à la fois sur un gisement");
        // Saturé : le gisement a presque toujours quelqu'un (2 s par charge de 5).
        assert!(stock(&app).primary >= 35, "récolté : {}", stock(&app).primary);
    }

    #[test]
    fn rare_resource_needs_a_finished_extractor() {
        let mut app = app();
        let source = node(&mut app, ResourceKind::Rare, IVec2::new(49, 56), UVec2::new(2, 2), 300);
        let w = worker(&mut app, Vec2::new(3.0, 4.0));
        order(&mut app, vec![w], source);
        run(&mut app, 1.0);
        assert!(app.world().get::<Harvesting>(w).is_none(), "pas d'extracteur : rien à récolter");
        // Un extracteur terminé, posé sur la source.
        let frame = app.world().resource::<NavGrid>().frame();
        // On sort le catalogue du monde le temps de s'en servir (sinon le
        // monde serait emprunté deux fois : en lecture et en écriture).
        let catalog = app.world_mut().remove_resource::<BuildingCatalog>().unwrap();
        let extractor = catalog.spawn(&mut app.world_mut().commands(), frame, PlayerId(0), EXTRACTOR, IVec2::new(49, 56), None);
        app.world_mut().flush();
        app.insert_resource(catalog);
        app.world_mut().get_mut::<ResourceNode>(source).unwrap().extractor = Some(extractor);
        order(&mut app, vec![w], source);
        run(&mut app, 12.0);
        assert!(stock(&app).rare >= 8, "récolté : {}", stock(&app).rare);
        assert_eq!(stock(&app).primary, 0);
    }
}
