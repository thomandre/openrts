//! Relief de la carte : des plateaux à plusieurs niveaux, séparés par des
//! falaises infranchissables et reliés par des rampes (comme dans
//! StarCraft 2).
//!
//! # Deux lectures du même relief
//!
//! - **La simulation** ne connaît que des cases de 1 m : chaque case a un
//!   *niveau* (0 en bas, 1, 2…) et une hauteur logique (le niveau fois
//!   [`Terrain::level_height`], ou une pente régulière sur une rampe). Une
//!   case qui domine une voisine de plus de [`MAX_STEP`] est une **falaise** :
//!   elle devient un obstacle fixe ([`Cliff`] + [`StaticObstacle`]), que la
//!   navigation contourne et les collisions repoussent, comme un bâtiment.
//!   Les rampes ne sont pas constructibles. Ces calculs n'utilisent que des
//!   additions, des multiplications et des comparaisons : le résultat est le
//!   même sur toutes les machines (lockstep).
//! - **L'affichage** lit une grille plus fine (50 cm) et adoucie : les bords
//!   de plateau ondulent un peu (bruit), les falaises sont des pentes raides
//!   aux faces rocheuses. [`Terrain::height_at`] donne la hauteur du sol
//!   affiché sous n'importe quel point : les unités et les bâtiments y sont
//!   posés ([`OnTerrain`]), et la souris vise ce sol-là ([`Terrain::ray_hit`]).
//!
//! # Vision
//!
//! On ne voit pas plus haut que soi : une unité au sol ne voit pas les cases
//! d'un niveau supérieur au sien (voir [`crate::vision`]). Il faut monter, ou
//! un volant, pour voir ce qu'il y a sur un plateau.
//!
//! Sans relief (la ressource par défaut), tout est au niveau 0 et rien ne
//! change par rapport à un sol plat.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::collision::{MapBounds, StaticObstacle};
use crate::simulation::{SimPosition, SimSet};

/// Écart de hauteur (mètres) au-delà duquel on ne passe plus d'une case à sa
/// voisine : la case haute est une falaise. Une rampe de 7,5 m pour un
/// niveau de 3 m monte de 0,4 m par case (0,57 en diagonale) : elle passe.
pub const MAX_STEP: f32 = 0.9;

/// Hauteur d'un niveau par défaut (mètres) : presque deux fantassins.
pub const DEFAULT_LEVEL_HEIGHT: f32 = 3.0;

/// Pas de la grille d'affichage (mètres).
const DISPLAY_STEP: f32 = 0.5;

/// L'affichage déborde de la carte de cette distance (mètres) : un plateau
/// qui touche le bord continue un peu, puis redescend en douceur.
const DISPLAY_MARGIN: f32 = 24.0;

/// Un plateau : un polygone (sommets dans l'ordre, en x, z) élevé au niveau
/// `level`. Les plateaux peuvent s'emboîter : un point prend le plus haut
/// niveau des plateaux qui le contiennent.
#[derive(Debug, Clone, PartialEq)]
pub struct Plateau {
    pub outline: Vec<Vec2>,
    pub level: u8,
}

impl Plateau {
    pub fn new(level: u8, outline: impl Into<Vec<Vec2>>) -> Self {
        Self { outline: outline.into(), level }
    }

    /// Le point est-il dans le polygone ? (Règle pair-impair : on compte les
    /// côtés que croise une demi-droite partant du point vers +x.)
    fn contains(&self, p: Vec2) -> bool {
        let mut inside = false;
        let n = self.outline.len();
        for i in 0..n {
            let (a, b) = (self.outline[i], self.outline[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                inside = !inside;
            }
        }
        inside
    }
}

/// Une rampe : une bande droite de largeur `width`, du point bas `bottom`
/// au point haut `top`. La hauteur y monte régulièrement du niveau du sol
/// en `bottom` à celui du sol en `top`. Pour une rampe bien large, la faire
/// déborder de 3 m de part et d'autre du bord du plateau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ramp {
    pub bottom: Vec2,
    pub top: Vec2,
    pub width: f32,
}

impl Ramp {
    /// Avancement (0 en bas, 1 en haut) du point `p` sur la rampe, s'il est dessus.
    fn progress(&self, p: Vec2) -> Option<f32> {
        let axis = self.top - self.bottom;
        let length2 = axis.length_squared();
        if length2 <= 0.0 {
            return None;
        }
        let t = (p - self.bottom).dot(axis) / length2;
        // Écart à l'axe, au carré, comparé sans racine.
        let across = (p - self.bottom).perp_dot(axis);
        let half = self.width / 2.0;
        ((0.0..=1.0).contains(&t) && across * across <= half * half * length2).then_some(t)
    }
}

/// Le relief de la carte (voir la doc du module). Ressource : le jeu la
/// remplace par la sienne (`insert_resource`) avant le début de la partie.
#[derive(Resource, Debug, Clone)]
pub struct Terrain {
    bounds: Rect,
    level_height: f32,
    plateaus: Vec<Plateau>,
    ramps: Vec<Ramp>,
    // --- Simulation : cases de 1 m couvrant `bounds` ---
    columns: i32,
    rows: i32,
    levels: Vec<u8>,
    heights: Vec<f32>,
    on_ramp: Vec<bool>,
    cliff: Vec<bool>,
    // --- Affichage ---
    display: HeightField,
}

impl Default for Terrain {
    /// Sol plat (aucun plateau) sur la carte par défaut.
    fn default() -> Self {
        Self::new(MapBounds::default().0, DEFAULT_LEVEL_HEIGHT, Vec::new(), Vec::new())
    }
}

/// Les hauteurs affichées, sur une grille régulière (sommets).
#[derive(Debug, Clone)]
struct HeightField {
    origin: Vec2,
    step: f32,
    columns: usize,
    rows: usize,
    heights: Vec<f32>,
    /// Raideur de chaque sommet (0 : plat, 1 : falaise), pour les couleurs.
    steepness: Vec<f32>,
    /// Ombre au pied des falaises (0 : aucune, jusqu'à 0,5).
    foot: Vec<f32>,
    max_height: f32,
}

impl Terrain {
    /// Le relief de la carte `bounds` : ses plateaux et ses rampes.
    pub fn new(bounds: Rect, level_height: f32, plateaus: Vec<Plateau>, ramps: Vec<Ramp>) -> Self {
        let size = bounds.size().ceil().as_ivec2().max(IVec2::ONE);
        let mut terrain = Self {
            bounds,
            level_height,
            plateaus,
            ramps,
            columns: size.x,
            rows: size.y,
            levels: Vec::new(),
            heights: Vec::new(),
            on_ramp: Vec::new(),
            cliff: Vec::new(),
            display: HeightField { origin: Vec2::ZERO, step: 1.0, columns: 0, rows: 0, heights: Vec::new(), steepness: Vec::new(), foot: Vec::new(), max_height: 0.0 },
        };
        terrain.compute_cells();
        terrain.display = terrain.compute_display();
        terrain
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn level_height(&self) -> f32 {
        self.level_height
    }

    /// Y a-t-il du relief ? (Sinon, tout est plat à y = 0.)
    pub fn is_flat(&self) -> bool {
        self.plateaus.is_empty()
    }

    /// Niveau des plateaux au point `p` (avant les rampes).
    fn plateau_level(&self, p: Vec2) -> u8 {
        self.plateaus.iter().filter(|plateau| plateau.contains(p)).map(|plateau| plateau.level).max().unwrap_or(0)
    }

    /// Hauteur logique et niveau au point `p` : celui des plateaux, ou, sur
    /// une rampe, la pente entre ses deux bouts. On est sur le niveau haut
    /// d'une rampe à partir de sa moitié.
    fn logical(&self, p: Vec2) -> (f32, u8, bool) {
        for ramp in &self.ramps {
            if let Some(t) = ramp.progress(p) {
                let (low, high) = (self.plateau_level(ramp.bottom), self.plateau_level(ramp.top));
                let level = low as f32 + (high as f32 - low as f32) * t;
                return (level * self.level_height, if t < 0.5 { low } else { high }, true);
            }
        }
        let level = self.plateau_level(p);
        (level as f32 * self.level_height, level, false)
    }

    fn compute_cells(&mut self) {
        let count = (self.columns * self.rows) as usize;
        self.levels = vec![0; count];
        self.heights = vec![0.0; count];
        self.on_ramp = vec![false; count];
        self.cliff = vec![false; count];
        if self.is_flat() {
            return;
        }
        for y in 0..self.rows {
            for x in 0..self.columns {
                let i = (y * self.columns + x) as usize;
                let (height, level, ramp) = self.logical(self.cell_center(IVec2::new(x, y)));
                (self.heights[i], self.levels[i], self.on_ramp[i]) = (height, level, ramp);
            }
        }
        // Une case est une falaise si elle domine trop une de ses 8 voisines.
        for y in 0..self.rows {
            for x in 0..self.columns {
                let i = (y * self.columns + x) as usize;
                self.cliff[i] = (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| {
                        let (nx, ny) = (x + dx, y + dy);
                        nx >= 0 && ny >= 0 && nx < self.columns && ny < self.rows && {
                            let j = (ny * self.columns + nx) as usize;
                            self.heights[i] - self.heights[j] > MAX_STEP
                        }
                    })
                });
            }
        }
    }

    fn cell_center(&self, cell: IVec2) -> Vec2 {
        self.bounds.min + cell.as_vec2() + 0.5
    }

    /// La case (de 1 m) qui contient `p`, si elle est dans la carte.
    fn cell_index(&self, p: Vec2) -> Option<usize> {
        let cell = (p - self.bounds.min).floor().as_ivec2();
        (cell.x >= 0 && cell.y >= 0 && cell.x < self.columns && cell.y < self.rows).then(|| (cell.y * self.columns + cell.x) as usize)
    }

    /// Niveau du sol au point `p` (0 hors de la carte).
    pub fn level_at(&self, p: Vec2) -> u8 {
        self.cell_index(p).map_or(0, |i| self.levels[i])
    }

    /// Le point `p` est-il sur une rampe ? (On n'y construit pas.)
    pub fn is_ramp(&self, p: Vec2) -> bool {
        self.cell_index(p).is_some_and(|i| self.on_ramp[i])
    }

    /// Le point `p` est-il sur une falaise (infranchissable) ?
    pub fn is_cliff(&self, p: Vec2) -> bool {
        self.cell_index(p).is_some_and(|i| self.cliff[i])
    }

    /// Les falaises, regroupées en rectangles : les cases d'une même ligne
    /// qui se suivent forment une bande, et les bandes identiques de lignes
    /// consécutives fusionnent. Quelques centaines de rectangles au plus.
    pub fn cliff_rects(&self) -> Vec<Rect> {
        // (début, fin exclue, première ligne, dernière ligne) de chaque bande ouverte.
        let mut open: Vec<(i32, i32, i32, i32)> = Vec::new();
        let mut done = Vec::new();
        for y in 0..self.rows {
            let mut runs = Vec::new();
            let mut x = 0;
            while x < self.columns {
                if self.cliff[(y * self.columns + x) as usize] {
                    let start = x;
                    while x < self.columns && self.cliff[(y * self.columns + x) as usize] {
                        x += 1;
                    }
                    runs.push((start, x));
                } else {
                    x += 1;
                }
            }
            let mut next = Vec::new();
            for (start, end) in runs {
                match open.iter().position(|&(s, e, _, _)| s == start && e == end) {
                    Some(k) => {
                        let (s, e, first, _) = open.swap_remove(k);
                        next.push((s, e, first, y));
                    }
                    None => next.push((start, end, y, y)),
                }
            }
            // Les bandes qui ne continuent pas sur cette ligne sont finies.
            done.append(&mut open);
            open = next;
        }
        done.append(&mut open);
        // Ordre fixe (les entités seront créées dans cet ordre).
        done.sort_unstable();
        done.into_iter()
            .map(|(start, end, first, last)| {
                let min = self.bounds.min + Vec2::new(start as f32, first as f32);
                Rect::from_corners(min, self.bounds.min + Vec2::new(end as f32, last as f32 + 1.0))
            })
            .collect()
    }

    // --- Affichage ---

    fn compute_display(&self) -> HeightField {
        let rect = self.bounds.inflate(DISPLAY_MARGIN);
        let columns = (rect.width() / DISPLAY_STEP).ceil() as usize + 1;
        let rows = (rect.height() / DISPLAY_STEP).ceil() as usize + 1;
        let mut field = HeightField {
            origin: rect.min,
            step: DISPLAY_STEP,
            columns,
            rows,
            heights: vec![0.0; columns * rows],
            steepness: vec![0.0; columns * rows],
            foot: vec![0.0; columns * rows],
            max_height: 0.0,
        };
        if self.is_flat() {
            return field;
        }
        // 1. La hauteur logique en chaque sommet, bords de plateau ondulés
        // par un bruit (les rampes restent droites).
        let mut raw = vec![0.0; columns * rows];
        for z in 0..rows {
            for x in 0..columns {
                let p = field.point(x, z);
                let wobble = Vec2::new(value_noise(p * 0.22, 3), value_noise(p * 0.22, 11)) * 0.5;
                let warped = p + wobble;
                let height = match self.ramps.iter().find_map(|r| r.progress(p).map(|t| (r, t))) {
                    Some((ramp, t)) => {
                        let (low, high) = (self.plateau_level(ramp.bottom), self.plateau_level(ramp.top));
                        (low as f32 + (high as f32 - low as f32) * t) * self.level_height
                    }
                    None => self.plateau_level(warped) as f32 * self.level_height,
                };
                // Au-delà de la carte, le relief redescend doucement.
                let outside = (p - p.clamp(self.bounds.min, self.bounds.max)).length();
                let fade = 1.0 - smoothstep(DISPLAY_MARGIN * 0.35, DISPLAY_MARGIN * 0.85, outside);
                raw[z * columns + x] = height * fade;
            }
        }
        // 2. Adoucissement : deux flous de rayon 1 sommet (50 cm), les
        // falaises deviennent des pentes très raides d'environ 1,5 m.
        let mut smooth = raw.clone();
        for _ in 0..2 {
            box_blur(&mut smooth, columns, rows, 1);
        }
        // 3. Faces rocheuses : du relief en plus là où ça monte fort.
        for z in 0..rows {
            for x in 0..columns {
                let slope = field_slope(&smooth, columns, rows, x, z, DISPLAY_STEP);
                let steep = smoothstep(0.6, 1.8, slope);
                let p = field.point(x, z);
                let rock = (value_noise(p * 1.4, 5) * 0.6 + value_noise(p * 3.1, 7) * 0.4) * 0.35;
                let i = z * columns + x;
                field.heights[i] = smooth[i] + rock * steep;
                field.steepness[i] = steep;
            }
        }
        // 4. Au pied d'une falaise, le sol est dans l'ombre : la moyenne des
        // hauteurs alentour (sur 3 m) dépasse la sienne.
        let mut around = field.heights.clone();
        box_blur(&mut around, columns, rows, 6);
        for (i, foot) in field.foot.iter_mut().enumerate() {
            *foot = ((around[i] - field.heights[i]) / self.level_height * 1.5).clamp(0.0, 0.5);
        }
        field.max_height = field.heights.iter().copied().fold(0.0, f32::max);
        field
    }

    /// Hauteur du sol affiché au point `p` (interpolée entre les sommets).
    pub fn height_at(&self, p: Vec2) -> f32 {
        self.display.sample(p)
    }

    /// Point du sol affiché que touche le rayon (la souris vise le relief).
    /// Sans relief : le plan y = 0.
    pub fn ray_hit(&self, ray: Ray3d) -> Option<Vec3> {
        let plane = |height: f32| {
            let distance = ray.intersect_plane(Vec3::Y * height, InfinitePlane3d::new(Vec3::Y))?;
            Some(ray.get_point(distance))
        };
        let top = self.display.max_height;
        if top <= 0.0 {
            return plane(0.0);
        }
        // On part du plan au-dessus du plus haut sommet, puis on avance par
        // petits pas jusqu'à passer sous le sol ; la dichotomie affine.
        let start = if ray.origin.y > top { ray.intersect_plane(Vec3::Y * top, InfinitePlane3d::new(Vec3::Y))? } else { 0.0 };
        let below = |t: f32| {
            let p = ray.get_point(t);
            p.y <= self.height_at(p.xz())
        };
        let step = 0.25;
        let mut t = start;
        for _ in 0..2000 {
            let next = t + step;
            if below(next) {
                let (mut lo, mut hi) = (t, next);
                for _ in 0..10 {
                    let mid = (lo + hi) / 2.0;
                    if below(mid) { hi = mid } else { lo = mid }
                }
                return Some(ray.get_point(hi));
            }
            if ray.get_point(next).y < 0.0 {
                break;
            }
            t = next;
        }
        plane(0.0)
    }

    /// Le maillage du sol affiché (sommets tous les 50 cm, sur la carte et
    /// ses abords), soulevé de `lift` mètres. Coordonnées de texture : 0 à 1
    /// sur `uv_rect`. Avec `shade`, chaque sommet reçoit une couleur
    /// (multipliée à la texture) : roche sombre et strates sur les falaises,
    /// sol un peu plus clair sur les hauteurs.
    pub fn mesh(&self, lift: f32, uv_rect: Rect, shade: bool) -> Mesh {
        let field = &self.display;
        let (columns, rows) = (field.columns, field.rows);
        let mut positions = Vec::with_capacity(columns * rows);
        let mut normals = Vec::with_capacity(columns * rows);
        let mut uvs = Vec::with_capacity(columns * rows);
        let mut colors = Vec::with_capacity(columns * rows);
        let max_level = self.plateaus.iter().map(|p| p.level).max().unwrap_or(1).max(1) as f32;
        for z in 0..rows {
            for x in 0..columns {
                let i = z * columns + x;
                let p = field.point(x, z);
                let height = field.heights[i];
                positions.push([p.x, height + lift, p.y]);
                // Normale : pente par différences centrées.
                let at = |x: usize, z: usize| field.heights[z.min(rows - 1) * columns + x.min(columns - 1)];
                let dx = at(x + 1, z) - at(x.saturating_sub(1), z);
                let dz = at(x, z + 1) - at(x, z.saturating_sub(1));
                normals.push(Vec3::new(-dx, 2.0 * field.step, -dz).normalize().to_array());
                uvs.push(((p - uv_rect.min) / uv_rect.size()).to_array());
                if shade {
                    let steep = field.steepness[i];
                    // Strates : des bandes horizontales sur les faces rocheuses.
                    let strata = 0.82 + 0.18 * (height * 4.1 + value_noise(p * 0.7, 9) * 2.0).sin().abs();
                    let rock = 0.22 * strata;
                    let plateau = 1.0 + 0.45 * (height / self.level_height).clamp(0.0, max_level);
                    let light = (plateau + (rock - plateau) * steep) * (1.0 - field.foot[i]);
                    // Les falaises tirent un peu vers l'ocre.
                    colors.push([light * (1.0 + 0.12 * steep), light, light * (1.0 - 0.1 * steep), 1.0]);
                }
            }
        }
        let mut indices = Vec::with_capacity((columns - 1) * (rows - 1) * 6);
        for z in 0..rows - 1 {
            for x in 0..columns - 1 {
                let quad = (z * columns + x) as u32;
                let next_row = columns as u32;
                // Même sens que le plan de Bevy (faces vers le haut).
                indices.extend_from_slice(&[quad + next_row + 1, quad + 1, quad + next_row, quad, quad + next_row, quad + 1]);
            }
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_indices(Indices::U32(indices))
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        if shade {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        }
        mesh
    }

    /// Le rectangle couvert par [`Terrain::mesh`] : au-delà, le sol est plat.
    pub fn mesh_rect(&self) -> Rect {
        let field = &self.display;
        Rect::from_corners(field.origin, field.point(field.columns - 1, field.rows - 1))
    }
}

/// Un anneau plat (à la hauteur `y`) entre le rectangle `inner` et le
/// rectangle `outer` : le sol autour du maillage du relief. Coordonnées de
/// texture : 0 à 1 sur `uv_rect`.
pub fn ring_mesh(inner: Rect, outer: Rect, y: f32, uv_rect: Rect) -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    // Quatre bandes : sud et nord sur toute la largeur, ouest et est entre les deux.
    let bands = [
        Rect::from_corners(outer.min, Vec2::new(outer.max.x, inner.min.y)),
        Rect::from_corners(Vec2::new(outer.min.x, inner.max.y), outer.max),
        Rect::from_corners(Vec2::new(outer.min.x, inner.min.y), Vec2::new(inner.min.x, inner.max.y)),
        Rect::from_corners(Vec2::new(inner.max.x, inner.min.y), Vec2::new(outer.max.x, inner.max.y)),
    ];
    for band in bands {
        let base = positions.len() as u32;
        for p in [band.min, Vec2::new(band.max.x, band.min.y), Vec2::new(band.min.x, band.max.y), band.max] {
            positions.push(Vec3::new(p.x, y, p.y));
        }
        indices.extend_from_slice(&[base + 3, base + 1, base + 2, base, base + 2, base + 1]);
    }
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| ((p.xz() - uv_rect.min) / uv_rect.size()).to_array()).collect();
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_indices(Indices::U32(indices))
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
}

impl HeightField {
    fn point(&self, x: usize, z: usize) -> Vec2 {
        self.origin + Vec2::new(x as f32, z as f32) * self.step
    }

    fn sample(&self, p: Vec2) -> f32 {
        if self.heights.is_empty() {
            return 0.0;
        }
        let local = (p - self.origin) / self.step;
        if local.x < 0.0 || local.y < 0.0 || local.x > (self.columns - 1) as f32 || local.y > (self.rows - 1) as f32 {
            return 0.0;
        }
        let (x0, z0) = (local.x.floor() as usize, local.y.floor() as usize);
        let (x1, z1) = ((x0 + 1).min(self.columns - 1), (z0 + 1).min(self.rows - 1));
        let f = local - Vec2::new(x0 as f32, z0 as f32);
        let at = |x: usize, z: usize| self.heights[z * self.columns + x];
        let top = at(x0, z0) + (at(x1, z0) - at(x0, z0)) * f.x;
        let bottom = at(x0, z1) + (at(x1, z1) - at(x0, z1)) * f.x;
        top + (bottom - top) * f.y
    }
}

/// Pente (mètres par mètre) au sommet (x, z) d'une grille de hauteurs.
fn field_slope(heights: &[f32], columns: usize, rows: usize, x: usize, z: usize, step: f32) -> f32 {
    let at = |x: usize, z: usize| heights[z.min(rows - 1) * columns + x.min(columns - 1)];
    let dx = at(x + 1, z) - at(x.saturating_sub(1), z);
    let dz = at(x, z + 1) - at(x, z.saturating_sub(1));
    Vec2::new(dx, dz).length() / (2.0 * step)
}

/// Flou « boîte » de rayon `radius` (en sommets), en ligne puis en colonne.
fn box_blur(values: &mut [f32], columns: usize, rows: usize, radius: usize) {
    let mut tmp = vec![0.0; values.len()];
    let r = radius as i64;
    for z in 0..rows {
        for x in 0..columns {
            let mut sum = 0.0;
            for d in -r..=r {
                let xx = (x as i64 + d).clamp(0, columns as i64 - 1) as usize;
                sum += values[z * columns + xx];
            }
            tmp[z * columns + x] = sum / (2 * r + 1) as f32;
        }
    }
    for z in 0..rows {
        for x in 0..columns {
            let mut sum = 0.0;
            for d in -r..=r {
                let zz = (z as i64 + d).clamp(0, rows as i64 - 1) as usize;
                sum += tmp[zz * columns + x];
            }
            values[z * columns + x] = sum / (2 * r + 1) as f32;
        }
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bruit de valeur entre -1 et 1 : des valeurs au hasard (mais fixes) aux
/// points entiers, interpolées en douceur entre eux. `seed` donne un autre bruit.
fn value_noise(p: Vec2, seed: u32) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let u = f * f * (3.0 - 2.0 * f);
    let corner = |dx: i32, dy: i32| hash(cell.x as i32 + dx, cell.y as i32 + dy, seed);
    let top = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * u.x;
    let bottom = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * u.x;
    top + (bottom - top) * u.y
}

/// Un nombre « au hasard » entre -1 et 1, toujours le même pour (x, y, seed).
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343) ^ (y as u32).wrapping_mul(0xD816_3841) ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 32767.5 - 1.0
}

/// Marqueur : un rectangle de falaise (obstacle fixe tiré du relief).
#[derive(Component, Debug)]
pub struct Cliff;

/// Marqueur : cette entité (sans parent) est posée sur le sol affiché. Sa
/// hauteur de départ devient un décalage au-dessus du sol, qui la suit
/// quand elle bouge. Ajouté d'office aux unités ([`SimPosition`]) et aux
/// obstacles fixes (bâtiments, gisements).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct OnTerrain;

/// Hauteur de l'entité au-dessus du sol, relevée à l'ajout de [`OnTerrain`].
#[derive(Component, Debug, Clone, Copy)]
struct TerrainOffset(f32);

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Terrain>()
            .add_observer(|add: On<Add<SimPosition>>, mut commands: Commands| {
                commands.entity(add.entity).insert(OnTerrain);
            })
            .add_observer(|add: On<Add<StaticObstacle>>, cliffs: Query<(), With<Cliff>>, mut commands: Commands| {
                if !cliffs.contains(add.entity) {
                    commands.entity(add.entity).insert(OnTerrain);
                }
            })
            .add_observer(record_offset)
            // Avant la navigation : la grille doit connaître les falaises.
            .add_systems(FixedUpdate, sync_cliffs.before(SimSet::Navigation))
            .add_systems(
                PostUpdate,
                follow_terrain.after(crate::simulation::sync_transforms).before(TransformSystems::Propagate),
            );
    }
}

fn record_offset(add: On<Add<OnTerrain>>, mut commands: Commands, transforms: Query<&Transform>) {
    let offset = transforms.get(add.entity).map_or(0.0, |t| t.translation.y);
    // `try_insert` : sans erreur si l'entité a disparu entre-temps.
    commands.entity(add.entity).try_insert(TerrainOffset(offset));
}

/// Les falaises suivent le relief : recréées quand il change.
fn sync_cliffs(mut commands: Commands, terrain: Res<Terrain>, cliffs: Query<Entity, With<Cliff>>) {
    if !terrain.is_changed() {
        return;
    }
    for cliff in &cliffs {
        commands.entity(cliff).despawn();
    }
    for rect in terrain.cliff_rects() {
        commands.spawn((Name::new("Falaise"), Cliff, StaticObstacle(rect)));
    }
}

/// Pose chaque entité [`OnTerrain`] sur le sol affiché, à son décalage.
fn follow_terrain(terrain: Res<Terrain>, mut entities: Query<(&TerrainOffset, &mut Transform)>) {
    if terrain.is_flat() && !terrain.is_changed() {
        return;
    }
    for (offset, mut transform) in &mut entities {
        let y = offset.0 + terrain.height_at(transform.translation.xz());
        // N'écrire que si ça change (pas de propagation inutile).
        if (transform.translation.y - y).abs() > 1e-4 {
            transform.translation.y = y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un plateau de niveau 1 sur la moitié nord (z > 0) de la carte, avec
    /// une rampe au milieu.
    fn half_plateau() -> Terrain {
        let bounds = Rect::new(-20.0, -20.0, 20.0, 20.0);
        let plateau = Plateau::new(1, [Vec2::new(-30.0, 0.0), Vec2::new(30.0, 0.0), Vec2::new(30.0, 30.0), Vec2::new(-30.0, 30.0)]);
        let ramp = Ramp { bottom: Vec2::new(0.0, -3.5), top: Vec2::new(0.0, 3.5), width: 6.0 };
        Terrain::new(bounds, DEFAULT_LEVEL_HEIGHT, vec![plateau], vec![ramp])
    }

    #[test]
    fn levels_ramps_and_cliffs() {
        let terrain = half_plateau();
        assert_eq!(terrain.level_at(Vec2::new(10.0, -10.0)), 0);
        assert_eq!(terrain.level_at(Vec2::new(10.0, 10.0)), 1);
        // Le bord du plateau est une falaise, sauf sur la rampe.
        assert!(terrain.is_cliff(Vec2::new(10.0, 0.5)));
        assert!(!terrain.is_cliff(Vec2::new(10.0, -0.5)));
        assert!(terrain.is_ramp(Vec2::new(0.5, 0.5)));
        assert!(!terrain.is_cliff(Vec2::new(0.5, 0.5)));
        // Les rectangles de falaise couvrent exactement les cases de falaise.
        let rects = terrain.cliff_rects();
        let area: f32 = rects.iter().map(|r| r.width() * r.height()).sum();
        assert_eq!(area as usize, terrain.cliff.iter().filter(|c| **c).count());
    }

    #[test]
    fn the_mouse_hits_the_plateau() {
        let terrain = half_plateau();
        // Un rayon vertical au-dessus du plateau s'arrête sur le plateau.
        let ray = Ray3d::new(Vec3::new(10.0, 50.0, 10.0), Dir3::NEG_Y);
        let hit = terrain.ray_hit(ray).unwrap();
        assert!((hit.y - DEFAULT_LEVEL_HEIGHT).abs() < 0.05, "{hit}");
        let low = terrain.ray_hit(Ray3d::new(Vec3::new(10.0, 50.0, -10.0), Dir3::NEG_Y)).unwrap();
        assert!(low.y.abs() < 0.05, "{low}");
    }

    #[test]
    fn a_flat_map_has_no_cliffs() {
        let terrain = Terrain::default();
        assert!(terrain.cliff_rects().is_empty());
        assert_eq!(terrain.height_at(Vec2::new(3.0, 4.0)), 0.0);
    }
}
