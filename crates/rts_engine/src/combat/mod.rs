//! Combat : points de vie, armure, armes, ciblage automatique, ordres
//! d'attaque, dégâts et morts.
//!
//! Comme le reste du moteur, le combat ne connaît aucune unité précise : le
//! jeu lui fournit des [`CombatRules`] (une fiche [`CombatStats`] par type
//! d'unité et par type de bâtiment : PV, armure, catégories, armes). Le
//! moteur en équipe chaque unité et chaque bâtiment ([`Combatant`],
//! [`Attacker`]) dès son apparition.
//!
//! # Règles (voir GAME_DESIGN.md, « Unités, combat et lisibilité »)
//!
//! - **Dégâts** d'un coup : dégâts de l'arme + bonus contre une catégorie de
//!   la cible (« forces et faiblesses ») − armure de la cible, au moins 0,5.
//!   Une arme de zone touche aussi les autres ennemis proches de la cible.
//! - **Cibles possibles** : une arme touche le sol, l'air, ou les deux ; une
//!   cible est « aérienne » si elle a la catégorie [`Category::Air`].
//! - **Ciblage automatique** : une unité sans ordre (ou en attaque-déplacement)
//!   prend pour cible l'ennemi à portée d'acquisition qui l'intéresse le plus :
//!   d'abord ce qui attaque, puis les autres unités, puis les bâtiments qui
//!   ne tirent pas ; à priorité égale le plus proche, puis le plus petit
//!   identifiant (départage déterministe).
//! - **Riposte** : une unité attaquée riposte toujours (sauf pendant un
//!   déplacement simple, qui sert à battre en retraite).
//! - **Laisse** : une unité au repos ne poursuit pas au-delà de quelques
//!   mètres de sa position, puis y revient.
//! - **Ordres** : attaquer une cible, attaque-déplacement, stop, tenir la
//!   position ([`CombatOrder`]), tous par la file des ordres.
//! - **Tirs instantanés** (« hitscan ») : les dégâts sont appliqués au pas
//!   du tir. Les traits lumineux à l'écran ne sont que de l'affichage.
//! - **Mort** : une entité à 0 PV disparaît. Tout ce qui la désignait (cible,
//!   récolte, chantier, ralliement…) est revérifié à chaque usage ; la
//!   population se recompte seule, la grille de navigation se reconstruit
//!   quand un bâtiment disparaît.
//!
//! # Déterminisme et performances
//!
//! Tout se passe dans la simulation à pas fixe, sans aléatoire : unités
//! parcourues par identifiant croissant, dégâts d'abord tous calculés puis
//! appliqués ensemble (deux unités peuvent s'entretuer au même pas).
//! Les ennemis proches se cherchent dans une grille spatiale
//! ([`SpatialGrid`]), et chaque unité ne refait sa recherche complète qu'un
//! pas sur [`RESCAN_TICKS`] (en décalé), ou quand sa cible disparaît : pas de
//! comparaison de chaque unité avec toutes les autres.

pub mod effects;
pub mod ui;

use bevy::ecs::query::QueryData;
use bevy::prelude::*;

use crate::buildings::{distance_to_rect, refund_plan, Building, ConstructionTask, Health, PlannedSite, UnderConstruction};
use crate::collision::{Collider, MapBounds, SpatialGrid, StaticObstacle};
use crate::economy::harvest::Harvesting;
use crate::navigation::{MoveGroup, MoveGroupCounter, NavPath};
use crate::orders::{owned_units, PlayerCommand, TickCommands};
use crate::players::{are_enemies, Owner, PlayerId, Players};
use crate::selection::{formation, Selectable};
use crate::simulation::{MoveSpeed, MoveTarget, SimPosition, SimSet, TICKS_PER_SECOND};
use crate::units::Unit;

// ---------------------------------------------------------------------------
// Données (fournies par le jeu).
// ---------------------------------------------------------------------------

/// Une catégorie d'unité ou de bâtiment, pour les bonus de dégâts (« forces
/// et faiblesses ») et les cibles possibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Infantry,
    Armored,
    /// Unité volante : seules les armes qui touchent l'air l'atteignent.
    Air,
    Melee,
    Ranged,
    Worker,
    /// Géant (Bastion lourd, Gardien, Titan…).
    Massive,
    Structure,
}

/// Un ensemble de catégories, rangé dans les bits d'un entier : le bit n°k
/// vaut 1 si la k-ième catégorie est présente (léger à copier et à comparer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Categories(u16);

impl Categories {
    /// `const fn` : utilisable dans une constante, calculée à la compilation.
    pub const fn of(list: &[Category]) -> Self {
        let mut bits = 0;
        let mut i = 0;
        // Pas de `for` dans une `const fn` : une boucle `while` à la place.
        while i < list.len() {
            bits |= 1 << list[i] as u16;
            i += 1;
        }
        Self(bits)
    }

    pub fn contains(self, category: Category) -> bool {
        self.0 & (1 << category as u16) != 0
    }
}

/// Une arme.
#[derive(Debug, Clone, PartialEq)]
pub struct Weapon {
    /// Dégâts d'un coup, avant bonus et armure.
    pub damage: f32,
    /// Portée en mètres, d'un bord à l'autre (entre les cercles au sol, ou
    /// jusqu'au bord d'un bâtiment). Quelques dixièmes : une arme de contact.
    pub range: f32,
    /// Temps entre deux coups, en secondes.
    pub cooldown: f32,
    pub hits_ground: bool,
    pub hits_air: bool,
    /// Dégâts en plus contre une catégorie (« +25 contre les blindés »).
    pub bonus: Vec<(Category, f32)>,
    /// Rayon des dégâts de zone autour de la cible (0 : une seule cible).
    pub splash_radius: f32,
}

impl Weapon {
    /// Arme simple, sans bonus ni zone. `targets` : (sol, air).
    pub fn new(damage: f32, range: f32, cooldown: f32, targets: (bool, bool)) -> Self {
        Self { damage, range, cooldown, hits_ground: targets.0, hits_air: targets.1, bonus: Vec::new(), splash_radius: 0.0 }
    }

    /// La même, avec un bonus contre `category` (méthode « constructeur »).
    pub fn with_bonus(mut self, category: Category, amount: f32) -> Self {
        self.bonus.push((category, amount));
        self
    }

    /// La même, avec des dégâts de zone de rayon `radius`.
    pub fn with_splash(mut self, radius: f32) -> Self {
        self.splash_radius = radius;
        self
    }

    /// Peut-elle toucher une cible aérienne (`air`) ou au sol ?
    pub fn can_hit(&self, air: bool) -> bool {
        if air { self.hits_air } else { self.hits_ground }
    }

    /// Dégâts infligés à une cible de ces catégories et de cette armure.
    pub fn damage_against(&self, categories: Categories, armor: f32) -> f32 {
        let bonus: f32 = self.bonus.iter().filter(|(c, _)| categories.contains(*c)).map(|(_, b)| b).sum();
        (self.damage + bonus - armor).max(MIN_DAMAGE)
    }

    /// Temps entre deux coups, en pas de simulation (au moins 1).
    fn cooldown_ticks(&self) -> u32 {
        (self.cooldown * TICKS_PER_SECOND as f32).round().max(1.0) as u32
    }
}

/// Dégâts minimaux d'un coup, quelle que soit l'armure.
pub const MIN_DAMAGE: f32 = 0.5;

/// La fiche de combat d'un type d'unité ou de bâtiment.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CombatStats {
    /// Points de vie d'une unité. Ignoré pour un bâtiment, dont les PV sont
    /// dans [`crate::buildings::BuildingType::max_health`].
    pub max_health: f32,
    /// Retirée des dégâts de chaque coup reçu.
    pub armor: f32,
    pub categories: Categories,
    /// Ses armes (souvent une seule ; aucune pour un bâtiment ordinaire).
    /// Elle tire avec la première qui peut toucher la cible.
    pub weapons: Vec<Weapon>,
    /// Distance à laquelle elle repère seule un ennemi (0 : portée de sa
    /// plus longue arme + 3 m, au moins 6 m).
    pub acquire_range: f32,
}

impl CombatStats {
    /// Une unité : PV, armure, catégories et armes.
    pub fn unit(max_health: f32, armor: f32, categories: &[Category], weapons: Vec<Weapon>) -> Self {
        Self { max_health, armor, categories: Categories::of(categories), weapons, acquire_range: 0.0 }
    }

    /// Un bâtiment (catégorie `Structure` ajoutée) : armure et armes
    /// éventuelles (défense fixe).
    pub fn structure(armor: f32, categories: &[Category], weapons: Vec<Weapon>) -> Self {
        let mut stats = Self::unit(0.0, armor, categories, weapons);
        stats.categories.0 |= Categories::of(&[Category::Structure]).0;
        stats
    }

    /// Portée d'acquisition effective.
    pub fn acquire(&self) -> f32 {
        if self.acquire_range > 0.0 {
            return self.acquire_range;
        }
        let longest = self.weapons.iter().map(|w| w.range).fold(0.0, f32::max);
        (longest + 3.0).max(6.0)
    }

    /// Portée de sa plus longue arme.
    fn max_range(&self) -> f32 {
        self.weapons.iter().map(|w| w.range).fold(0.0, f32::max)
    }
}

/// Les fiches de combat de la faction, fournies par le jeu : une par type
/// d'unité et par type de bâtiment, dans l'ordre des catalogues. Un type sans
/// fiche a 1 d'armure (bâtiment) ou 0 (unité), et aucune arme.
#[derive(Resource, Debug, Clone, Default)]
pub struct CombatRules {
    pub units: Vec<CombatStats>,
    pub buildings: Vec<CombatStats>,
}

impl CombatRules {
    /// Ajoute les fiches d'une autre faction, dont les types d'unités et de
    /// bâtiments commencent aux indices `first_unit` et `first_building` des
    /// catalogues communs.
    pub fn merge(&mut self, other: CombatRules, first_unit: usize, first_building: usize) {
        self.units.resize(first_unit, CombatStats::default());
        self.units.extend(other.units);
        self.buildings.resize(first_building, CombatStats::default());
        self.buildings.extend(other.buildings);
    }

    fn stats(&self, kind: CombatKind) -> Option<&CombatStats> {
        match kind {
            CombatKind::Unit(i) => self.units.get(i),
            CombatKind::Building(i) => self.buildings.get(i),
        }
    }
}

// ---------------------------------------------------------------------------
// Composants (posés par le moteur).
// ---------------------------------------------------------------------------

/// Unité ou bâtiment, et son type : où trouver sa fiche de combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatKind {
    Unit(usize),
    Building(usize),
}

/// Ce qui peut être attaqué : armure, catégories, et le dernier attaquant
/// (pour la riposte).
#[derive(Component, Debug, Clone, Copy)]
pub struct Combatant {
    pub kind: CombatKind,
    pub armor: f32,
    pub categories: Categories,
    /// Qui l'a touché au dernier pas (effacé au pas suivant).
    pub attacked_by: Option<Entity>,
}

impl Combatant {
    pub fn is_air(&self) -> bool {
        self.categories.contains(Category::Air)
    }
}

/// Ce qui peut attaquer : l'état de ses armes et de son ciblage.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Attacker {
    /// Pas restants avant que chaque arme puisse retirer.
    pub cooldowns: Vec<u32>,
    /// La cible actuelle.
    pub target: Option<Entity>,
    /// Position de repos (au bout de la laisse).
    pub anchor: Vec2,
    /// Point vers lequel l'unité poursuit sa cible (pour ne pas recalculer
    /// son chemin à chaque pas).
    pub chase_goal: Option<Vec2>,
    /// Elle revient à sa position de repos après une poursuite.
    pub returning: bool,
    /// Elle a combattu depuis sa position de repos (et y reviendra).
    pub engaged: bool,
}

/// L'ordre de combat donné par le joueur. Sans ce composant, une unité est
/// au repos : elle repère seule les ennemis, riposte et revient à sa place.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub enum CombatOrder {
    /// Attaquer cette entité, où qu'elle aille.
    Attack(Entity),
    /// Aller à `point` en combattant ce qu'on croise. `interrupted` : la
    /// marche a été suspendue pour un combat, à reprendre ensuite.
    AttackMove { point: Vec2, interrupted: bool },
    /// Ne pas bouger, tirer sur ce qui passe à portée.
    Hold,
}

/// Un évènement de combat, pour l'affichage (traits de tir, impacts, morts).
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub enum CombatEvent {
    /// Un tir : de `from` vers `to` (positions au sol), par le joueur `player`.
    Shot { attacker: Entity, target: Entity, from: Vec2, to: Vec2, player: PlayerId, splash: f32, melee: bool },
    /// Une entité détruite : sa position, son propriétaire et son type.
    Died { entity: Entity, position: Vec2, owner: PlayerId, kind: CombatKind, yaw: f32 },
}

/// Un ordre visant un point vient d'être donné par le joueur local :
/// l'affichage y pose un repère au sol (rouge pour une attaque).
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct OrderMarker {
    pub point: Vec2,
    pub attack: bool,
}

/// Les coups du pas en cours, appliqués ensemble à la fin du ciblage.
#[derive(Resource, Debug, Default)]
struct PendingHits(Vec<(Entity, f32, Entity)>);

/// Compteur de pas du combat (décale les recherches de cible).
#[derive(Resource, Debug, Default)]
struct CombatClock(u64);

/// Une unité ne refait sa recherche complète de cible qu'un pas sur
/// `RESCAN_TICKS` (un quart de seconde), en décalé selon son identifiant.
pub const RESCAN_TICKS: u64 = 8;
/// Une unité au repos poursuit jusqu'à sa portée d'acquisition plus cette
/// distance de sa position de repos, puis y revient.
pub const LEASH: f32 = 5.0;
/// Côté des cases de la grille de recherche des ennemis, en mètres.
const GRID_CELL: f32 = 4.0;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CombatEvent>()
            .add_message::<OrderMarker>()
            .init_resource::<PendingHits>()
            .init_resource::<CombatClock>()
            .add_systems(
                FixedUpdate,
                (
                    equip_combatants.in_set(SimSet::Orders).before(crate::orders::apply_unit_orders),
                    apply_combat_orders
                        .in_set(SimSet::Orders)
                        .after(crate::orders::apply_unit_orders)
                        .after(crate::economy::harvest::apply_harvest_orders)
                        .before(crate::buildings::apply_build_orders),
                    (fight, resolve_hits).chain().in_set(SimSet::Combat),
                ),
            );
    }
}

// ---------------------------------------------------------------------------
// Équipement des nouvelles entités.
// ---------------------------------------------------------------------------

/// Équipe toute unité ou tout bâtiment qui ne l'est pas encore (apparu
/// depuis le dernier pas) d'après sa fiche : PV, [`Combatant`], et
/// [`Attacker`] s'il a des armes.
fn equip_combatants(
    mut commands: Commands,
    rules: Option<Res<CombatRules>>,
    units: Query<(Entity, &Unit, &SimPosition), Without<Combatant>>,
    buildings: Query<(Entity, &Building, &StaticObstacle), Without<Combatant>>,
) {
    let Some(rules) = rules else { return };
    let fallback = |kind: CombatKind| match kind {
        CombatKind::Unit(_) => CombatStats::unit(1.0, 0.0, &[], Vec::new()),
        CombatKind::Building(_) => CombatStats::structure(1.0, &[], Vec::new()),
    };
    let mut equip = |entity: Entity, kind: CombatKind, position: Vec2| {
        let stats = rules.stats(kind).cloned().unwrap_or_else(|| fallback(kind));
        let mut e = commands.entity(entity);
        e.insert(Combatant { kind, armor: stats.armor, categories: stats.categories, attacked_by: None });
        if let CombatKind::Unit(_) = kind {
            e.insert(Health { current: stats.max_health, max: stats.max_health });
        }
        if !stats.weapons.is_empty() {
            e.insert(Attacker {
                cooldowns: vec![0; stats.weapons.len()],
                target: None,
                anchor: position,
                chase_goal: None,
                returning: false,
                engaged: false,
            });
        }
    };
    for (entity, unit, position) in &units {
        equip(entity, CombatKind::Unit(unit.type_index), position.current);
    }
    for (entity, building, obstacle) in &buildings {
        equip(entity, CombatKind::Building(building.type_index), obstacle.0.center());
    }
}

// ---------------------------------------------------------------------------
// Ordres.
// ---------------------------------------------------------------------------

/// Applique les ordres de combat du pas (attaquer, attaque-déplacement,
/// stop, tenir la position), et fait oublier leur combat aux unités qui
/// reçoivent un autre ordre (déplacement simple, récolte).
fn apply_combat_orders(
    mut commands: Commands,
    tick: Res<TickCommands>,
    mut groups: ResMut<MoveGroupCounter>,
    mut players: ResMut<Players>,
    mut units: Query<(&Owner, &Selectable, &SimPosition, Option<&mut Attacker>, Option<&PlannedSite>), With<MoveSpeed>>,
    targets: Query<&Owner, With<Combatant>>,
) {
    for issued in &tick.0 {
        let player = issued.player;
        let owner_of = |e: Entity| units.get(e).ok().map(|(o, ..)| *o);
        let (list, command) = match &issued.command {
            PlayerCommand::Move { units, .. } | PlayerCommand::Harvest { units, .. } => (owned_units(units, player, owner_of), &issued.command),
            PlayerCommand::Attack { units, .. }
            | PlayerCommand::AttackMove { units, .. }
            | PlayerCommand::Stop { units }
            | PlayerCommand::HoldPosition { units } => (owned_units(units, player, owner_of), &issued.command),
            _ => continue,
        };
        if list.is_empty() {
            continue;
        }
        // Un ordre de combat remplace ce que faisait l'unité : comme un
        // déplacement, elle quitte récolte, chantier et emplacement promis (remboursé).
        let takes_over = !matches!(command, PlayerCommand::Move { .. } | PlayerCommand::Harvest { .. });
        let slots = match command {
            PlayerCommand::AttackMove { target, .. } => {
                let radii: Vec<f32> = list.iter().filter_map(|&e| units.get(e).ok()).map(|(_, s, ..)| s.radius).collect();
                formation(&radii, *target)
            }
            _ => Vec::new(),
        };
        let group = groups.next_group();
        for (i, &entity) in list.iter().enumerate() {
            let Ok((_, _, position, attacker, plan)) = units.get_mut(entity) else { continue };
            let here = position.current;
            let armed = attacker.is_some();
            if takes_over && let Some(plan) = plan {
                refund_plan(&mut players, player, plan);
            }
            // L'unité oublie sa cible et sa poursuite.
            if let Some(mut attacker) = attacker {
                attacker.target = None;
                attacker.chase_goal = None;
                attacker.returning = false;
                attacker.engaged = false;
                attacker.anchor = here;
            }
            let mut e = commands.entity(entity);
            e.remove::<CombatOrder>();
            if takes_over {
                e.remove::<(Harvesting, ConstructionTask, PlannedSite)>();
            }
            match command {
                PlayerCommand::Attack { target, .. } => {
                    // Seulement une entité adverse qui peut être attaquée.
                    let valid = targets.get(*target).is_ok_and(|o| are_enemies(player, o.0));
                    if valid && armed {
                        e.insert(CombatOrder::Attack(*target)).remove::<(MoveTarget, NavPath, MoveGroup)>();
                    }
                }
                PlayerCommand::AttackMove { .. } => {
                    let point = slots.get(i).copied().unwrap_or(here);
                    e.insert((MoveTarget(point), group));
                    // Une unité sans arme fait un simple déplacement.
                    if armed {
                        e.insert(CombatOrder::AttackMove { point, interrupted: false });
                    }
                }
                PlayerCommand::Stop { .. } => {
                    e.remove::<(MoveTarget, NavPath, MoveGroup)>();
                }
                PlayerCommand::HoldPosition { .. } => {
                    e.insert(CombatOrder::Hold).remove::<(MoveTarget, NavPath, MoveGroup)>();
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ciblage et tirs.
// ---------------------------------------------------------------------------

/// Ce que le ciblage sait d'une entité attaquable pendant un pas.
#[derive(Debug, Clone, Copy)]
struct View {
    entity: Entity,
    owner: PlayerId,
    /// Centre au sol.
    position: Vec2,
    /// Rayon au sol (unité) ; 0 pour un bâtiment, qui a un rectangle.
    radius: f32,
    rect: Option<Rect>,
    air: bool,
    categories: Categories,
    armor: f32,
    /// Elle attaque quelqu'un (priorité de ciblage).
    attacking: bool,
}

impl View {
    /// Distance entre les bords de deux entités.
    fn gap(&self, other: &View) -> f32 {
        match (self.rect, other.rect) {
            (None, None) => self.position.distance(other.position) - self.radius - other.radius,
            (Some(rect), None) => distance_to_rect(other.position, rect) - other.radius,
            (None, Some(rect)) => distance_to_rect(self.position, rect) - self.radius,
            (Some(a), Some(b)) => {
                // Écart entre deux rectangles, sur chaque axe.
                let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
                let dz = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
                Vec2::new(dx, dz).length()
            }
        }
    }

    /// Priorité de ciblage : plus petit = plus intéressant.
    fn priority(&self) -> u8 {
        match (self.attacking, self.rect.is_some()) {
            (true, _) => 0,
            (false, false) => 1,
            (false, true) => 2,
        }
    }
}

/// Ce que lit et modifie le ciblage, pour chaque entité qui combat.
///
/// `#[derive(QueryData)]` : une structure qui décrit une requête, plus
/// lisible qu'un long tuple. `#[query_data(mutable)]` : elle contient des
/// accès en écriture (`&mut`).
#[derive(QueryData)]
#[query_data(mutable)]
struct Fighter {
    entity: Entity,
    owner: &'static Owner,
    combatant: &'static Combatant,
    health: &'static Health,
    position: Option<&'static SimPosition>,
    obstacle: Option<&'static StaticObstacle>,
    collider: Option<&'static Collider>,
    attacker: Option<&'static mut Attacker>,
    order: Option<&'static CombatOrder>,
    moving: Has<MoveTarget>,
    /// Ouvrier occupé (récolte, chantier, emplacement promis) : il ne combat pas.
    harvesting: Has<Harvesting>,
    building_task: Has<ConstructionTask>,
    planned: Has<PlannedSite>,
    under_construction: Has<UnderConstruction>,
}

/// Un pas de combat : chaque entité armée choisit sa cible, la poursuit ou
/// lui tire dessus. Les coups sont rangés dans [`PendingHits`], appliqués
/// juste après par [`resolve_hits`].
#[allow(clippy::too_many_arguments)]
fn fight(
    mut commands: Commands,
    rules: Option<Res<CombatRules>>,
    bounds: Res<MapBounds>,
    mut clock: ResMut<CombatClock>,
    mut hits: ResMut<PendingHits>,
    mut events: MessageWriter<CombatEvent>,
    mut fighters: Query<Fighter>,
) {
    let Some(rules) = rules else { return };
    clock.0 += 1;

    // 1. Ce qu'on sait de chaque entité attaquable, trié par identifiant.
    let mut views: Vec<View> = fighters
        .iter()
        .filter(|f| f.health.current > 0.0)
        .filter_map(|f| {
            let (position, radius, rect) = match (f.position, f.obstacle) {
                (Some(p), _) => (p.current, f.collider.map_or(0.5, |c| c.radius), None),
                (None, Some(o)) => (o.0.center(), 0.0, Some(o.0)),
                _ => return None,
            };
            Some(View {
                entity: f.entity,
                owner: f.owner.0,
                position,
                radius,
                rect,
                air: f.combatant.is_air(),
                categories: f.combatant.categories,
                armor: f.combatant.armor,
                attacking: f.attacker.as_ref().is_some_and(|a| a.target.is_some()),
            })
        })
        .collect();
    views.sort_unstable_by_key(|v| v.entity);
    let find = |views: &[View], entity: Entity| views.binary_search_by_key(&entity, |v| v.entity).ok();

    // 2. Grille des unités (les bâtiments, peu nombreux, sont à part).
    let unit_indices: Vec<usize> = (0..views.len()).filter(|&i| views[i].rect.is_none()).collect();
    let building_indices: Vec<usize> = (0..views.len()).filter(|&i| views[i].rect.is_some()).collect();
    let mut grid = SpatialGrid::new(bounds.0, GRID_CELL);
    grid.rebuild(unit_indices.iter().map(|&i| views[i].position));
    let max_radius = unit_indices.iter().map(|&i| views[i].radius).fold(0.0, f32::max);

    // Les entités (indices dans `views`) à moins de `reach` de `me`, d'un
    // bord à l'autre, avec leur écart, rangées dans `out`.
    let nearby = |me: &View, reach: f32, out: &mut Vec<(usize, f32)>| {
        out.clear();
        let mut consider = |index: usize| {
            let gap = me.gap(&views[index]);
            if gap <= reach {
                out.push((index, gap));
            }
        };
        let search = reach + me.radius + max_radius + me.rect.map_or(0.0, |r| r.half_size().length());
        grid.for_each_within(me.position, search, |k| consider(unit_indices[k]));
        for &index in &building_indices {
            consider(index);
        }
    };
    let mut found: Vec<(usize, f32)> = Vec::new();

    // 3. Les décisions, entité par entité, dans l'ordre des identifiants.
    let mut order: Vec<Entity> = fighters.iter().filter(|f| f.attacker.is_some()).map(|f| f.entity).collect();
    order.sort_unstable();
    for entity in order {
        let Ok(mut f) = fighters.get_mut(entity) else { continue };
        let Some(stats) = rules.stats(f.combatant.kind) else { continue };
        // `take` sort l'attaquant de l'élément de requête ; c'est une
        // référence (`Mut`) : les modifications vont droit au composant.
        let Some(mut attacker) = f.attacker.take() else { continue };
        for cooldown in attacker.cooldowns.iter_mut() {
            *cooldown = cooldown.saturating_sub(1);
        }
        let Some(me_index) = find(&views, entity) else { continue };
        let me = views[me_index];
        let busy = f.harvesting || f.building_task || f.planned;
        if busy || f.under_construction || stats.weapons.is_empty() {
            attacker.target = None;
            attacker.chase_goal = None;
            attacker.returning = false;
            attacker.engaged = false;
            continue;
        }
        let is_building = me.rect.is_some();
        let order = f.order.copied();
        let hold = is_building || order == Some(CombatOrder::Hold);
        let explicit = match order {
            Some(CombatOrder::Attack(target)) => Some(target),
            _ => None,
        };
        let idle = order.is_none() && !is_building;
        // Déplacement simple (ordre du joueur, ralliement) : on ignore les
        // ennemis, pour pouvoir battre en retraite.
        if f.moving && order.is_none() && attacker.chase_goal.is_none() && !attacker.returning {
            attacker.target = None;
            continue;
        }
        let acquire = stats.acquire();
        let max_range = stats.max_range();
        let anchor = attacker.anchor;
        // Jusqu'où une cible reste acceptable (sans ordre explicite).
        let acceptable = |other: &View, gap: f32| -> bool {
            if hold {
                gap <= max_range
            } else if idle {
                // La laisse : mesurée depuis la position de repos.
                let from_anchor = View { position: anchor, ..me };
                from_anchor.gap(other) <= acquire + LEASH
            } else {
                gap <= acquire + 2.0
            }
        };
        let hittable = |other: &View| stats.weapons.iter().any(|w| w.can_hit(other.air)) && are_enemies(me.owner, other.owner);

        // La cible actuelle est-elle toujours valable ?
        let mut target = explicit.or(attacker.target).and_then(|t| find(&views, t)).filter(|&t| {
            let other = &views[t];
            hittable(other) && (explicit.is_some() || acceptable(other, me.gap(other)))
        });
        let mut commands_e = commands.entity(entity);
        if explicit.is_some() && target.is_none() {
            // La cible désignée a disparu : l'ordre est terminé.
            commands_e.remove::<CombatOrder>();
            attacker.anchor = me.position;
        }

        if explicit.is_none() {
            // Riposte : on se retourne contre qui nous a touché, s'il est
            // plus intéressant que notre cible actuelle.
            if let Some(by) = f.combatant.attacked_by.and_then(|a| find(&views, a)) {
                let other = &views[by];
                let better = target.is_none_or(|t| views[t].priority() > 0);
                if better && hittable(other) && acceptable(other, me.gap(other)) {
                    target = Some(by);
                }
            }
            // Recherche complète : sans cible, ou de temps en temps (en décalé).
            let rescan = target.is_none() || (clock.0 + u64::from(entity.index_u32())).is_multiple_of(RESCAN_TICKS);
            if rescan {
                let reach = if hold { max_range } else { acquire };
                nearby(&me, reach, &mut found);
                // `min_by` avec une clé composée : priorité, puis écart, puis identifiant.
                let best = found
                    .iter()
                    .filter(|(index, _)| hittable(&views[*index]))
                    .min_by(|a, b| {
                        let (va, vb) = (&views[a.0], &views[b.0]);
                        va.priority().cmp(&vb.priority()).then(a.1.total_cmp(&b.1)).then(va.entity.cmp(&vb.entity))
                    })
                    .map(|&(index, _)| index);
                if let Some(best) = best {
                    let switch = target.is_none_or(|t| views[best].priority() < views[t].priority());
                    if switch {
                        target = Some(best);
                    }
                }
            }
        }
        attacker.target = target.map(|t| views[t].entity);
        if target.is_some() && idle {
            attacker.engaged = true;
        }

        // 4. Agir : tirer si c'est à portée, sinon poursuivre.
        match target {
            Some(t) => {
                let other = views[t];
                let gap = me.gap(&other);
                let in_range = stats.weapons.iter().any(|w| w.can_hit(other.air) && gap <= w.range);
                if in_range {
                    if f.moving && !is_building {
                        commands_e.remove::<(MoveTarget, NavPath, MoveGroup)>();
                        attacker.chase_goal = None;
                        attacker.returning = false;
                        if let Some(CombatOrder::AttackMove { point, .. }) = order {
                            commands_e.insert(CombatOrder::AttackMove { point, interrupted: true });
                        }
                    }
                    for (w, weapon) in stats.weapons.iter().enumerate() {
                        if attacker.cooldowns[w] > 0 || !weapon.can_hit(other.air) || gap > weapon.range {
                            continue;
                        }
                        attacker.cooldowns[w] = weapon.cooldown_ticks();
                        hits.0.push((other.entity, weapon.damage_against(other.categories, other.armor), entity));
                        if weapon.splash_radius > 0.0 {
                            nearby(&other, weapon.splash_radius, &mut found);
                            for &(index, _) in &found {
                                let near = &views[index];
                                if near.entity != other.entity && are_enemies(me.owner, near.owner) && weapon.can_hit(near.air) {
                                    hits.0.push((near.entity, weapon.damage_against(near.categories, near.armor), entity));
                                }
                            }
                        }
                        events.write(CombatEvent::Shot {
                            attacker: entity,
                            target: other.entity,
                            from: me.position,
                            to: other.position,
                            player: me.owner,
                            splash: weapon.splash_radius,
                            melee: weapon.range < 1.0,
                        });
                        // Une seule arme par pas.
                        break;
                    }
                } else if !hold {
                    // Poursuite : vers l'unité, ou vers le centre du bâtiment
                    // (la navigation s'arrête à la case libre la plus proche).
                    let goal = other.position;
                    let stale = attacker.chase_goal.is_none_or(|g| g.distance(goal) > 1.0);
                    if stale || !f.moving {
                        commands_e.insert(MoveTarget(goal)).remove::<MoveGroup>();
                        attacker.chase_goal = Some(goal);
                    }
                    attacker.returning = false;
                    if let Some(CombatOrder::AttackMove { point, .. }) = order {
                        commands_e.insert(CombatOrder::AttackMove { point, interrupted: true });
                    }
                }
            }
            None => {
                attacker.chase_goal = None;
                match order {
                    Some(CombatOrder::AttackMove { point, interrupted }) => {
                        if interrupted {
                            // Le combat est fini : on reprend la marche.
                            commands_e.insert((MoveTarget(point), CombatOrder::AttackMove { point, interrupted: false }));
                        } else if !f.moving {
                            // Arrivé : l'unité se met au repos ici.
                            commands_e.remove::<CombatOrder>();
                            attacker.anchor = me.position;
                        }
                    }
                    _ if idle => {
                        if attacker.returning {
                            if !f.moving {
                                attacker.returning = false;
                                attacker.anchor = me.position;
                            }
                        } else if attacker.engaged {
                            // Fin du combat : retour à la position de repos
                            // si la poursuite l'en a éloignée.
                            attacker.engaged = false;
                            if me.position.distance(attacker.anchor) > 1.0 {
                                commands_e.insert(MoveTarget(attacker.anchor)).remove::<MoveGroup>();
                                attacker.returning = true;
                            }
                        } else if !f.moving {
                            attacker.anchor = me.position;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Applique les coups du pas, retient le dernier attaquant de chaque cible
/// (riposte), et retire les entités détruites.
fn resolve_hits(
    mut commands: Commands,
    mut hits: ResMut<PendingHits>,
    mut players: ResMut<Players>,
    mut events: MessageWriter<CombatEvent>,
    mut targets: Query<(
        Entity,
        &Owner,
        &mut Health,
        &mut Combatant,
        Option<&SimPosition>,
        Option<&StaticObstacle>,
        Option<&PlannedSite>,
        Option<&Transform>,
    )>,
) {
    // Le dernier attaquant ne vaut que pour un pas.
    for (.., mut combatant, _, _, _, _) in &mut targets {
        if combatant.attacked_by.is_some() {
            combatant.attacked_by = None;
        }
    }
    let mut touched: Vec<Entity> = Vec::new();
    for (target, damage, by) in hits.0.drain(..) {
        let Ok((_, _, mut health, mut combatant, ..)) = targets.get_mut(target) else { continue };
        health.current -= damage;
        combatant.attacked_by = Some(by);
        touched.push(target);
    }
    touched.sort_unstable();
    touched.dedup();
    for entity in touched {
        let Ok((_, owner, health, combatant, position, obstacle, plan, transform)) = targets.get(entity) else { continue };
        if health.current > 0.0 {
            continue;
        }
        // Un ouvrier tué en route vers son emplacement promis : remboursé
        // (le bâtiment n'a jamais été posé).
        if let Some(plan) = plan {
            refund_plan(&mut players, owner.0, plan);
        }
        let position = position.map(|p| p.current).or(obstacle.map(|o| o.0.center())).unwrap_or_default();
        // L'orientation ne sert qu'à l'affichage de l'épave.
        let yaw = transform.map_or(0.0, |t| t.rotation.to_euler(EulerRot::YXZ).0);
        events.write(CombatEvent::Died { entity, position, owner: owner.0, kind: combatant.kind, yaw });
        // `despawn` retire aussi les enfants (modèle, anneau). Un bâtiment
        // retiré libère sa place dans la grille de navigation au pas suivant.
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests;
