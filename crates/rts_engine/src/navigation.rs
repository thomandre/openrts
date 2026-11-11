//! Navigation : grille, recherche de chemin A*, lissage et suivi des chemins.
//!
//! La carte est découpée en cases carrées (1 m par défaut, voir
//! [`NavSettings::cell_size`]). Une case est **bloquée** si un obstacle fixe
//! ([`StaticObstacle`] : bâtiment, rocher) la recouvre. Pour chaque case libre,
//! on calcule aussi son **dégagement** (« clearance ») : la distance entre son
//! centre et l'obstacle le plus proche (ou le bord de la carte). Une unité de
//! rayon `r` ne passe par une case que si ce dégagement est assez grand : un
//! Bastion (1,8 m de rayon) ne s'engage pas dans un couloir où seule
//! l'infanterie passe.
//!
//! Le chemin lui-même est trouvé par **A\*** (« A étoile ») : on explore les
//! cases en partant du départ, toujours en premier celle dont le coût estimé
//! « chemin déjà parcouru + distance restante à vol d'oiseau » est le plus
//! petit. Tant que l'estimation de la distance restante ne la surestime
//! jamais, le premier chemin trouvé est le plus court.
//!
//! Le chemin de case en case fait des zigzags : on le **lisse** ensuite en
//! « tirant la ficelle » (string pulling) : depuis chaque point, on saute
//! directement au point le plus lointain du chemin qu'on voit en ligne droite
//! (sans traverser de case trop étroite).
//!
//! Pour tenir 300 unités qui reçoivent un ordre en même temps, les unités d'un
//! même ordre de groupe ([`MoveGroup`]) et de même rayon partagent **un seul**
//! A* : chacune rejoint ce « couloir » commun en ligne droite, puis le quitte
//! pour sa place dans la formation. Les recherches restantes sont étalées sur
//! plusieurs pas de simulation, avec un budget de cases explorées par pas
//! ([`NavSettings::expansion_budget`]). Plus tard, avec de très grands groupes,
//! des **flow fields** (un champ de directions calculé une fois pour toute la
//! carte vers une destination) remplaceront ce partage (voir GAME_DESIGN.md).
//!
//! Tout est déterministe : coûts entiers, départage fixe des égalités, aucune
//! table de hachage, requêtes traitées dans l'ordre des identifiants.
//! Les unités volantes ([`CollisionLayer::Air`]) ignorent la grille.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy::prelude::*;

use crate::collision::{Collider, CollisionLayer, MapBounds, StaticObstacle};
use crate::simulation::{MoveTarget, SimPosition};
use crate::terrain::Terrain;

/// Réglages de la navigation.
#[derive(Resource, Debug, Clone)]
pub struct NavSettings {
    /// Côté d'une case de la grille, en mètres.
    pub cell_size: f32,
    /// Nombre maximal de cases explorées par A* à chaque pas de simulation,
    /// toutes recherches confondues (au moins une recherche est toujours faite).
    pub expansion_budget: usize,
}

impl Default for NavSettings {
    fn default() -> Self {
        Self { cell_size: 1.0, expansion_budget: 40_000 }
    }
}

/// Coût d'un pas droit et d'un pas en diagonale, en dixièmes de case.
/// Des entiers plutôt que 1 et √2 : les sommes sont exactes, donc les
/// comparaisons (et le chemin choisi) identiques sur toutes les machines.
const STRAIGHT_COST: u32 = 10;
const DIAGONAL_COST: u32 = 14;

/// Les 8 voisins d'une case : les 4 droits d'abord, puis les 4 diagonaux.
const NEIGHBORS: [IVec2; 8] = [
    IVec2::new(1, 0),
    IVec2::new(-1, 0),
    IVec2::new(0, 1),
    IVec2::new(0, -1),
    IVec2::new(1, 1),
    IVec2::new(-1, 1),
    IVec2::new(1, -1),
    IVec2::new(-1, -1),
];

/// Le repère de la grille : où commencent les cases et quelle est leur
/// taille. Sert à placer les bâtiments sur les cases, avant même que la grille
/// complète existe (au démarrage du jeu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridFrame {
    pub origin: Vec2,
    pub cell_size: f32,
}

impl GridFrame {
    /// Le repère de la grille qui couvrira la carte `bounds`.
    pub fn new(bounds: Rect, settings: &NavSettings) -> Self {
        Self { origin: bounds.min, cell_size: settings.cell_size.max(0.05) }
    }

    /// Rectangle (en mètres) couvert par un bloc de `size` cases dont le coin
    /// bas-gauche est la case `min_cell`.
    pub fn rect(&self, min_cell: IVec2, size: UVec2) -> Rect {
        let min = self.origin + min_cell.as_vec2() * self.cell_size;
        Rect::from_corners(min, min + size.as_vec2() * self.cell_size)
    }

    /// Case du coin d'un bloc de `size` cases centré au plus près de `point` :
    /// c'est ainsi qu'un bâtiment « s'aligne sur la grille » sous le pointeur.
    pub fn min_cell_centered(&self, point: Vec2, size: UVec2) -> IVec2 {
        ((point - self.origin) / self.cell_size - size.as_vec2() / 2.0).round().as_ivec2()
    }
}

/// La grille de navigation : cases bloquées et dégagement de chaque case.
///
/// Les cases sont repérées par leurs coordonnées entières (colonne, ligne)
/// dans un `IVec2` (vecteur de deux entiers signés), la ligne suivant l'axe z.
#[derive(Resource, Debug, Clone)]
pub struct NavGrid {
    origin: Vec2,
    cell_size: f32,
    columns: i32,
    rows: i32,
    blocked: Vec<bool>,
    /// Cases où l'on ne construit pas (rampes), en plus des cases bloquées.
    unbuildable: Vec<bool>,
    /// Niveau du relief de chaque case (voir [`Terrain`]).
    levels: Vec<u8>,
    /// Distance (mètres) du centre de chaque case à l'obstacle le plus proche
    /// ou au bord de la carte, plafonnée à `MAX_CLEARANCE`. 0 pour une case bloquée.
    clearance: Vec<f32>,
    /// Tolérance (mètres) : une unité de rayon `r` passe par une case dont le
    /// dégagement vaut au moins `r - slack`. Les centres de cases ne tombent
    /// pas pile au milieu des passages : sans tolérance, une unité de 0,6 m ne
    /// passerait pas dans un couloir de 2 m.
    slack: f32,
    /// Augmente à chaque changement d'obstacles : les chemins calculés avec
    /// une version plus ancienne doivent être revérifiés.
    version: u32,
}

/// Au-delà de cette distance (mètres), le dégagement exact n'importe plus :
/// aucune unité n'est aussi large.
const MAX_CLEARANCE: f32 = 4.0;

impl NavGrid {
    /// Grille vide (sans obstacle) couvrant `bounds`.
    pub fn new(bounds: Rect, cell_size: f32) -> Self {
        let cell_size = cell_size.max(0.05);
        let columns = ((bounds.width() / cell_size).ceil() as i32).max(1);
        let rows = ((bounds.height() / cell_size).ceil() as i32).max(1);
        let count = (columns * rows) as usize;
        let mut grid = Self {
            origin: bounds.min,
            cell_size,
            columns,
            rows,
            blocked: vec![false; count],
            unbuildable: vec![false; count],
            levels: vec![0; count],
            clearance: vec![0.0; count],
            slack: 0.25 * cell_size,
            version: 0,
        };
        grid.set_obstacles(std::iter::empty());
        grid
    }

    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    pub fn origin(&self) -> Vec2 {
        self.origin
    }

    /// Nombre de colonnes et de lignes.
    pub fn size(&self) -> IVec2 {
        IVec2::new(self.columns, self.rows)
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn frame(&self) -> GridFrame {
        GridFrame { origin: self.origin, cell_size: self.cell_size }
    }

    /// Recopie du relief ce qui ne bouge pas : les rampes (non
    /// constructibles) et le niveau de chaque case. Les falaises, elles,
    /// sont des obstacles comme les autres.
    pub fn set_terrain(&mut self, terrain: &Terrain) {
        for index in 0..self.blocked.len() {
            let center = self.cell_center(self.cell_at(index));
            self.unbuildable[index] = terrain.is_ramp(center);
            self.levels[index] = terrain.level_at(center);
        }
    }

    /// Niveau du relief de la case (0 hors de la grille).
    pub fn level(&self, cell: IVec2) -> u8 {
        if self.in_bounds(cell) { self.levels[self.index(cell)] } else { 0 }
    }

    /// Recalcule les cases bloquées et le dégagement à partir des obstacles.
    pub fn set_obstacles(&mut self, obstacles: impl Iterator<Item = Rect>) {
        self.blocked.fill(false);
        for rect in obstacles {
            // Toutes les cases dont le carré chevauche vraiment le rectangle
            // (le simple contact par un bord ne compte pas : la petite marge
            // `1e-3` évite qu'un bâtiment aligné sur la grille déborde d'une case).
            let min = ((rect.min - self.origin) / self.cell_size + 1e-3).floor().as_ivec2();
            let max = ((rect.max - self.origin) / self.cell_size - 1e-3).ceil().as_ivec2() - 1;
            for y in min.y.max(0)..=max.y.min(self.rows - 1) {
                for x in min.x.max(0)..=max.x.min(self.columns - 1) {
                    let index = self.index(IVec2::new(x, y));
                    self.blocked[index] = true;
                }
            }
        }
        self.compute_clearance();
        // `wrapping_add` : repart de 0 au lieu de planter après 4 milliards de changements.
        self.version = self.version.wrapping_add(1);
    }

    /// Dégagement de chaque case : distance exacte du centre de la case au
    /// carré bloqué le plus proche, ou au bord de la carte. On ne regarde que
    /// les cases à moins de `MAX_CLEARANCE` : 10 000 cases × 81 voisines, un
    /// calcul instantané fait seulement quand les obstacles changent.
    fn compute_clearance(&mut self) {
        let reach = (MAX_CLEARANCE / self.cell_size).ceil() as i32 + 1;
        for y in 0..self.rows {
            for x in 0..self.columns {
                let cell = IVec2::new(x, y);
                let index = self.index(cell);
                if self.blocked[index] {
                    self.clearance[index] = 0.0;
                    continue;
                }
                // Distance au bord de la carte, en cases (le centre est à +0,5).
                let to_border = [x, y, self.columns - 1 - x, self.rows - 1 - y]
                    .into_iter()
                    .min()
                    .unwrap_or(0) as f32
                    + 0.5;
                let mut best = to_border.min(MAX_CLEARANCE / self.cell_size);
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        let other = cell + IVec2::new(dx, dy);
                        if !self.in_bounds(other) || !self.blocked[self.index(other)] {
                            continue;
                        }
                        // Distance du centre (0,5 ; 0,5) au carré voisin : on
                        // retire la demi-case du côté du carré, sans descendre sous 0.
                        let gap = (Vec2::new(dx as f32, dy as f32).abs() - 0.5).max(Vec2::ZERO);
                        best = best.min(gap.length());
                    }
                }
                self.clearance[index] = best * self.cell_size;
            }
        }
    }

    fn index(&self, cell: IVec2) -> usize {
        (cell.y * self.columns + cell.x) as usize
    }

    fn cell_at(&self, index: usize) -> IVec2 {
        IVec2::new(index as i32 % self.columns, index as i32 / self.columns)
    }

    pub fn in_bounds(&self, cell: IVec2) -> bool {
        cell.x >= 0 && cell.y >= 0 && cell.x < self.columns && cell.y < self.rows
    }

    /// Case qui contient le point `p` (ramenée dans la grille s'il est dehors).
    pub fn cell_of(&self, p: Vec2) -> IVec2 {
        let cell = ((p - self.origin) / self.cell_size).floor().as_ivec2();
        cell.clamp(IVec2::ZERO, IVec2::new(self.columns - 1, self.rows - 1))
    }

    /// Centre d'une case, en mètres.
    pub fn cell_center(&self, cell: IVec2) -> Vec2 {
        self.origin + (cell.as_vec2() + 0.5) * self.cell_size
    }

    pub fn is_blocked(&self, cell: IVec2) -> bool {
        !self.in_bounds(cell) || self.blocked[self.index(cell)]
    }

    /// Dégagement (mètres) d'une case ; 0 hors de la grille.
    pub fn clearance(&self, cell: IVec2) -> f32 {
        if self.in_bounds(cell) { self.clearance[self.index(cell)] } else { 0.0 }
    }

    /// Une unité de rayon `radius` peut-elle se tenir au centre de cette case ?
    pub fn passable(&self, cell: IVec2, radius: f32) -> bool {
        self.in_bounds(cell) && {
            let index = self.index(cell);
            !self.blocked[index] && self.clearance[index] + self.slack >= radius
        }
    }

    /// Toutes les cases du bloc sont-elles dans la carte, libres et
    /// constructibles (pas sur une rampe) ? (Sert à la pose des bâtiments.)
    pub fn area_free(&self, min_cell: IVec2, size: UVec2) -> bool {
        (0..size.y as i32).all(|dy| {
            (0..size.x as i32).all(|dx| {
                let cell = min_cell + IVec2::new(dx, dy);
                !self.is_blocked(cell) && !self.unbuildable[self.index(cell)]
            })
        })
    }

    /// Case praticable (pour `radius`) la plus proche de `cell`, en distance
    /// à vol d'oiseau ; à égalité, la première dans l'ordre des indices.
    ///
    /// On parcourt des carrés de plus en plus grands autour de `cell`. Un
    /// coin du carré `k` est à k√2 alors que le milieu du carré `k + 1` est à
    /// k + 1 : on ne s'arrête donc que quand plus aucun carré ne peut faire mieux.
    pub fn nearest_passable(&self, cell: IVec2, radius: f32) -> Option<IVec2> {
        let cell = cell.clamp(IVec2::ZERO, IVec2::new(self.columns - 1, self.rows - 1));
        if self.passable(cell, radius) {
            return Some(cell);
        }
        let mut best: Option<(i32, usize)> = None;
        let max_ring = self.columns.max(self.rows);
        for ring in 1..=max_ring {
            if let Some((best_d2, _)) = best
                && ring * ring > best_d2
            {
                break;
            }
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    // Seulement le contour du carré.
                    if dx.abs() != ring && dy.abs() != ring {
                        continue;
                    }
                    let other = cell + IVec2::new(dx, dy);
                    if !self.passable(other, radius) {
                        continue;
                    }
                    let key = (dx * dx + dy * dy, self.index(other));
                    if best.is_none_or(|b| key < b) {
                        best = Some(key);
                    }
                }
            }
        }
        best.map(|(_, index)| self.cell_at(index))
    }

    /// Point praticable le plus proche de `p` : `p` lui-même si sa case est
    /// praticable, sinon le centre de la case praticable la plus proche.
    pub fn nearest_passable_point(&self, p: Vec2, radius: f32) -> Option<Vec2> {
        let cell = self.cell_of(p);
        if self.passable(cell, radius) {
            return Some(p);
        }
        self.nearest_passable(cell, radius).map(|c| self.cell_center(c))
    }

    /// Vrai si une unité de rayon `radius` peut aller en ligne droite de `a` à
    /// `b` : toutes les cases traversées par le segment sont praticables (sauf
    /// celle de départ, où l'unité se trouve déjà).
    ///
    /// Parcours « supercover » : on avance case par case le long du segment
    /// (entre les centres des cases de `a` et de `b`) ; quand il passe pile
    /// par un coin, on vérifie aussi les deux cases qui touchent ce coin, pour
    /// ne jamais couper un angle de bâtiment.
    pub fn line_of_sight(&self, a: Vec2, b: Vec2, radius: f32) -> bool {
        let start = self.cell_of(a);
        let end = self.cell_of(b);
        let delta = end - start;
        let (nx, ny) = (delta.x.abs(), delta.y.abs());
        let step = delta.signum();
        let mut cell = start;
        let (mut ix, mut iy) = (0, 0);
        while ix < nx || iy < ny {
            // Signe de cette expression : le segment sort-il de la case par
            // un bord vertical (< 0), horizontal (> 0) ou par le coin (= 0) ?
            let decision = (1 + 2 * ix) * ny - (1 + 2 * iy) * nx;
            if decision == 0 {
                if !self.passable(cell + IVec2::new(step.x, 0), radius) || !self.passable(cell + IVec2::new(0, step.y), radius) {
                    return false;
                }
                cell += step;
                ix += 1;
                iy += 1;
            } else if decision < 0 {
                cell.x += step.x;
                ix += 1;
            } else {
                cell.y += step.y;
                iy += 1;
            }
            if !self.passable(cell, radius) {
                return false;
            }
        }
        true
    }
}

/// Tableaux de travail d'A*, gardés d'une recherche à l'autre pour ne pas
/// réallouer 10 000 cases à chaque fois.
///
/// Astuce du « tampon de génération » : au lieu de remettre tous les tableaux
/// à zéro avant chaque recherche, on note dans `visited[i]` le numéro de la
/// recherche qui a touché la case `i`. Une case dont le numéro n'est pas celui
/// de la recherche en cours est considérée comme jamais vue.
#[derive(Resource, Debug, Default)]
pub struct PathScratch {
    /// Meilleur coût connu depuis le départ (valide si `visited == generation`).
    cost: Vec<u32>,
    /// Case précédente sur le meilleur chemin connu.
    parent: Vec<u32>,
    visited: Vec<u32>,
    /// Case déjà « fermée » (son meilleur chemin est définitif).
    closed: Vec<u32>,
    generation: u32,
    /// La file d'attente des cases à explorer : un **tas binaire**
    /// (`BinaryHeap`), qui donne toujours son plus grand élément en premier.
    /// On y range des `Reverse(...)` : `Reverse` inverse l'ordre de
    /// comparaison, donc le tas donne le plus **petit** élément. Les éléments
    /// sont des tuples, comparés champ par champ (l'ordre `Ord` des tuples) :
    /// coût estimé total, puis distance restante, puis indice de la case.
    /// Le départage des égalités est donc fixe, donc déterministe.
    open: BinaryHeap<Reverse<(u32, u32, u32)>>,
}

impl PathScratch {
    /// Prépare une nouvelle recherche sur une grille de `count` cases.
    fn begin(&mut self, count: usize) {
        if self.cost.len() != count {
            self.cost = vec![0; count];
            self.parent = vec![0; count];
            self.visited = vec![0; count];
            self.closed = vec![0; count];
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // Après 4 milliards de recherches, les vieux numéros reviennent :
            // on remet tout à zéro une fois.
            self.visited.fill(0);
            self.closed.fill(0);
            self.generation = 1;
        }
        self.open.clear();
    }
}

/// Distance « octile » entre deux cases, en dixièmes de case : la longueur du
/// plus court trajet sur une grille à 8 directions sans obstacle (des pas en
/// diagonale, puis des pas droits). Elle ne surestime jamais la vraie
/// distance : c'est l'heuristique d'A*.
fn octile(a: IVec2, b: IVec2) -> u32 {
    let d = (a - b).abs();
    let (low, high) = (d.x.min(d.y) as u32, d.x.max(d.y) as u32);
    DIAGONAL_COST * low + STRAIGHT_COST * (high - low)
}

/// Résultat d'une recherche A* : les cases du chemin, du départ à l'arrivée.
#[derive(Debug, Clone, PartialEq)]
pub struct CellPath {
    pub cells: Vec<IVec2>,
    /// Faux si la case visée était inatteignable : le chemin mène alors à la
    /// case atteignable la plus proche d'elle.
    pub reached: bool,
    /// Nombre de cases explorées (pour le budget).
    pub expanded: usize,
}

/// A* de la case `start` à la case `goal` pour une unité de rayon `radius`.
/// `start` et `goal` doivent être praticables (voir [`NavGrid::nearest_passable`]).
pub fn find_cell_path(grid: &NavGrid, start: IVec2, goal: IVec2, radius: f32, scratch: &mut PathScratch) -> CellPath {
    scratch.begin(grid.blocked.len());
    let generation = scratch.generation;
    let start_index = grid.index(start);
    scratch.cost[start_index] = 0;
    scratch.parent[start_index] = start_index as u32;
    scratch.visited[start_index] = generation;
    let start_h = octile(start, goal);
    scratch.open.push(Reverse((start_h, start_h, start_index as u32)));

    // Case fermée la plus proche du but (distance restante, puis coût, puis
    // indice) : là où l'on s'arrête si le but est inatteignable.
    let mut closest = (start_h, 0u32, start_index as u32);
    let mut expanded = 0;
    let mut reached = false;

    while let Some(Reverse((_, h, index))) = scratch.open.pop() {
        let index = index as usize;
        if scratch.closed[index] == generation {
            // Déjà fermée par un meilleur chemin (une case peut être dans le
            // tas plusieurs fois : on n'enlève pas les anciennes entrées).
            continue;
        }
        scratch.closed[index] = generation;
        expanded += 1;
        let cost = scratch.cost[index];
        if (h, cost, index as u32) < closest {
            closest = (h, cost, index as u32);
        }
        let cell = grid.cell_at(index);
        if cell == goal {
            reached = true;
            break;
        }
        for (n, offset) in NEIGHBORS.iter().enumerate() {
            let next = cell + *offset;
            if !grid.passable(next, radius) {
                continue;
            }
            let diagonal = n >= 4;
            // Pas de coupe de coin : une diagonale n'est permise que si les
            // deux cases droites qu'elle longe sont praticables.
            if diagonal
                && (!grid.passable(cell + IVec2::new(offset.x, 0), radius)
                    || !grid.passable(cell + IVec2::new(0, offset.y), radius))
            {
                continue;
            }
            let next_index = grid.index(next);
            if scratch.closed[next_index] == generation {
                continue;
            }
            let next_cost = cost + if diagonal { DIAGONAL_COST } else { STRAIGHT_COST };
            if scratch.visited[next_index] == generation && scratch.cost[next_index] <= next_cost {
                continue;
            }
            scratch.visited[next_index] = generation;
            scratch.cost[next_index] = next_cost;
            scratch.parent[next_index] = index as u32;
            let next_h = octile(next, goal);
            scratch.open.push(Reverse((next_cost + next_h, next_h, next_index as u32)));
        }
    }

    // On remonte les parents depuis l'arrivée (ou la case la plus proche).
    let end = if reached { grid.index(goal) } else { closest.2 as usize };
    let mut cells = vec![grid.cell_at(end)];
    let mut current = end;
    while current != start_index {
        current = scratch.parent[current] as usize;
        cells.push(grid.cell_at(current));
    }
    cells.reverse();
    CellPath { cells, reached, expanded }
}

/// Lisse une suite de points en « tirant la ficelle » : on garde le premier
/// point, puis on saute au point le plus lointain qu'on voit encore en ligne
/// droite, et ainsi de suite. Renvoie les points de passage **après** le premier.
pub fn smooth(grid: &NavGrid, points: &[Vec2], radius: f32) -> Vec<Vec2> {
    let mut result = Vec::new();
    if points.len() < 2 {
        return result;
    }
    let mut anchor = points[0];
    let mut i = 1;
    while i < points.len() - 1 {
        // Si le point suivant n'est pas visible depuis l'ancre, le point
        // courant devient un virage obligé.
        if !grid.line_of_sight(anchor, points[i + 1], radius) {
            result.push(points[i]);
            anchor = points[i];
        }
        i += 1;
    }
    result.push(points[points.len() - 1]);
    result
}

/// Un chemin prêt à suivre, pour une unité.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedPath {
    /// Points de passage, le dernier étant la destination finale.
    pub waypoints: Vec<Vec2>,
    /// Destination finale : la cible demandée, ou le point atteignable le plus
    /// proche si elle est dans un obstacle ou inatteignable.
    pub goal: Vec2,
    pub expanded: usize,
}

/// Chemin brut, avant lissage : `start`, les centres des cases du chemin,
/// puis la destination retenue. Renvoie aussi cette destination et le
/// nombre de cases explorées. `None` si aucune case n'est praticable.
fn raw_path(grid: &NavGrid, start: Vec2, goal: Vec2, radius: f32, scratch: &mut PathScratch) -> Option<(Vec<Vec2>, Vec2, usize)> {
    let start_cell = grid.nearest_passable(grid.cell_of(start), radius)?;
    let goal_cell = grid.nearest_passable(grid.cell_of(goal), radius)?;
    // Raccourci : la cible est visible en ligne droite, pas besoin d'A*.
    if grid.cell_of(goal) == goal_cell && grid.line_of_sight(start, goal, radius) {
        return Some((vec![start, goal], goal, 0));
    }
    let path = find_cell_path(grid, start_cell, goal_cell, radius, scratch);
    // Destination exacte si sa case est atteinte et praticable, sinon le
    // centre de la dernière case du chemin.
    let last_cell = *path.cells.last().unwrap_or(&start_cell);
    let end = if path.reached && grid.cell_of(goal) == goal_cell { goal } else { grid.cell_center(last_cell) };

    let mut points = Vec::with_capacity(path.cells.len() + 2);
    points.push(start);
    // Si l'unité n'est pas dans une case praticable (poussée contre un
    // bâtiment), elle rejoint d'abord le centre de la case de départ.
    if grid.cell_of(start) != start_cell {
        points.push(grid.cell_center(start_cell));
    }
    // Les cases entre la première et la dernière (exclues) : la dernière est
    // remplacée par la destination exacte.
    let between = path.cells.len().saturating_sub(2);
    points.extend(path.cells.iter().skip(1).take(between).map(|c| grid.cell_center(*c)));
    points.push(end);
    Some((points, end, path.expanded))
}

/// Chemin complet (A* puis lissage) d'une unité de rayon `radius` de `start`
/// à `goal`. `None` seulement si aucune case de la carte n'est praticable.
pub fn plan_path(grid: &NavGrid, start: Vec2, goal: Vec2, radius: f32, scratch: &mut PathScratch) -> Option<PlannedPath> {
    let (points, end, expanded) = raw_path(grid, start, goal, radius, scratch)?;
    Some(PlannedPath { waypoints: smooth(grid, &points, radius), goal: end, expanded })
}

/// Chemins de tout un groupe d'unités de même rayon, avec un seul A*.
///
/// `members` : (position, place visée) de chaque unité. L'unité la plus proche
/// du centre du groupe calcule un « couloir » vers le centre des places
/// visées ; chaque unité le rejoint en ligne droite, le suit, et le quitte en
/// ligne droite vers sa place. Renvoie `None` pour une unité qui ne voit pas
/// le couloir (derrière un mur) : elle aura besoin de son propre A*.
pub fn plan_group(
    grid: &NavGrid,
    members: &[(Vec2, Vec2)],
    radius: f32,
    scratch: &mut PathScratch,
) -> (Vec<Option<PlannedPath>>, usize) {
    if members.is_empty() {
        return (Vec::new(), 0);
    }
    let count = members.len() as f32;
    let start_center = members.iter().map(|m| m.0).sum::<Vec2>() / count;
    let goal_center = members.iter().map(|m| m.1).sum::<Vec2>() / count;
    // Le « guide » : l'unité la plus proche du centre (à égalité, la première).
    let leader = members
        .iter()
        .enumerate()
        .fold((0, f32::INFINITY), |best, (i, m)| {
            let d = m.0.distance_squared(start_center);
            if d < best.1 { (i, d) } else { best }
        })
        .0;
    // Le couloir n'est pas lissé : de case en case, il offre plus de points
    // où se raccrocher.
    let Some((mut corridor, corridor_end, expanded)) = raw_path(grid, members[leader].0, goal_center, radius, scratch) else {
        return (vec![None; members.len()], 0);
    };
    corridor.remove(0); // la position du guide
    // Le centre visé était-il atteignable ? Sinon, chacun s'en approche au
    // plus près : au bout du couloir.
    let center_reached = corridor_end.distance(goal_center) < grid.cell_size;

    let plans = members
        .iter()
        .map(|&(start, wanted)| {
            let goal = match grid.nearest_passable_point(wanted, radius) {
                Some(goal) if center_reached || grid.line_of_sight(corridor_end, goal, radius) => goal,
                _ => corridor_end,
            };
            // Cible visible directement : ligne droite.
            if grid.line_of_sight(start, goal, radius) {
                return Some(PlannedPath { waypoints: vec![goal], goal, expanded: 0 });
            }
            // On rejoint le couloir au point le plus avancé qu'on voit…
            if !grid.line_of_sight(start, corridor[0], radius) {
                return None;
            }
            let mut join = 0;
            while join + 1 < corridor.len() && grid.line_of_sight(start, corridor[join + 1], radius) {
                join += 1;
            }
            // … et on le quitte au premier point d'où l'on voit sa place.
            let last = corridor.len() - 1;
            if !grid.line_of_sight(corridor[last], goal, radius) {
                return None;
            }
            let mut leave = last;
            while leave > join && grid.line_of_sight(corridor[leave - 1], goal, radius) {
                leave -= 1;
            }
            let mut points = vec![start];
            points.extend_from_slice(&corridor[join..=leave]);
            points.push(goal);
            Some(PlannedPath { waypoints: smooth(grid, &points, radius), goal, expanded: 0 })
        })
        .collect();
    (plans, expanded)
}

// ---------------------------------------------------------------------------
// Intégration dans la simulation (ECS).
// ---------------------------------------------------------------------------

/// Le chemin que suit une unité au sol.
#[derive(Component, Debug, Clone)]
pub struct NavPath {
    pub waypoints: Vec<Vec2>,
    /// Indice du prochain point de passage.
    pub next: usize,
    /// Pour chaque point de passage, longueur du chemin qui reste après lui.
    remaining_after: Vec<f32>,
    /// Cible de l'ordre au moment du calcul, et destination retenue : tant
    /// que `MoveTarget` vaut l'une des deux, le chemin est à jour.
    pub ordered: Vec2,
    pub goal: Vec2,
    /// Version de la grille au moment du calcul.
    grid_version: u32,
}

impl NavPath {
    fn new(plan: PlannedPath, ordered: Vec2, grid_version: u32) -> Self {
        let mut remaining_after = vec![0.0; plan.waypoints.len()];
        for i in (0..plan.waypoints.len().saturating_sub(1)).rev() {
            remaining_after[i] = remaining_after[i + 1] + plan.waypoints[i].distance(plan.waypoints[i + 1]);
        }
        Self { waypoints: plan.waypoints, next: 0, remaining_after, ordered, goal: plan.goal, grid_version }
    }

    /// Ce chemin correspond-il encore à l'ordre `target` ?
    pub fn matches(&self, target: Vec2) -> bool {
        target == self.ordered || target == self.goal
    }

    /// Fait avancer le suivi du chemin pour une unité en `position`, et
    /// renvoie (point de passage visé, longueur restante après lui). Le point
    /// de passage est `None` sur le dernier tronçon : l'unité va droit à sa cible.
    ///
    /// On passe au point suivant quand on est assez près du point courant
    /// (`reach`), ou quand on voit déjà le suivant en ligne droite : une unité
    /// bousculée hors du chemin coupe ainsi au plus court.
    pub fn advance(&mut self, position: Vec2, radius: f32, reach: f32, grid: &NavGrid) -> (Option<Vec2>, f32) {
        let last = self.waypoints.len().saturating_sub(1);
        // Au plus deux sauts par pas : le test de ligne de vue a un coût.
        for _ in 0..2 {
            if self.next >= last {
                break;
            }
            let close = position.distance(self.waypoints[self.next]) <= reach;
            if close || grid.line_of_sight(position, self.waypoints[self.next + 1], radius) {
                self.next += 1;
            } else {
                break;
            }
        }
        if self.next >= last {
            (None, 0.0)
        } else {
            (Some(self.waypoints[self.next]), self.remaining_after[self.next])
        }
    }

    /// Le reste du chemin (depuis `position`) est-il toujours praticable ?
    fn still_valid(&self, position: Vec2, radius: f32, grid: &NavGrid) -> bool {
        let mut from = position;
        self.waypoints[self.next.min(self.waypoints.len())..].iter().all(|&p| {
            let ok = grid.line_of_sight(from, p, radius);
            from = p;
            ok
        })
    }
}

/// Unités d'un même ordre de groupe : elles partagent une recherche de chemin.
/// À insérer avec le `MoveTarget` de chaque unité (voir `selection`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MoveGroup(pub u32);

/// Numéro du prochain ordre de groupe.
#[derive(Resource, Debug, Default)]
pub struct MoveGroupCounter(u32);

impl MoveGroupCounter {
    pub fn next_group(&mut self) -> MoveGroup {
        self.0 = self.0.wrapping_add(1);
        MoveGroup(self.0)
    }
}

/// Marqueur : l'unité attend son chemin (elle est dans la file).
#[derive(Component, Debug)]
pub struct PathPending;

/// File des demandes de chemin, dans un ordre reproductible.
#[derive(Resource, Debug, Default)]
pub struct PathQueue {
    pending: Vec<Entity>,
}

/// Reconstruit la grille quand des obstacles apparaissent, bougent ou disparaissent.
pub(crate) fn rebuild_grid(
    mut commands: Commands,
    bounds: Res<MapBounds>,
    settings: Res<NavSettings>,
    grid: Option<ResMut<NavGrid>>,
    obstacles: Query<&StaticObstacle>,
    changed: Query<(), Changed<StaticObstacle>>,
    mut removed: RemovedComponents<StaticObstacle>,
    terrain: Res<Terrain>,
) {
    let removed_any = removed.read().count() > 0;
    match grid {
        Some(mut grid) => {
            if changed.is_empty() && !removed_any && !bounds.is_changed() && !terrain.is_changed() {
                return;
            }
            if bounds.is_changed() || grid.cell_size != settings.cell_size {
                *grid = NavGrid::new(bounds.0, settings.cell_size);
            }
            if terrain.is_changed() || bounds.is_changed() {
                grid.set_terrain(&terrain);
            }
            // L'ordre des obstacles n'importe pas : une case est bloquée dès
            // qu'un seul la recouvre.
            grid.set_obstacles(obstacles.iter().map(|o| o.0));
        }
        None => {
            let mut grid = NavGrid::new(bounds.0, settings.cell_size);
            grid.set_terrain(&terrain);
            grid.set_obstacles(obstacles.iter().map(|o| o.0));
            commands.insert_resource(grid);
        }
    }
}

/// Repère les unités au sol qui ont besoin d'un (nouveau) chemin : nouvel
/// ordre, ou obstacle apparu sur leur route.
pub(crate) fn request_paths(
    mut commands: Commands,
    grid: Option<Res<NavGrid>>,
    mut queue: ResMut<PathQueue>,
    mut units: Query<
        (Entity, &MoveTarget, &SimPosition, &Collider, Option<&mut NavPath>, Option<&MoveGroup>),
        Without<PathPending>,
    >,
) {
    let Some(grid) = grid else { return };
    let mut new_requests: Vec<(Option<MoveGroup>, Entity)> = Vec::new();
    for (entity, target, position, collider, path, group) in &mut units {
        if collider.layer != CollisionLayer::Ground {
            continue;
        }
        let needs_path = match path {
            None => true,
            Some(path) if !path.matches(target.0) => true,
            Some(mut path) => {
                if path.grid_version == grid.version() {
                    false
                } else if path.still_valid(position.current, collider.radius, &grid) {
                    path.grid_version = grid.version();
                    false
                } else {
                    // La route est coupée : nouvelle recherche, seule.
                    commands.entity(entity).remove::<MoveGroup>();
                    new_requests.push((None, entity));
                    commands.entity(entity).insert(PathPending);
                    continue;
                }
            }
        };
        if needs_path {
            new_requests.push((group.copied(), entity));
            commands.entity(entity).insert(PathPending);
        }
    }
    // Ordre reproductible : par groupe, puis par identifiant (les membres
    // d'un groupe se suivent ainsi dans la file).
    new_requests.sort_unstable();
    queue.pending.extend(new_requests.into_iter().map(|(_, entity)| entity));
}

/// Calcule les chemins en attente, dans la limite du budget de ce pas.
pub(crate) fn compute_paths(
    mut commands: Commands,
    grid: Option<Res<NavGrid>>,
    settings: Res<NavSettings>,
    mut queue: ResMut<PathQueue>,
    mut scratch: ResMut<PathScratch>,
    mut units: Query<(&mut MoveTarget, &SimPosition, &Collider, Option<&MoveGroup>), With<PathPending>>,
) {
    let Some(grid) = grid else { return };
    let mut spent = 0;
    let mut done = 0;
    while done < queue.pending.len() && (spent < settings.expansion_budget || done == 0) {
        // Le lot : cette demande, plus les suivantes du même groupe et du même rayon.
        let first = queue.pending[done];
        let Ok((target, position, collider, group)) = units.get(first) else {
            // Unité disparue ou ordre annulé. `get_entity` échoue si l'entité
            // n'existe plus ; `try_remove` ne fait rien si elle disparaît d'ici
            // à l'application de la commande.
            if let Ok(mut entity) = commands.get_entity(first) {
                entity.try_remove::<PathPending>();
            }
            done += 1;
            continue;
        };
        let (group, radius) = (group.copied(), collider.radius);
        let mut batch = vec![(first, position.current, target.0)];
        if group.is_some() {
            for &other in &queue.pending[done + 1..] {
                match units.get(other) {
                    Ok((t, p, c, g)) if g.copied() == group && c.radius == radius => batch.push((other, p.current, t.0)),
                    _ => break,
                }
            }
        }
        done += batch.len();

        let mut results: Vec<Option<PlannedPath>> = if batch.len() > 1 {
            let members: Vec<(Vec2, Vec2)> = batch.iter().map(|b| (b.1, b.2)).collect();
            let (plans, expanded) = plan_group(&grid, &members, radius, &mut scratch);
            spent += expanded;
            plans
        } else {
            vec![None]
        };
        for ((entity, start, ordered), result) in batch.iter().zip(results.iter_mut()) {
            // Les unités qui n'ont pas pu suivre le couloir (et les unités
            // seules) font leur propre recherche.
            let plan = match result.take() {
                Some(plan) => Some(plan),
                None => plan_path(&grid, *start, *ordered, radius, &mut scratch).inspect(|p| spent += p.expanded),
            };
            let mut entity_commands = commands.entity(*entity);
            entity_commands.remove::<PathPending>();
            let Some(plan) = plan else {
                entity_commands.remove::<(MoveTarget, NavPath)>();
                continue;
            };
            // Cible inatteignable ou dans un obstacle : l'ordre devient
            // « aller au plus près ».
            if plan.goal != *ordered
                && let Ok((mut target, ..)) = units.get_mut(*entity)
            {
                target.0 = plan.goal;
            }
            entity_commands.insert(NavPath::new(plan, *ordered, grid.version()));
        }
    }
    queue.pending.drain(..done);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_grid() -> NavGrid {
        NavGrid::new(Rect::new(-50.0, -50.0, 50.0, 50.0), 1.0)
    }

    /// Longueur totale d'un chemin depuis `start`.
    fn length(start: Vec2, waypoints: &[Vec2]) -> f32 {
        let mut from = start;
        waypoints
            .iter()
            .map(|&p| {
                let d = from.distance(p);
                from = p;
                d
            })
            .sum()
    }

    /// Vérifie qu'aucun segment du chemin ne traverse une case impraticable.
    fn assert_walkable(grid: &NavGrid, start: Vec2, waypoints: &[Vec2], radius: f32) {
        let mut from = start;
        for &p in waypoints {
            assert!(grid.line_of_sight(from, p, radius), "segment {from} → {p} impraticable");
            from = p;
        }
    }

    #[test]
    fn straight_line_without_obstacle() {
        let grid = open_grid();
        let mut scratch = PathScratch::default();
        let plan = plan_path(&grid, Vec2::new(-20.0, 0.0), Vec2::new(20.0, 3.0), 0.5, &mut scratch).unwrap();
        assert_eq!(plan.waypoints, vec![Vec2::new(20.0, 3.0)]);
    }

    #[test]
    fn path_goes_around_a_wall() {
        let mut grid = open_grid();
        // Un mur vertical de 20 m entre le départ et l'arrivée.
        grid.set_obstacles([Rect::new(-1.0, -10.0, 1.0, 10.0)].into_iter());
        let (start, goal) = (Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0));
        let mut scratch = PathScratch::default();
        let plan = plan_path(&grid, start, goal, 0.5, &mut scratch).unwrap();
        assert_eq!(*plan.waypoints.last().unwrap(), goal);
        assert!(plan.waypoints.len() >= 2, "le chemin devrait contourner le mur");
        assert_walkable(&grid, start, &plan.waypoints, 0.5);
        // Le détour passe par un bout du mur : un peu plus de 2 × √(10² + 10²).
        let len = length(start, &plan.waypoints);
        assert!(len > 28.0 && len < 32.0, "longueur inattendue : {len}");
    }

    #[test]
    fn corridor_too_narrow_for_a_big_unit() {
        let mut grid = open_grid();
        // Deux murs laissent un couloir de 2 m (x de -1 à 1) ; tout le reste est fermé.
        grid.set_obstacles([Rect::new(-50.0, -2.0, -1.0, 2.0), Rect::new(1.0, -2.0, 50.0, 2.0)].into_iter());
        let (start, goal) = (Vec2::new(0.0, -10.0), Vec2::new(0.0, 10.0));
        let mut scratch = PathScratch::default();
        // L'infanterie passe…
        let infantry = plan_path(&grid, start, goal, 0.5, &mut scratch).unwrap();
        assert_eq!(infantry.goal, goal);
        assert_walkable(&grid, start, &infantry.waypoints, 0.5);
        // … le Bastion (1,8 m) ne passe pas : il s'approche au plus près, devant le couloir.
        let tank = plan_path(&grid, start, goal, 1.8, &mut scratch).unwrap();
        assert!(tank.goal.y < -2.0, "le Bastion n'aurait pas dû franchir le couloir : {}", tank.goal);
        assert!(tank.goal.distance(Vec2::new(0.0, -2.0)) < 3.5, "il devrait s'arrêter devant : {}", tank.goal);
    }

    #[test]
    fn target_inside_an_obstacle_goes_to_nearest_free_cell() {
        let mut grid = open_grid();
        grid.set_obstacles([Rect::new(0.0, 0.0, 6.0, 6.0)].into_iter());
        let mut scratch = PathScratch::default();
        let plan = plan_path(&grid, Vec2::new(-10.0, 1.5), Vec2::new(1.5, 1.5), 0.5, &mut scratch).unwrap();
        // Cases libres les plus proches de (1,5 ; 1,5) : juste à gauche du
        // carré ou juste en dessous, à 2 m. À égalité, l'ordre des cases
        // départage : (1,5 ; −0,5), la ligne la plus basse.
        assert_eq!(plan.goal, Vec2::new(1.5, -0.5));
    }

    #[test]
    fn unreachable_target_gets_as_close_as_possible() {
        let mut grid = open_grid();
        // Une enceinte fermée autour de (20, 0).
        grid.set_obstacles(
            [
                Rect::new(14.0, -6.0, 26.0, -5.0),
                Rect::new(14.0, 5.0, 26.0, 6.0),
                Rect::new(14.0, -5.0, 15.0, 5.0),
                Rect::new(25.0, -5.0, 26.0, 5.0),
            ]
            .into_iter(),
        );
        let mut scratch = PathScratch::default();
        let plan = plan_path(&grid, Vec2::new(-10.0, 0.0), Vec2::new(20.0, 0.0), 0.5, &mut scratch).unwrap();
        // On s'arrête contre l'enceinte, dehors, au plus près de la cible.
        let enclosure = Rect::new(14.0, -6.0, 26.0, 6.0);
        assert!(!enclosure.contains(plan.goal), "arrivée dans l'enceinte : {}", plan.goal);
        assert!(plan.goal.distance(Vec2::new(20.5, 0.5)) <= 6.0 + 1e-4, "arrivée trop loin : {}", plan.goal);
    }

    #[test]
    fn pathfinding_is_deterministic() {
        let mut grid = open_grid();
        grid.set_obstacles([Rect::new(-1.0, -10.0, 1.0, 10.0), Rect::new(5.0, -3.0, 12.0, 3.0)].into_iter());
        let mut a = PathScratch::default();
        let mut b = PathScratch::default();
        for i in 0..20 {
            let start = Vec2::new(-15.0, i as f32 - 10.0);
            let goal = Vec2::new(20.0, 10.0 - i as f32);
            let first = plan_path(&grid, start, goal, 0.5, &mut a).unwrap();
            let second = plan_path(&grid, start, goal, 0.5, &mut b).unwrap();
            assert_eq!(first, second);
        }
    }

    #[test]
    fn no_corner_cutting() {
        let mut grid = open_grid();
        // Deux blocs qui ne se touchent que par un coin : la diagonale entre eux est interdite.
        grid.set_obstacles([Rect::new(0.0, 0.0, 1.0, 1.0), Rect::new(1.0, 1.0, 2.0, 2.0)].into_iter());
        assert!(!grid.line_of_sight(Vec2::new(1.5, 0.5), Vec2::new(0.5, 1.5), 0.1));
        let mut scratch = PathScratch::default();
        let path = find_cell_path(&grid, grid.cell_of(Vec2::new(1.5, 0.5)), grid.cell_of(Vec2::new(0.5, 1.5)), 0.1, &mut scratch);
        assert!(path.cells.len() > 2, "le chemin a coupé le coin : {:?}", path.cells);
    }

    #[test]
    fn group_shares_one_search() {
        let mut grid = open_grid();
        grid.set_obstacles([Rect::new(-1.0, -12.0, 1.0, 12.0)].into_iter());
        let members: Vec<(Vec2, Vec2)> = (0..100)
            .map(|i| {
                let offset = Vec2::new((i % 10) as f32, (i / 10) as f32) * 1.2;
                (Vec2::new(-20.0, -6.0) + offset, Vec2::new(10.0, -6.0) + offset)
            })
            .collect();
        let mut scratch = PathScratch::default();
        let (plans, _) = plan_group(&grid, &members, 0.5, &mut scratch);
        let shared = plans.iter().filter(|p| p.is_some()).count();
        assert!(shared >= 95, "seulement {shared} unités sur 100 ont pu suivre le couloir commun");
        for ((start, goal), plan) in members.iter().zip(&plans) {
            if let Some(plan) = plan {
                assert_eq!(plan.goal, *goal);
                assert_walkable(&grid, *start, &plan.waypoints, 0.5);
            }
        }
    }

    #[test]
    fn three_hundred_requests_are_fast() {
        // Un champ d'obstacles et 300 recherches individuelles d'un bord à l'autre.
        let mut grid = open_grid();
        let walls: Vec<Rect> = (0..8)
            .map(|i| {
                let x = -35.0 + i as f32 * 10.0;
                if i % 2 == 0 { Rect::new(x, -45.0, x + 2.0, 30.0) } else { Rect::new(x, -30.0, x + 2.0, 45.0) }
            })
            .collect();
        grid.set_obstacles(walls.into_iter());
        let mut scratch = PathScratch::default();
        let started = std::time::Instant::now();
        let mut expanded = 0;
        for i in 0..300 {
            let y = (i % 60) as f32 * 1.5 - 45.0;
            let plan = plan_path(&grid, Vec2::new(-48.0, y), Vec2::new(48.0, -y), 0.5, &mut scratch).unwrap();
            expanded += plan.expanded;
        }
        let elapsed = started.elapsed();
        println!("300 recherches : {elapsed:?}, {expanded} cases explorées");
        // Large marge : les tests tournent sans les optimisations du mode release.
        assert!(elapsed.as_secs_f32() < 5.0, "trop lent : {elapsed:?}");
    }

    /// Le vrai trajet du jeu, sans fenêtre : chemins de groupe, suivi de
    /// chemin et collisions. 40 unités contournent un bâtiment de 7 × 7 m.
    #[test]
    fn group_walks_around_a_building() {
        use crate::collision::{step, Agent, CollisionSettings, Steering};
        let bounds = Rect::new(-50.0, -50.0, 50.0, 50.0);
        let building = Rect::new(-3.0, -3.0, 4.0, 4.0);
        let mut grid = NavGrid::new(bounds, 1.0);
        grid.set_obstacles([building].into_iter());

        let radii = [0.5, 0.5, 0.6, 0.45];
        let mut agents: Vec<Agent> = (0..40)
            .map(|i| Agent {
                position: Vec2::new(-14.0 + (i % 8) as f32 * 1.3, -4.0 + (i / 8) as f32 * 1.3),
                radius: radii[i % radii.len()],
                mass: 1.0,
                speed: 4.0,
                target: None,
                waypoint: None,
                remaining: 0.0,
                hold: false,
                layer: CollisionLayer::Ground,
                steering: Steering::default(),
            })
            .collect();
        let slots = crate::selection::formation(&agents.iter().map(|a| a.radius).collect::<Vec<_>>(), Vec2::new(12.0, 0.5));
        let mut scratch = PathScratch::default();
        let mut paths: Vec<Option<NavPath>> = Vec::new();
        // Une recherche par rayon, comme dans `compute_paths`.
        for radius in radii {
            let members: Vec<usize> = (0..agents.len()).filter(|&i| agents[i].radius == radius).collect();
            let pairs: Vec<(Vec2, Vec2)> = members.iter().map(|&i| (agents[i].position, slots[i])).collect();
            let (plans, _) = plan_group(&grid, &pairs, radius, &mut scratch);
            paths.resize(agents.len(), None);
            for (&i, plan) in members.iter().zip(plans) {
                let plan = plan.or_else(|| plan_path(&grid, agents[i].position, slots[i], radius, &mut scratch)).unwrap();
                agents[i].target = Some(plan.goal);
                paths[i] = Some(NavPath::new(plan, slots[i], grid.version()));
            }
        }
        let settings = CollisionSettings::default();
        for _ in 0..32 * 20 {
            for (agent, path) in agents.iter_mut().zip(&mut paths) {
                if let (Some(_), Some(path)) = (agent.target, path.as_mut()) {
                    (agent.waypoint, agent.remaining) = path.advance(agent.position, agent.radius, agent.radius.max(4.0 / 32.0), &grid);
                }
            }
            step(&mut agents, 1.0 / 32.0, bounds, &[building], &settings);
            for a in &agents {
                assert!(
                    crate::collision::circle_rect_push(a.position, a.radius - 0.05, building).is_none(),
                    "une unité est entrée dans le bâtiment : {}",
                    a.position
                );
            }
        }
        let moving = agents.iter().filter(|a| a.target.is_some()).count();
        assert_eq!(moving, 0, "{moving} unités sont encore en marche");
        let far = agents.iter().filter(|a| a.position.x < 6.0).count();
        assert_eq!(far, 0, "{far} unités sont restées de l'autre côté du bâtiment");
    }

    #[test]
    fn clearance_measures_distance_to_obstacles() {
        let mut grid = open_grid();
        grid.set_obstacles([Rect::new(0.0, 0.0, 2.0, 2.0)].into_iter());
        assert_eq!(grid.clearance(grid.cell_of(Vec2::new(1.0, 1.0))), 0.0);
        assert_eq!(grid.clearance(grid.cell_of(Vec2::new(-0.5, 0.5))), 0.5);
        assert_eq!(grid.clearance(grid.cell_of(Vec2::new(-2.5, 0.5))), 2.5);
        // En diagonale du coin (0, 0) : distance √(1,5² + 1,5²).
        let diagonal = grid.clearance(grid.cell_of(Vec2::new(-1.5, -1.5)));
        assert!((diagonal - 4.5f32.sqrt()).abs() < 1e-5);
    }
}
