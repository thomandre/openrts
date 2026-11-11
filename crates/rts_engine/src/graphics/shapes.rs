//! Formes géométriques des pièces d'unités, et fusion des pièces en un maillage.
//!
//! Un **maillage** (`Mesh`) est une liste de sommets (position, normale,
//! coordonnées de texture « UV ») et de triangles qui les relient. Bevy
//! fournit les formes de base (cylindre, sphère…) ; on ajoute ici deux formes
//! utiles pour des engins blindés : la plaque biseautée et le prisme.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

/// Forme d'une pièce, en mètres, centrée sur son origine.
#[derive(Debug, Clone, PartialEq)]
pub enum PartShape {
    /// Pavé aux arêtes et coins coupés en biseau (`bevel` en mètres) : la
    /// pièce de base du blindage. Les biseaux attrapent la lumière et rendent
    /// les volumes lisibles, là où un cube net paraît plat.
    Plate { size: Vec3, bevel: f32 },
    /// Profil convexe dessiné dans le plan (z, y), extrudé sur `width` le long
    /// de x : coques de char, ailerons, tourelles.
    Prism { profile: Vec<Vec2>, width: f32 },
    /// Cylindre vertical (axe y).
    Cylinder { radius: f32, height: f32 },
    /// Cône tronqué vertical (axe y) ; `radius_top` = 0 donne un cône pointu.
    Frustum { radius_bottom: f32, radius_top: f32, height: f32 },
    Sphere { radius: f32 },
    /// Boule organique (tronc, rocher, touffe de mousse) : une sphère de rayon
    /// `radius` dont la surface est bosselée par un bruit déterministe
    /// (± `amplitude` × rayon). `frequency` règle la taille des bosses (plus
    /// c'est grand, plus elles sont petites et nombreuses), `seed` change le motif.
    Blob { radius: f32, amplitude: f32, frequency: f32, seed: f32 },
    /// Tube qui suit une courbe lisse passant par tous les `points` (courbe de
    /// Catmull-Rom), d'un rayon `radius` au départ à `radius_end` à l'arrivée :
    /// branches, racines, pattes, tuyaux.
    Tube { points: Vec<Vec3>, radius: f32, radius_end: f32 },
    /// Forme de révolution (« lathe », comme sur un tour de potier) : le
    /// profil, une suite de points (rayon, hauteur) du bas vers le haut, tourne
    /// autour de l'axe y en `segments` pas. Bulbes, vases, cosses, coupoles.
    Lathe { profile: Vec<Vec2>, segments: u32 },
    /// Touffe de `count` petites feuilles en losange plié, de `leaf_size`
    /// mètres de long, réparties sur une sphère de rayon `radius` (surtout
    /// vers le haut) et tournées vers l'extérieur. Tout le feuillage d'une
    /// touffe forme un seul maillage ; `seed` change la disposition.
    Foliage { radius: f32, count: u32, leaf_size: f32, seed: u32 },
    /// Comme [`PartShape::Prism`], mais les arêtes des deux faces latérales
    /// sont coupées en biseau de `bevel` mètres (comme une plaque) : coques,
    /// tourelles et ailes attrapent un liseré de lumière sur leur contour.
    BeveledPrism { profile: Vec<Vec2>, width: f32, bevel: f32 },
    /// Forme de révolution **taillée** : comme [`PartShape::Lathe`], mais avec
    /// peu de côtés (`sides`) et des faces planes, comme une pierre taillée ou
    /// un écrou. Profil (rayon, hauteur) du bas vers le haut. Une face est
    /// tournée vers +z. Exemples : `sides` = 4 et le profil
    /// [(0, −r), (r, 0), (0, r)] donnent un octaèdre ; `sides` = 6 et un
    /// profil rectangulaire, un prisme hexagonal.
    Faceted { profile: Vec<Vec2>, sides: u32 },
}

impl PartShape {
    /// Construit le maillage de la forme.
    pub fn mesh(&self) -> Mesh {
        let mesh = match self {
            PartShape::Plate { size, bevel } => plate(*size, *bevel),
            PartShape::Prism { profile, width } => prism(profile, *width),
            PartShape::Cylinder { radius, height } => Cylinder::new(*radius, *height).mesh().resolution(14).build(),
            PartShape::Frustum { radius_bottom, radius_top, height } => {
                if *radius_top <= 0.0 {
                    Cone { radius: *radius_bottom, height: *height }.mesh().resolution(14).build()
                } else {
                    ConicalFrustum { radius_top: *radius_top, radius_bottom: *radius_bottom, height: *height }
                        .mesh()
                        .resolution(14)
                        .build()
                }
            }
            PartShape::Sphere { radius } => Sphere::new(*radius).mesh().uv(14, 10),
            PartShape::Blob { radius, amplitude, frequency, seed } => blob(*radius, *amplitude, *frequency, *seed),
            PartShape::Tube { points, radius, radius_end } => tube(points, *radius, *radius_end),
            PartShape::Lathe { profile, segments } => lathe(profile, *segments),
            PartShape::Foliage { radius, count, leaf_size, seed } => foliage(*radius, *count, *leaf_size, *seed),
            PartShape::BeveledPrism { profile, width, bevel } => beveled_prism(profile, *width, *bevel),
            PartShape::Faceted { profile, sides } => faceted(profile, *sides),
        };
        with_box_uvs(mesh)
    }
}

/// Fusionne plusieurs pièces (forme + placement) en un seul maillage.
///
/// Dessiner un gros maillage coûte bien moins cher que dessiner trente petits :
/// toutes les pièces d'une unité qui partagent un matériau deviennent un seul
/// objet pour la carte graphique.
pub fn merge_parts<'a>(parts: impl IntoIterator<Item = (&'a PartShape, Transform)>) -> Mesh {
    let mut merged = empty_mesh();
    for (shape, transform) in parts {
        let mesh = shape.mesh().transformed_by(transform);
        // `merge` ne peut échouer que si les attributs diffèrent, ce qui n'arrive
        // pas ici : toutes nos formes ont position, normale et UV.
        if let Err(error) = merged.merge(&mesh) {
            warn!("Pièce ignorée : {error}");
        }
    }
    // Les tangentes (direction du « u » de la texture sur la surface) sont
    // nécessaires aux normal maps.
    if let Err(error) = merged.generate_tangents() {
        warn!("Tangentes impossibles à calculer : {error}");
    }
    merged
}

fn empty_mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new())
        .with_inserted_indices(Indices::U32(Vec::new()))
}

/// Recalcule les UV par « projection sur un cube » : chaque sommet prend les
/// deux coordonnées perpendiculaires à l'axe vers lequel il est tourné, en
/// mètres. Une texture garde ainsi la même taille sur toutes les pièces,
/// petites ou grandes.
fn with_box_uvs(mut mesh: Mesh) -> Mesh {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return mesh;
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
        return mesh;
    };
    let uvs: Vec<[f32; 2]> = positions
        .iter()
        .zip(normals)
        .map(|(p, n)| {
            let (p, n) = (Vec3::from_array(*p), Vec3::from_array(*n).abs());
            if n.x >= n.y && n.x >= n.z {
                [p.z, -p.y]
            } else if n.y >= n.z {
                [p.x, p.z]
            } else {
                [p.x, -p.y]
            }
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

/// Ajoute une face plane convexe : ses sommets sont remis dans l'ordre
/// antihoraire vu de l'extérieur (le sens « avant » des triangles pour la
/// carte graphique), puis découpés en triangles en éventail.
fn push_face(
    points: &[Vec3],
    normal: Vec3,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    let center = points.iter().copied().sum::<Vec3>() / points.len() as f32;
    let tangent = normal.any_orthonormal_vector();
    let bitangent = normal.cross(tangent);
    let mut sorted: Vec<Vec3> = points.to_vec();
    sorted.sort_by(|a, b| {
        let angle = |p: &Vec3| (*p - center).dot(bitangent).atan2((*p - center).dot(tangent));
        angle(a).total_cmp(&angle(b))
    });
    sorted.dedup_by(|a, b| a.distance(*b) < 1e-5);
    if sorted.len() < 3 {
        return;
    }
    let first = positions.len() as u32;
    for p in &sorted {
        positions.push(p.to_array());
        normals.push(normal.to_array());
    }
    for i in 1..sorted.len() as u32 - 1 {
        indices.extend_from_slice(&[first, first + i, first + i + 1]);
    }
}

fn build_mesh(positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, indices: Vec<u32>) -> Mesh {
    let uvs = vec![[0.0, 0.0]; positions.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// Plaque biseautée : 6 grandes faces, 12 biseaux d'arête, 8 coins.
///
/// Chaque coin du pavé est remplacé par trois sommets, chacun repoussé vers
/// l'une des trois faces qui s'y rejoignent. Les faces se forment ensuite en
/// regroupant les sommets qui partagent une même direction.
fn plate(size: Vec3, bevel: f32) -> Mesh {
    let half = size / 2.0;
    let bevel = bevel.clamp(0.0, size.min_element() * 0.22);
    let inner = half - Vec3::splat(bevel);
    // Sommet du coin `corner` (signes ±1) repoussé le long de l'axe `axis`.
    let point = |corner: Vec3, axis: usize| {
        let mut p = corner * inner;
        p[axis] += corner[axis] * bevel;
        p
    };
    let corners: Vec<Vec3> = (0..8)
        .map(|i| Vec3::new(sign(i & 1), sign(i & 2), sign(i & 4)))
        .collect();

    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    // Les 26 directions (x, y, z) ∈ {-1, 0, 1}³ sauf (0, 0, 0) : 6 faces, 12 arêtes, 8 coins.
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let dir = Vec3::new(dx as f32, dy as f32, dz as f32);
                if dir == Vec3::ZERO || (bevel <= 0.0 && dir.abs().element_sum() > 1.0) {
                    continue;
                }
                let axes: Vec<usize> = (0..3).filter(|&a| dir[a] != 0.0).collect();
                // Coins concernés : ceux qui ont les mêmes signes que `dir` sur ses axes non nuls.
                let points: Vec<Vec3> = corners
                    .iter()
                    .filter(|c| axes.iter().all(|&a| c[a] == dir[a]))
                    .flat_map(|&c| axes.iter().map(move |&a| point(c, a)))
                    .collect();
                push_face(&points, dir.normalize(), &mut positions, &mut normals, &mut indices);
            }
        }
    }
    build_mesh(positions, normals, indices)
}

fn sign(bit: usize) -> f32 {
    if bit != 0 { 1.0 } else { -1.0 }
}

/// Prisme : le profil (z, y) est posé en x = ±width/2, puis chaque arête du
/// profil devient une face rectangulaire.
fn prism(profile: &[Vec2], width: f32) -> Mesh {
    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    let half = width / 2.0;
    let to_3d = |p: Vec2, x: f32| Vec3::new(x, p.y, p.x);
    let center = profile.iter().copied().sum::<Vec2>() / profile.len() as f32;

    for x in [-half, half] {
        let points: Vec<Vec3> = profile.iter().map(|&p| to_3d(p, x)).collect();
        push_face(&points, Vec3::X * x.signum(), &mut positions, &mut normals, &mut indices);
    }
    for i in 0..profile.len() {
        let (a, b) = (profile[i], profile[(i + 1) % profile.len()]);
        // Normale de l'arête dans le plan (z, y), tournée vers l'extérieur.
        let mut n = Vec2::new(b.y - a.y, a.x - b.x).normalize_or_zero();
        if n.dot((a + b) / 2.0 - center) < 0.0 {
            n = -n;
        }
        let points = [to_3d(a, -half), to_3d(b, -half), to_3d(b, half), to_3d(a, half)];
        push_face(&points, Vec3::new(0.0, n.y, n.x), &mut positions, &mut normals, &mut indices);
    }
    build_mesh(positions, normals, indices)
}

/// Prisme biseauté : le profil complet est posé en x = ±(width/2 − bevel),
/// un profil rétréci de `bevel` (chaque arête rentrée vers l'intérieur) ferme
/// les deux faces latérales en x = ±width/2, et une bande inclinée à 45°
/// relie les deux. Le profil doit être convexe.
fn beveled_prism(profile: &[Vec2], width: f32, bevel: f32) -> Mesh {
    let n = profile.len();
    if n < 3 {
        return prism(profile, width);
    }
    let center = profile.iter().copied().sum::<Vec2>() / n as f32;
    // Normale extérieure de l'arête i (du point i au point i + 1), dans le plan (z, y).
    let edge_normal = |i: usize| {
        let (a, b) = (profile[i], profile[(i + 1) % n]);
        let normal = Vec2::new(b.y - a.y, a.x - b.x).normalize_or_zero();
        if normal.dot((a + b) / 2.0 - center) < 0.0 { -normal } else { normal }
    };
    // Le biseau ne doit pas dévorer la pièce : au plus un quart de son épaisseur.
    let thinnest = (0..n).map(|i| (profile[i] - center).dot(edge_normal(i)).abs()).fold(width / 2.0, f32::min);
    let bevel = bevel.clamp(0.0, thinnest * 0.5);
    if bevel <= 0.0 {
        return prism(profile, width);
    }
    // Profil rétréci : chaque sommet recule le long de la bissectrice de ses
    // deux arêtes, assez pour que chaque arête se décale de `bevel` exactement.
    let inset: Vec<Vec2> = (0..n)
        .map(|i| {
            let (before, after) = (edge_normal((i + n - 1) % n), edge_normal(i));
            profile[i] - bevel * (before + after) / (1.0 + before.dot(after)).max(0.2)
        })
        .collect();

    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    let (outer, inner) = (width / 2.0, width / 2.0 - bevel);
    let to_3d = |p: Vec2, x: f32| Vec3::new(x, p.y, p.x);
    for side in [-1.0, 1.0] {
        let cap: Vec<Vec3> = inset.iter().map(|&p| to_3d(p, side * outer)).collect();
        push_face(&cap, Vec3::X * side, &mut positions, &mut normals, &mut indices);
    }
    for i in 0..n {
        let j = (i + 1) % n;
        let normal = edge_normal(i);
        let flank = Vec3::new(0.0, normal.y, normal.x);
        // Face principale de l'arête, entre les deux biseaux.
        let points = [to_3d(profile[i], -inner), to_3d(profile[j], -inner), to_3d(profile[j], inner), to_3d(profile[i], inner)];
        push_face(&points, flank, &mut positions, &mut normals, &mut indices);
        // Les deux biseaux de l'arête, inclinés à 45° entre le flanc et la face latérale.
        for side in [-1.0, 1.0] {
            let points = [
                to_3d(profile[i], side * inner),
                to_3d(profile[j], side * inner),
                to_3d(inset[j], side * outer),
                to_3d(inset[i], side * outer),
            ];
            push_face(&points, (flank + Vec3::X * side).normalize(), &mut positions, &mut normals, &mut indices);
        }
    }
    build_mesh(positions, normals, indices)
}

/// Forme de révolution taillée : chaque segment du profil, tourné autour de
/// l'axe y, donne une couronne de `sides` faces planes (des quadrilatères, ou
/// des triangles quand un bout du segment est sur l'axe). Chaque face a ses
/// propres sommets et sa propre normale : les arêtes restent vives et
/// accrochent la lumière, comme les facettes d'une gemme.
fn faceted(profile: &[Vec2], sides: u32) -> Mesh {
    let sides = sides.max(3);
    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    // Sommet `k` d'un anneau de rayon `r` à la hauteur `y`. Les sommets sont
    // décalés d'un demi-pas : la face 0 est centrée sur +z.
    let corner = |p: Vec2, k: u32| {
        let a = std::f32::consts::TAU * (k as f32 - 0.5) / sides as f32;
        Vec3::new(p.x * a.sin(), p.y, p.x * a.cos())
    };
    for pair in profile.windows(2) {
        let (low, high) = (pair[0], pair[1]);
        // Direction de l'extérieur dans le plan (rayon, hauteur), comme pour `lathe`.
        let along = (high - low).normalize_or(Vec2::Y);
        let out = Vec2::new(along.y, -along.x);
        for k in 0..sides {
            let points = [corner(low, k), corner(low, k + 1), corner(high, k + 1), corner(high, k)];
            // Normale de Newell : somme sur les arêtes, robuste même quand
            // deux sommets sont confondus (face triangulaire sur l'axe).
            let mut normal = Vec3::ZERO;
            for i in 0..4 {
                let (a, b) = (points[i], points[(i + 1) % 4]);
                normal += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
            }
            // On la retourne si elle pointe vers l'intérieur.
            let mid = std::f32::consts::TAU * k as f32 / sides as f32;
            let outward = Vec3::new(mid.sin() * out.x, out.y, mid.cos() * out.x);
            let mut normal = normal.normalize_or(outward);
            if normal.dot(outward) < 0.0 {
                normal = -normal;
            }
            push_face(&points, normal, &mut positions, &mut normals, &mut indices);
        }
    }
    build_mesh(positions, normals, indices)
}

// ---------------------------------------------------------------------------
// Formes organiques : boules bosselées, tubes courbes, formes de révolution,
// feuillage. Toutes sont des grilles de sommets (anneaux × secteurs) reliés
// deux par deux en triangles, avec des normales lisses calculées à la main.
// ---------------------------------------------------------------------------

/// Bruit déterministe : une somme de sinus croisés (le `nz` de la page de
/// direction artistique). « Déterministe » veut dire qu'il n'y a aucun
/// tirage au hasard : les mêmes coordonnées donnent toujours la même valeur.
/// Une unité garde donc exactement la même forme à chaque lancement, sur
/// toutes les machines. Valeurs comprises à peu près entre −1,5 et 1,5.
pub fn organic_noise(p: Vec3) -> f32 {
    (p.x * 1.7 + p.y * 2.3).sin() * (p.y * 1.9 + p.z * 2.1).sin() * (p.z * 2.7 + p.x * 1.3).sin()
        + 0.5 * (p.x * 4.1 + p.z * 3.3).sin() * (p.y * 4.7 + p.x * 2.9).sin()
}

/// Relie une grille de `rows` × `columns` sommets (rangés ligne par ligne)
/// en triangles. Les triangles sont dans le sens antihoraire vu de
/// l'extérieur quand chaque ligne tourne dans le sens antihoraire vu depuis
/// la ligne suivante (c'est le cas de toutes les formes ci-dessous).
fn grid_indices(rows: u32, columns: u32, indices: &mut Vec<u32>) {
    for row in 0..rows - 1 {
        for col in 0..columns - 1 {
            let a = row * columns + col;
            let b = a + columns;
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
}

/// Boule bosselée : une sphère en anneaux et secteurs, dont chaque sommet est
/// repoussé le long de sa direction selon le bruit.
fn blob(radius: f32, amplitude: f32, frequency: f32, seed: f32) -> Mesh {
    // 11 anneaux × 17 secteurs = 187 sommets : les bosses cachent les
    // facettes, inutile d'en mettre plus (des centaines d'unités en portent).
    const RINGS: u32 = 10;
    const SECTORS: u32 = 16;
    // Point de la surface dans la direction (normalisée) `dir`.
    let surface = |dir: Vec3| {
        let noise = organic_noise(dir * frequency + Vec3::new(seed, 0.0, 0.0));
        dir * radius * (1.0 + amplitude * noise)
    };
    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for ring in 0..=RINGS {
        // θ va du pôle nord (0) au pôle sud (π).
        let theta = std::f32::consts::PI * ring as f32 / RINGS as f32;
        for sector in 0..=SECTORS {
            let phi = std::f32::consts::TAU * sector as f32 / SECTORS as f32;
            let dir = Vec3::new(theta.sin() * phi.cos(), theta.cos(), -theta.sin() * phi.sin());
            positions.push(surface(dir).to_array());
            // Normale : on regarde comment la surface bouge quand on se décale
            // un tout petit peu dans deux directions perpendiculaires ; le
            // produit vectoriel de ces deux pentes est perpendiculaire à la surface.
            let (t1, t2) = dir.any_orthonormal_pair();
            let eps = 0.01;
            let slope1 = surface((dir + t1 * eps).normalize()) - surface((dir - t1 * eps).normalize());
            let slope2 = surface((dir + t2 * eps).normalize()) - surface((dir - t2 * eps).normalize());
            let mut normal = slope1.cross(slope2).normalize_or(dir);
            if normal.dot(dir) < 0.0 {
                normal = -normal;
            }
            normals.push(normal.to_array());
        }
    }
    // Les anneaux vont du haut vers le bas : on les relie dans l'ordre inverse
    // pour garder le sens antihoraire vu de l'extérieur.
    let columns = SECTORS + 1;
    let mut grid = Vec::new();
    grid_indices(RINGS + 1, columns, &mut grid);
    for triangle in grid.chunks(3) {
        indices.extend_from_slice(&[triangle[0], triangle[2], triangle[1]]);
    }
    build_mesh(positions, normals, indices)
}

/// Point d'une **courbe de Catmull-Rom** entre `p1` et `p2` (`t` de 0 à 1).
///
/// Une courbe de Catmull-Rom passe exactement par tous ses points de
/// contrôle, en tournant en douceur : entre deux points, la direction de la
/// courbe est donnée par leurs voisins (`p0` avant, `p3` après). C'est la
/// façon la plus simple de dessiner une branche « à main levée » avec
/// quelques points seulement.
pub fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let (t2, t3) = (t * t, t * t * t);
    0.5 * (2.0 * p1
        + (p2 - p0) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
}

/// Échantillonne la courbe de Catmull-Rom qui passe par `points`, avec
/// `per_span` points entre deux points de contrôle.
fn sample_curve(points: &[Vec3], per_span: usize) -> Vec<Vec3> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    // Aux deux bouts, il manque un voisin : on en invente un dans le
    // prolongement du premier (et du dernier) segment.
    let before = 2.0 * points[0] - points[1];
    let after = 2.0 * points[n - 1] - points[n - 2];
    let at = |i: isize| -> Vec3 {
        if i < 0 {
            before
        } else if i as usize >= n {
            after
        } else {
            points[i as usize]
        }
    };
    let mut samples = Vec::with_capacity((n - 1) * per_span + 1);
    for span in 0..n as isize - 1 {
        for k in 0..per_span {
            let t = k as f32 / per_span as f32;
            samples.push(catmull_rom(at(span - 1), at(span), at(span + 1), at(span + 2), t));
        }
    }
    samples.push(points[n - 1]);
    samples
}

/// Tube le long d'une courbe de Catmull-Rom, fermé à ses deux bouts.
///
/// Autour de chaque point de la courbe, on dessine un anneau de sommets dans
/// le plan perpendiculaire à la courbe. Pour que les anneaux ne vrillent pas,
/// le repère de chaque anneau est celui du précédent, tourné juste de quoi
/// suivre la courbe (« transport parallèle »).
fn tube(points: &[Vec3], radius: f32, radius_end: f32) -> Mesh {
    // 6 côtés et 4 anneaux entre deux points de contrôle : assez rond vu de
    // la caméra RTS, et léger.
    const SIDES: u32 = 6;
    let curve = sample_curve(points, 4);
    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    if curve.len() < 2 {
        return build_mesh(positions, normals, indices);
    }
    let last = curve.len() - 1;
    let tangent = |i: usize| (curve[(i + 1).min(last)] - curve[i.saturating_sub(1)]).normalize_or(Vec3::Y);

    let mut side = tangent(0).any_orthonormal_vector();
    let mut previous_tangent = tangent(0);
    let mut frames = Vec::with_capacity(curve.len());
    for (i, &center) in curve.iter().enumerate() {
        let t = tangent(i);
        // Rotation qui amène la tangente précédente sur la nouvelle,
        // appliquée au vecteur de côté.
        side = Quat::from_rotation_arc(previous_tangent, t) * side;
        side = (side - t * side.dot(t)).normalize_or(t.any_orthonormal_vector());
        previous_tangent = t;
        let up = t.cross(side);
        let r = radius + (radius_end - radius) * i as f32 / last as f32;
        for s in 0..=SIDES {
            let a = std::f32::consts::TAU * s as f32 / SIDES as f32;
            let dir = side * a.cos() + up * a.sin();
            positions.push((center + dir * r).to_array());
            normals.push(dir.to_array());
        }
        frames.push((center, side, up, r, t));
    }
    grid_indices(curve.len() as u32, SIDES + 1, &mut indices);

    // Bouchons plats aux deux bouts (un éventail de triangles autour du centre).
    for (frame, outward) in [(frames[0], -1.0), (frames[last], 1.0)] {
        let (center, side, up, r, t) = frame;
        if r < 0.005 {
            continue;
        }
        let normal = t * outward;
        let first = positions.len() as u32;
        positions.push(center.to_array());
        normals.push(normal.to_array());
        for s in 0..SIDES {
            let a = std::f32::consts::TAU * s as f32 / SIDES as f32;
            positions.push((center + (side * a.cos() + up * a.sin()) * r).to_array());
            normals.push(normal.to_array());
        }
        for s in 0..SIDES {
            let (k, next) = (first + 1 + s, first + 1 + (s + 1) % SIDES);
            if outward > 0.0 {
                indices.extend_from_slice(&[first, k, next]);
            } else {
                indices.extend_from_slice(&[first, next, k]);
            }
        }
    }
    build_mesh(positions, normals, indices)
}

/// Forme de révolution : chaque point (rayon, hauteur) du profil devient un
/// anneau horizontal. Les normales sont lissées le long du profil.
fn lathe(profile: &[Vec2], segments: u32) -> Mesh {
    let segments = segments.max(3);
    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    if profile.len() < 2 {
        return build_mesh(positions, normals, indices);
    }
    let last = profile.len() - 1;
    for (i, p) in profile.iter().enumerate() {
        // Direction du profil en ce point, puis sa perpendiculaire tournée
        // vers l'extérieur (le profil monte : l'extérieur est à sa droite).
        let along = (profile[(i + 1).min(last)] - profile[i.saturating_sub(1)]).normalize_or(Vec2::Y);
        let out = Vec2::new(along.y, -along.x);
        for s in 0..=segments {
            let a = std::f32::consts::TAU * s as f32 / segments as f32;
            let (cos, sin) = (a.cos(), -a.sin());
            positions.push([p.x * cos, p.y, p.x * sin]);
            normals.push(Vec3::new(out.x * cos, out.y, out.x * sin).normalize_or(Vec3::Y).to_array());
        }
    }
    grid_indices(profile.len() as u32, segments + 1, &mut indices);
    build_mesh(positions, normals, indices)
}

/// Petit générateur pseudo-aléatoire (« xorshift ») : à partir d'une graine,
/// il produit toujours la même suite de nombres. Le feuillage est donc
/// identique à chaque lancement.
struct LeafRng(u32);

impl LeafRng {
    /// Nombre entre 0 et 1.
    fn next(&mut self) -> f32 {
        // Décalages et « ou exclusif » qui mélangent les bits de l'état.
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Touffe de feuilles. Chaque feuille est un losange plié en deux le long de
/// sa nervure (deux triangles), dessiné des deux côtés : 8 sommets par feuille.
fn foliage(radius: f32, count: u32, leaf_size: f32, seed: u32) -> Mesh {
    // La graine ne doit jamais valoir 0 (xorshift resterait bloqué à 0).
    let mut rng = LeafRng(seed.wrapping_mul(2_654_435_761).max(1));
    let (length, width, fold) = (leaf_size, leaf_size * 0.29, leaf_size * 0.13);
    // La feuille au repos : tige en (0, 0, 0), pointe en +y, bords pliés vers +z.
    let leaf = [
        Vec3::ZERO,
        Vec3::new(-width, length * 0.4, fold),
        Vec3::new(0.0, length, 0.0),
        Vec3::new(width, length * 0.4, fold),
    ];
    // Normale commune aux deux moitiés de la feuille (la moyenne des deux) :
    // la face avant regarde vers +z, ce qui donne un rendu doux.
    let face_normal = Vec3::Z;

    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..count {
        // Direction au hasard sur la sphère, penchée vers le haut comme dans
        // un vrai feuillage qui cherche la lumière.
        let theta = rng.next() * std::f32::consts::TAU;
        let u = rng.next() * 2.0 - 1.0;
        let horizontal = (1.0 - u * u).sqrt();
        let dir = Vec3::new(horizontal * theta.cos(), u * 0.6 + 0.4, horizontal * theta.sin()).normalize();
        let at = dir * radius * (0.6 + 0.4 * rng.next());
        // La feuille pointe vers l'extérieur, tournée et inclinée au hasard.
        let rotation = Quat::from_rotation_arc(Vec3::Y, dir)
            * Quat::from_rotation_y(rng.next() * std::f32::consts::TAU)
            * Quat::from_rotation_x((rng.next() - 0.5) * 0.9);
        let scale = 0.7 + rng.next() * 0.8;
        let normal = rotation * face_normal;
        // Face avant puis face arrière (normales opposées, triangles retournés).
        for sign in [1.0, -1.0] {
            let first = positions.len() as u32;
            for corner in leaf {
                positions.push((at + rotation * (corner * scale)).to_array());
                normals.push((normal * sign).to_array());
            }
            if sign > 0.0 {
                indices.extend_from_slice(&[first, first + 2, first + 1, first, first + 3, first + 2]);
            } else {
                indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
            }
        }
    }
    build_mesh(positions, normals, indices)
}
