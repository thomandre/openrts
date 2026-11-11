//! Brouillard de guerre, côté simulation : ce que voit chaque joueur.
//!
//! Chaque unité et chaque bâtiment voit autour de lui, à un rayon donné en
//! données (le champ `sight` des fiches de combat, voir
//! [`crate::combat::CombatStats::sight`]), mesuré comme la portée des armes :
//! depuis son bord. La carte est découpée en cases de [`CELL_SIZE`] mètre ;
//! pour chaque joueur, chaque case est dans l'un de trois états
//! ([`CellVision`]) : jamais vue, déjà vue, ou visible maintenant.
//!
//! # Règles
//!
//! - une unité ne repère seule (et ne garde pour cible) que des ennemis
//!   visibles par son joueur ; un ordre d'attaque ne vise qu'un ennemi visible,
//!   ou un bâtiment ennemi dont le joueur se souvient (voir [`crate::combat`]) ;
//! - le joueur ne sélectionne et ne vise que ce qu'il voit (l'affichage cache
//!   le reste, voir [`display`]) ;
//! - l'IA ne connaît que ce que voit son joueur, plus les bâtiments ennemis
//!   déjà aperçus (voir [`crate::ai`]) ;
//! - **souvenir** : chaque joueur retient les bâtiments ennemis aperçus, tels
//!   qu'au dernier aperçu ([`RememberedBuilding`]). Il ne les oublie qu'en
//!   revoyant leur emplacement vide (détruits entre-temps).
//!
//! # Déterminisme et performances
//!
//! Tout se calcule dans la simulation à pas fixe, un pas sur
//! [`UPDATE_TICKS`]. Marquer une case « visible » ne fait que poser une
//! valeur : le résultat ne dépend pas de l'ordre dans lequel on parcourt les
//! unités. Les souvenirs sont rangés par identifiant d'entité (`BTreeMap`),
//! dans un ordre fixe. Chaque source de vision remplit son disque ligne par
//! ligne (une racine carrée par ligne de cases, puis un remplissage d'un
//! seul tenant) : 600 unités coûtent quelques dizaines de microsecondes.
//!
//! # Pour les autres modules (minicarte…)
//!
//! La ressource [`FogOfWar`] répond à « ce point, ce rectangle, cette entité
//! sont-ils visibles par le joueur P ? » ([`FogOfWar::is_visible`],
//! [`FogOfWar::is_rect_visible`], [`FogOfWar::sees_entity`]), donne l'état
//! brut de chaque case ([`FogOfWar::cells`]) et les bâtiments dont un joueur
//! se souvient ([`FogOfWar::remembered`]). Côté affichage, les entités que le
//! joueur local ne voit pas portent le marqueur [`display::FogHidden`], et
//! les souvenirs de bâtiments sont des entités [`display::BuildingMemory`].

pub mod display;

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::buildings::{Building, UnderConstruction};
use crate::collision::{Collider, MapBounds, StaticObstacle};
use crate::combat::{CombatKind, CombatRules, DEFAULT_SIGHT};
use crate::navigation::GridFrame;
use crate::players::{Owner, PlayerId};
use crate::simulation::{SimPosition, SimSet};
use crate::units::Unit;

/// Côté d'une case de la grille de vision, en mètres.
pub const CELL_SIZE: f32 = 1.0;

/// La vision est recalculée un pas de simulation sur `UPDATE_TICKS` (16 fois
/// par seconde) : une unité à 4 m/s avance de 25 cm entre deux calculs.
pub const UPDATE_TICKS: u32 = 2;

/// Ce qu'un joueur sait d'une case.
///
/// `#[repr(u8)]` : chaque valeur tient dans un octet (0, 1 ou 2), ce qui
/// garde la grille compacte. L'ordre des variantes sert aux comparaisons
/// (`PartialOrd`) : `Unexplored < Explored < Visible`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CellVision {
    /// Jamais vue.
    #[default]
    Unexplored = 0,
    /// Déjà vue, mais personne ne la regarde en ce moment.
    Explored = 1,
    /// Visible maintenant.
    Visible = 2,
}

/// Réglages du brouillard, fournis par le jeu.
#[derive(Resource, Debug, Clone)]
pub struct VisionSettings {
    /// `false` : tout est visible pour tout le monde (démonstrations,
    /// `OPENRTS_FOG=0`). La vision reste calculée (pour la minicarte).
    pub enabled: bool,
}

impl Default for VisionSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Un bâtiment ennemi tel qu'un joueur l'a vu pour la dernière fois.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RememberedBuilding {
    pub owner: PlayerId,
    /// Type de bâtiment (indice du catalogue).
    pub type_index: usize,
    /// Son empreinte au sol.
    pub rect: Rect,
    /// Avancement du chantier au dernier aperçu (`None` : terminé).
    pub progress: Option<f32>,
}

/// Une source de vision pour un pas : un joueur, une zone (un point pour une
/// unité, l'empreinte pour un bâtiment) et un rayon autour de cette zone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisionSource {
    pub player: PlayerId,
    pub area: Rect,
    pub radius: f32,
}

/// Ce que sait un joueur : l'état de chaque case et ses souvenirs.
#[derive(Debug, Clone, Default)]
struct PlayerVision {
    cells: Vec<CellVision>,
    /// `BTreeMap` : un dictionnaire trié par clé ; on le parcourt toujours
    /// dans le même ordre (par identifiant d'entité), contrairement à `HashMap`.
    memory: BTreeMap<Entity, RememberedBuilding>,
}

/// Disposition de la grille : origine (coin bas-gauche) et nombre de cases.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Layout {
    origin: Vec2,
    columns: i32,
    rows: i32,
}

impl Layout {
    fn index(&self, cell: IVec2) -> usize {
        (cell.y * self.columns + cell.x) as usize
    }

    /// La case qui contient `point`, si elle est dans la carte.
    fn cell_of(&self, point: Vec2) -> Option<IVec2> {
        let cell = ((point - self.origin) / CELL_SIZE).floor().as_ivec2();
        (cell.x >= 0 && cell.y >= 0 && cell.x < self.columns && cell.y < self.rows).then_some(cell)
    }

    /// Les cases (coins inclus, bornées à la carte) que chevauche vraiment
    /// `rect` (un simple contact par un bord ne compte pas, comme dans la
    /// grille de navigation). Un rectangle réduit à un point donne sa case.
    fn cells_under(&self, rect: Rect) -> (IVec2, IVec2) {
        let min = ((rect.min - self.origin) / CELL_SIZE + 1e-3).floor().as_ivec2();
        let max = ((rect.max - self.origin) / CELL_SIZE - 1e-3).ceil().as_ivec2() - 1;
        (min.max(IVec2::ZERO), max.max(min).min(IVec2::new(self.columns - 1, self.rows - 1)))
    }

    /// Une case chevauchée par `rect` est-elle visible dans `cells` ?
    fn rect_visible(&self, cells: &[CellVision], rect: Rect) -> bool {
        if cells.is_empty() {
            return false;
        }
        let (min, max) = self.cells_under(rect);
        (min.y..=max.y).any(|y| (min.x..=max.x).any(|x| cells[self.index(IVec2::new(x, y))] == CellVision::Visible))
    }

    /// Marque visibles toutes les cases dont le centre est à moins de
    /// `source.radius` de sa zone : un disque autour d'un point, un
    /// rectangle aux coins arrondis autour d'une empreinte.
    fn stamp(&self, cells: &mut [CellVision], source: &VisionSource) {
        let (area, r) = (source.area, source.radius.max(0.0));
        // Coordonnée (en cases) du centre de la case `i` : `i + 0,5`.
        let first = |from: f32, origin: f32| ((from - origin) / CELL_SIZE - 0.5).ceil() as i32;
        let last = |to: f32, origin: f32| ((to - origin) / CELL_SIZE - 0.5).floor() as i32;
        let y_min = first(area.min.y - r, self.origin.y).max(0);
        let y_max = last(area.max.y + r, self.origin.y).min(self.rows - 1);
        for y in y_min..=y_max {
            let center_z = self.origin.y + (y as f32 + 0.5) * CELL_SIZE;
            // Écart vertical entre le centre de la ligne et la zone.
            let dz = (area.min.y - center_z).max(center_z - area.max.y).max(0.0);
            if dz > r {
                continue;
            }
            // Demi-largeur du disque sur cette ligne (Pythagore).
            let half = (r * r - dz * dz).sqrt();
            let x_min = first(area.min.x - half, self.origin.x).max(0);
            let x_max = last(area.max.x + half, self.origin.x).min(self.columns - 1);
            if x_min > x_max {
                continue;
            }
            let row = self.index(IVec2::new(0, y));
            // `fill` remplit d'un coup une tranche (`[a..=b]`) du tableau.
            cells[row + x_min as usize..=row + x_max as usize].fill(CellVision::Visible);
        }
    }
}

/// Le brouillard de guerre de la partie : la grille de vision de chaque
/// joueur et ses souvenirs. Tenu par la simulation ; l'affichage, l'IA et
/// la minicarte le lisent.
#[derive(Resource, Debug, Clone, Default)]
pub struct FogOfWar {
    enabled: bool,
    bounds: Rect,
    layout: Layout,
    /// Rangés par numéro de joueur.
    players: Vec<PlayerVision>,
    version: u64,
}

impl FogOfWar {
    /// Un brouillard couvrant la carte `bounds`, où personne n'a rien vu.
    pub fn new(bounds: Rect) -> Self {
        let size = (bounds.size() / CELL_SIZE).ceil().as_ivec2().max(IVec2::ONE);
        Self {
            enabled: true,
            bounds,
            layout: Layout { origin: bounds.min, columns: size.x, rows: size.y },
            players: Vec::new(),
            version: 0,
        }
    }

    /// Le brouillard est-il actif ? Sinon, tout est visible.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Repère de la grille (origine et côté des cases).
    pub fn frame(&self) -> GridFrame {
        GridFrame { origin: self.layout.origin, cell_size: CELL_SIZE }
    }

    /// Nombre de cases (colonnes, lignes) ; les lignes suivent l'axe z.
    pub fn size(&self) -> UVec2 {
        UVec2::new(self.layout.columns as u32, self.layout.rows as u32)
    }

    /// Augmente à chaque calcul de la vision (l'affichage s'en sert pour ne
    /// travailler que quand quelque chose a changé).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// L'état brut de chaque case pour le joueur `player`, ligne par ligne
    /// (case `(x, z)` à l'indice `z * colonnes + x`) ; vide pour un joueur
    /// qui n'a encore rien vu. Ignore [`FogOfWar::is_enabled`].
    pub fn cells(&self, player: PlayerId) -> &[CellVision] {
        self.players.get(usize::from(player.0)).map_or(&[], |p| p.cells.as_slice())
    }

    /// L'état brut de la case qui contient `point` (hors carte : jamais vue).
    /// Ignore [`FogOfWar::is_enabled`].
    pub fn at(&self, player: PlayerId, point: Vec2) -> CellVision {
        let cells = self.cells(player);
        match self.layout.cell_of(point) {
            Some(cell) if !cells.is_empty() => cells[self.layout.index(cell)],
            _ => CellVision::Unexplored,
        }
    }

    /// Le joueur `player` voit-il le point `point` en ce moment ?
    pub fn is_visible(&self, player: PlayerId, point: Vec2) -> bool {
        !self.enabled || self.at(player, point) == CellVision::Visible
    }

    /// Le joueur `player` voit-il au moins une case de `rect` (un bâtiment
    /// est visible dès qu'on en voit un coin) ?
    pub fn is_rect_visible(&self, player: PlayerId, rect: Rect) -> bool {
        !self.enabled || self.layout.rect_visible(self.cells(player), rect)
    }

    /// Le joueur `player` a-t-il déjà vu le point `point` ?
    pub fn is_explored(&self, player: PlayerId, point: Vec2) -> bool {
        !self.enabled || self.at(player, point) >= CellVision::Explored
    }

    /// Le joueur `viewer` voit-il une entité du joueur `owner` placée en
    /// `position` (une unité), ou d'empreinte `rect` (un bâtiment) ? Ses
    /// propres entités sont toujours visibles.
    pub fn sees_entity(&self, viewer: PlayerId, owner: PlayerId, position: Vec2, rect: Option<Rect>) -> bool {
        viewer == owner
            || match rect {
                Some(rect) => self.is_rect_visible(viewer, rect),
                None => self.is_visible(viewer, position),
            }
    }

    /// Les joueurs qui voient ce point (une unité) ou ce rectangle (un
    /// bâtiment) : le bit n°p vaut 1 si le joueur p le voit. Tous les bits
    /// sont à 1 quand le brouillard est coupé. Le propriétaire n'est pas
    /// ajouté d'office.
    pub fn seen_mask(&self, position: Vec2, rect: Option<Rect>) -> u32 {
        if !self.enabled {
            return u32::MAX;
        }
        let mut mask = 0;
        // `.take(32)` : un `u32` n'a que 32 bits (au-delà, voir `mask_sees`).
        for (p, vision) in self.players.iter().enumerate().take(32) {
            let seen = match rect {
                Some(rect) => self.layout.rect_visible(&vision.cells, rect),
                None => self.layout.cell_of(position).is_some_and(|c| !vision.cells.is_empty() && vision.cells[self.layout.index(c)] == CellVision::Visible),
            };
            if seen {
                mask |= 1 << p;
            }
        }
        mask
    }

    /// Les bâtiments ennemis dont se souvient le joueur `player`, par
    /// identifiant croissant. Un souvenir peut désigner une entité qui
    /// n'existe plus (bâtiment détruit hors de sa vue).
    pub fn remembered(&self, player: PlayerId) -> impl Iterator<Item = (Entity, &RememberedBuilding)> {
        self.players.get(usize::from(player.0)).into_iter().flat_map(|p| p.memory.iter().map(|(e, b)| (*e, b)))
    }

    /// Le souvenir qu'a `player` du bâtiment `entity`, s'il en a un.
    pub fn remembers(&self, player: PlayerId, entity: Entity) -> Option<&RememberedBuilding> {
        self.players.get(usize::from(player.0)).and_then(|p| p.memory.get(&entity))
    }

    /// Un calcul de la vision : les cases visibles au calcul précédent
    /// deviennent « déjà vues », chaque source marque son disque, puis
    /// chaque joueur met à jour ses souvenirs à partir de `buildings`
    /// (tous les bâtiments de la partie, **triés par entité**).
    pub fn update(&mut self, sources: &[VisionSource], buildings: &[(Entity, RememberedBuilding)]) {
        let layout = self.layout;
        let cell_count = (layout.columns * layout.rows) as usize;
        let needed = sources.iter().map(|s| s.player).chain(buildings.iter().map(|(_, b)| b.owner)).map(|p| usize::from(p.0) + 1).max().unwrap_or(0);
        if self.players.len() < needed {
            self.players.resize_with(needed, PlayerVision::default);
        }
        for vision in &mut self.players {
            if vision.cells.len() != cell_count {
                vision.cells = vec![CellVision::Unexplored; cell_count];
            }
            for cell in vision.cells.iter_mut().filter(|c| **c == CellVision::Visible) {
                *cell = CellVision::Explored;
            }
        }
        for source in sources {
            layout.stamp(&mut self.players[usize::from(source.player.0)].cells, source);
        }
        for (p, vision) in self.players.iter_mut().enumerate() {
            let player = PlayerId(p as u8);
            for (entity, building) in buildings {
                if building.owner != player && layout.rect_visible(&vision.cells, building.rect) {
                    vision.memory.insert(*entity, *building);
                }
            }
            // `retain` garde les éléments pour lesquels la fermeture renvoie
            // vrai : on oublie un bâtiment disparu dont on voit l'emplacement.
            let cells = &vision.cells;
            vision.memory.retain(|entity, building| {
                buildings.binary_search_by_key(entity, |(e, _)| *e).is_ok() || !layout.rect_visible(cells, building.rect)
            });
        }
        self.version = self.version.wrapping_add(1);
    }
}

/// Le joueur `player` est-il dans le masque `mask` de [`FogOfWar::seen_mask`] ?
/// (Les numéros au-delà de 31 sont réputés voir tout.)
pub fn mask_sees(mask: u32, player: PlayerId) -> bool {
    player.0 >= 32 || mask & (1 << player.0) != 0
}

pub struct VisionPlugin;

impl Plugin for VisionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VisionSettings>().init_resource::<FogOfWar>().add_systems(
            FixedUpdate,
            // Après les déplacements et les chantiers (positions finales du
            // pas), avant le combat, qui ne vise que ce qui est visible.
            update_vision.after(SimSet::Construction).before(SimSet::Combat),
        );
    }
}

/// Recalcule la vision de tous les joueurs (un pas sur [`UPDATE_TICKS`]).
fn update_vision(
    mut ticks: Local<u32>,
    settings: Res<VisionSettings>,
    bounds: Res<MapBounds>,
    rules: Option<Res<CombatRules>>,
    mut fog: ResMut<FogOfWar>,
    units: Query<(&Unit, &Owner, &SimPosition, Option<&Collider>)>,
    buildings: Query<(Entity, &Building, &Owner, &StaticObstacle, Option<&UnderConstruction>)>,
) {
    fog.enabled = settings.enabled;
    // Nouvelle carte (ou premier pas) : une grille neuve, calculée tout de suite.
    let fresh = fog.bounds != bounds.0;
    if fresh {
        *fog = FogOfWar { enabled: settings.enabled, ..FogOfWar::new(bounds.0) };
    }
    let tick = *ticks;
    *ticks = ticks.wrapping_add(1);
    if !fresh && !tick.is_multiple_of(UPDATE_TICKS) {
        return;
    }
    let sight = |kind: CombatKind| rules.as_ref().and_then(|r| r.stats(kind)).map_or(DEFAULT_SIGHT, |s| s.sight());
    // L'ordre des sources est sans importance (voir la doc du module).
    let mut sources: Vec<VisionSource> = units
        .iter()
        .map(|(unit, owner, position, collider)| VisionSource {
            player: owner.0,
            area: Rect::from_center_size(position.current, Vec2::ZERO),
            radius: sight(CombatKind::Unit(unit.type_index)) + collider.map_or(0.0, |c| c.radius),
        })
        .collect();
    let mut list: Vec<(Entity, RememberedBuilding)> = buildings
        .iter()
        .map(|(entity, building, owner, obstacle, construction)| {
            let memory = RememberedBuilding {
                owner: owner.0,
                type_index: building.type_index,
                rect: obstacle.0,
                progress: construction.map(|c| c.progress),
            };
            (entity, memory)
        })
        .collect();
    list.sort_unstable_by_key(|(entity, _)| *entity);
    sources.extend(list.iter().map(|(_, b)| VisionSource {
        player: b.owner,
        area: b.rect,
        radius: sight(CombatKind::Building(b.type_index)),
    }));
    fog.update(&sources, &list);
}

#[cfg(test)]
mod tests;
