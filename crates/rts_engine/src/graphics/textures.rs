//! Textures procédurales : dessinées par le code au démarrage, sans fichier image.
//!
//! Chaque motif est d'abord dessiné comme une **carte de hauteur** (plus clair =
//! plus haut), puis converti en **normal map** : une image dont chaque pixel
//! code l'orientation de la surface (x, y, z rangés dans r, g, b). Le shader
//! s'en sert pour éclairer de faux reliefs (rainures, rivets) sur une face
//! parfaitement plate, sans ajouter un seul triangle.
//!
//! On dessine en même temps une image de **détail de couleur** en niveaux de
//! gris (salissures, rayures, joints sombres), multipliée par la couleur du
//! matériau.
//!
//! Les motifs sont « raccordables » : le bord droit continue le bord gauche,
//! pour qu'on puisse répéter l'image sur toute une surface sans couture visible.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Motif de détail d'une surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceDetail {
    /// Pas de texture : couleur unie.
    Smooth,
    /// Panneaux rivetés, propres.
    Panels,
    /// Panneaux rivetés, rayés et salis.
    WornPanels,
    /// Grille hexagonale (blindage, dalles).
    Hexagons,
    /// Écorce striée : longues fibres verticales ondulées, creux sombres.
    Bark,
    /// Bande de danger : chevrons diagonaux clairs et presque noirs. Multipliés
    /// par la couleur du matériau (la couleur d'équipe, en général), ils
    /// donnent les rayures orange et noir des engins de chantier.
    Chevrons,
}

/// Paire d'images d'un motif : détail de couleur et normal map.
pub struct DetailImages {
    pub albedo: Image,
    pub normal: Image,
}

/// Côté des textures de détail, en pixels.
const DETAIL_SIZE: usize = 256;

/// Dessine le motif demandé (`None` pour une surface unie).
pub fn detail_images(detail: SurfaceDetail) -> Option<DetailImages> {
    let (height, albedo, strength) = match detail {
        SurfaceDetail::Smooth => return None,
        SurfaceDetail::Panels => {
            let (h, a) = panels(false);
            (h, a, 2.2)
        }
        SurfaceDetail::WornPanels => {
            let (h, a) = panels(true);
            (h, a, 3.2)
        }
        SurfaceDetail::Hexagons => {
            let (h, a) = hexagons();
            (h, a, 3.0)
        }
        SurfaceDetail::Bark => {
            let (h, a) = bark();
            (h, a, 4.5)
        }
        SurfaceDetail::Chevrons => {
            let (h, a) = chevrons();
            (h, a, 2.0)
        }
    };
    Some(DetailImages {
        albedo: albedo.to_albedo_image(),
        normal: height.to_normal_image(strength),
    })
}

/// Textures du sol : couleur complète (pas un simple détail) et normal map.
///
/// `base` et `dust` sont les deux teintes du terrain (roche et poussière) ;
/// `grid_lines` trace une grille discrète sur les bords de l'image (et en son
/// milieu), pour aider à juger les distances.
pub fn terrain_images(base: Color, dust: Color, grid_lines: bool) -> DetailImages {
    const SIZE: usize = 512;
    let mut rng = Rng::new(7);
    let mut height = Canvas::new(SIZE, 0.5);
    let mut mix = Canvas::new(SIZE, 0.0);

    // Bruit fractal : plusieurs couches de bruit de plus en plus fin.
    let coarse = value_noise(SIZE, 4, &mut rng);
    let medium = value_noise(SIZE, 16, &mut rng);
    let fine = value_noise(SIZE, 64, &mut rng);
    for i in 0..SIZE * SIZE {
        height.data[i] = 0.5 * coarse[i] + 0.3 * medium[i] + 0.2 * fine[i];
        mix.data[i] = (coarse[i] * 0.8 - 0.2 + 0.5 * medium[i]).clamp(0.0, 1.0);
    }
    // Petits cailloux : bosses rondes.
    for _ in 0..260 {
        let (x, y, r) = (rng.next() * SIZE as f32, rng.next() * SIZE as f32, 1.5 + rng.next() * 3.5);
        height.fill_circle(x, y, r, 0.85);
        mix.fill_circle(x, y, r, 0.15);
    }
    // Fissures : petits traits sombres en creux.
    for _ in 0..40 {
        let (mut x, mut y) = (rng.next() * SIZE as f32, rng.next() * SIZE as f32);
        let mut angle = rng.next() * std::f32::consts::TAU;
        for _ in 0..14 {
            angle += (rng.next() - 0.5) * 1.2;
            let (nx, ny) = (x + angle.cos() * 5.0, y + angle.sin() * 5.0);
            height.line(x, y, nx, ny, 1.2, 0.2);
            x = nx;
            y = ny;
        }
    }

    let base = base.to_linear();
    let dust = dust.to_linear();
    let mut color = vec![0u8; SIZE * SIZE * 4];
    for i in 0..SIZE * SIZE {
        let t = mix.data[i];
        // Les creux sont plus sombres, les bosses plus claires.
        let shade = 0.8 + 0.4 * height.data[i];
        let c = LinearRgba::from_vec3(base.to_vec3().lerp(dust.to_vec3(), t) * shade);
        let srgb = Srgba::from(c).to_u8_array();
        color[i * 4..i * 4 + 4].copy_from_slice(&srgb);
    }
    if grid_lines {
        // Lignes un peu plus claires (marquage au sol) sur les bords et au milieu.
        for i in 0..SIZE {
            for line in [0, SIZE / 2] {
                for idx in [line * SIZE + i, i * SIZE + line] {
                    for channel in 0..3 {
                        let v = color[idx * 4 + channel] as f32;
                        color[idx * 4 + channel] = (v * 1.12 + 3.0).min(255.0) as u8;
                    }
                }
            }
        }
    }

    DetailImages {
        albedo: rgba_image(SIZE, color, TextureFormat::Rgba8UnormSrgb, false),
        normal: height.to_normal_image(4.0),
    }
}

/// Textures d'un sol de velours, comme le fond d'un écrin à bijoux : pas de
/// cailloux ni de fissures, seulement de grandes moirures douces entre `base`
/// et `dust` (le velours froissé change de teinte selon le sens du poil) et
/// un grain très fin. Le sol reste sobre : il fait ressortir les unités.
pub fn velvet_images(base: Color, dust: Color) -> DetailImages {
    const SIZE: usize = 512;
    let mut rng = Rng::new(5);
    let coarse = value_noise(SIZE, 4, &mut rng);
    let medium = value_noise(SIZE, 12, &mut rng);
    let fine = value_noise(SIZE, 128, &mut rng);
    let mut height = Canvas::new(SIZE, 0.5);
    let mut color = vec![0u8; SIZE * SIZE * 4];
    let (base, dust) = (base.to_linear().to_vec3(), dust.to_linear().to_vec3());
    for i in 0..SIZE * SIZE {
        // Moirures : le bruit grossier, resserré en bandes douces.
        let moire = (0.65 * coarse[i] + 0.35 * medium[i] - 0.25).clamp(0.0, 1.0);
        height.data[i] = 0.5 + 0.08 * fine[i] + 0.05 * medium[i];
        let shade = 0.94 + 0.12 * fine[i];
        let c = LinearRgba::from_vec3(base.lerp(dust, moire) * shade);
        color[i * 4..i * 4 + 4].copy_from_slice(&Srgba::from(c).to_u8_array());
    }
    DetailImages {
        albedo: rgba_image(SIZE, color, TextureFormat::Rgba8UnormSrgb, false),
        normal: height.to_normal_image(1.5),
    }
}

/// Panneaux de blindage : joints, panneaux en relief, rivets, grilles d'aération.
/// Inspiré de la page de direction artistique (`panelTex`).
fn panels(worn: bool) -> (Canvas, Canvas) {
    let n = DETAIL_SIZE as f32;
    let mut rng = Rng::new(if worn { 23 } else { 41 });
    let mut height = Canvas::new(DETAIL_SIZE, 0.5);
    let mut albedo = Canvas::new(DETAIL_SIZE, 0.95);

    if worn {
        // Fines taches de saleté.
        for _ in 0..1500 {
            let (x, y) = (rng.next() * n, rng.next() * n);
            let size = 1.0 + rng.next() * 3.0;
            albedo.multiply_rect(x, y, size, size, 1.0 - rng.next() * 0.08);
        }
    }
    // Panneaux rapportés : légèrement en relief, cernés d'une rainure, un rivet à chaque coin.
    for _ in 0..if worn { 8 } else { 4 } {
        let (w, h) = (40.0 + rng.next() * 90.0, 30.0 + rng.next() * 80.0);
        let (x, y) = (8.0 + rng.next() * (n - w - 16.0), 8.0 + rng.next() * (n - h - 16.0));
        height.fill_rect(x, y, w, h, 0.61);
        height.stroke_rect(x, y, w, h, 2.5, 0.22);
        albedo.multiply_rect(x, y, w, h, 1.04);
        albedo.stroke_rect(x, y, w, h, 1.5, 0.45);
        for (cx, cy) in [(x + 6.0, y + 6.0), (x + w - 6.0, y + 6.0), (x + 6.0, y + h - 6.0), (x + w - 6.0, y + h - 6.0)] {
            height.fill_circle(cx, cy, 2.3, 0.83);
            albedo.fill_circle(cx, cy, 2.4, 0.6);
        }
    }
    // Joints principaux : une grille de deux panneaux par côté.
    for p in [0.0, n * 0.5] {
        height.fill_rect(p, 0.0, 3.5, n, 0.18);
        height.fill_rect(0.0, p, n, 3.5, 0.18);
        albedo.fill_rect(p, 0.0, 2.0, n, 0.35);
        albedo.fill_rect(0.0, p, n, 2.0, 0.35);
    }
    // Grilles d'aération : cinq fentes sombres.
    for _ in 0..if worn { 3 } else { 1 } {
        let (x, y) = (16.0 + rng.next() * (n - 70.0), 16.0 + rng.next() * (n - 60.0));
        for k in 0..5 {
            height.fill_rect(x, y + k as f32 * 6.0, 28.0, 3.0, 0.11);
            albedo.fill_rect(x, y + k as f32 * 6.0, 28.0, 3.0, 0.25);
        }
    }
    if worn {
        // Rayures claires (peinture écaillée) et grandes taches d'usure.
        for _ in 0..46 {
            let (x, y) = (rng.next() * n, rng.next() * n);
            let (dx, dy) = ((rng.next() - 0.5) * 34.0, (rng.next() - 0.5) * 34.0);
            albedo.line(x, y, x + dx, y + dy, 0.8, 1.0 + 0.12 + rng.next() * 0.3);
        }
        for _ in 0..12 {
            let (x, y, r) = (rng.next() * n, rng.next() * n, 14.0 + rng.next() * 30.0);
            albedo.soft_spot(x, y, r, 0.78);
        }
    }
    (height, albedo)
}

/// Grille d'hexagones en creux (`hexTex` de la page de direction artistique).
fn hexagons() -> (Canvas, Canvas) {
    let n = DETAIL_SIZE as f32;
    let mut height = Canvas::new(DETAIL_SIZE, 0.5);
    let mut albedo = Canvas::new(DETAIL_SIZE, 0.92);
    // 4 colonnes et 4 rangées (nombre pair) d'hexagones : le motif se raccorde.
    let width = n / 4.0;
    let side = width / 3f32.sqrt();
    let row_height = n / 4.0;
    for row in -1..=4 {
        for col in -1..=4 {
            let cx = (col as f32 + if row % 2 != 0 { 0.5 } else { 0.0 }) * width;
            let cy = row as f32 * row_height;
            let corners: Vec<Vec2> = (0..6)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_3 * i as f32 + std::f32::consts::FRAC_PI_6;
                    // Écrasé verticalement pour que 4 rangées remplissent exactement l'image.
                    Vec2::new(cx + a.cos() * side, cy + a.sin() * side * row_height / (1.5 * side))
                })
                .collect();
            for i in 0..6 {
                let (a, b) = (corners[i], corners[(i + 1) % 6]);
                height.line(a.x, a.y, b.x, b.y, 3.5, 0.16);
                albedo.line(a.x, a.y, b.x, b.y, 2.0, 0.45);
            }
        }
    }
    (height, albedo)
}

/// Écorce striée (`barkTex` de la page de direction artistique) : 95 fibres
/// verticales qui ondulent doucement. Les fibres sombres sont des creux
/// (et assombrissent la couleur), les claires des crêtes.
fn bark() -> (Canvas, Canvas) {
    let n = DETAIL_SIZE as f32;
    let mut rng = Rng::new(59);
    let mut height = Canvas::new(DETAIL_SIZE, 0.45);
    let mut albedo = Canvas::new(DETAIL_SIZE, 0.86);
    for _ in 0..95 {
        let x = rng.next() * n;
        let width = 1.0 + rng.next() * 5.0;
        let dark = rng.next() < 0.6;
        let amplitude = 3.0 + rng.next() * 8.0;
        let phase = rng.next() * std::f32::consts::TAU;
        // Un nombre entier d'ondulations sur la hauteur de l'image : la fibre
        // se raccorde au bord du haut et du bas.
        let waves = 1.0 + (rng.next() * 3.0).floor();
        let x_at = |y: f32| x + (y / n * std::f32::consts::TAU * waves + phase).sin() * amplitude;
        // La fibre est tracée en petits segments de 8 pixels.
        let mut y = 0.0;
        while y < n {
            let next = y + 8.0;
            height.line(x_at(y), y, x_at(next), next, width, if dark { 0.15 } else { 0.74 });
            if dark {
                albedo.line(x_at(y), y, x_at(next), next, width, 0.53);
            }
            y = next;
        }
    }
    (height, albedo)
}

/// Chevrons de danger (`stripeTex` de la page de direction artistique) :
/// bandes diagonales à 45°, claires et presque noires, un peu usées.
fn chevrons() -> (Canvas, Canvas) {
    let n = DETAIL_SIZE as f32;
    // 4 paires de bandes sur la largeur : 256 est divisible par la période,
    // donc le motif se raccorde sur les bords.
    let period = n / 4.0;
    let mut rng = Rng::new(57);
    let mut height = Canvas::new(DETAIL_SIZE, 0.5);
    let mut albedo = Canvas::new(DETAIL_SIZE, 1.0);
    for y in 0..DETAIL_SIZE {
        for x in 0..DETAIL_SIZE {
            // Position le long de la diagonale, ramenée à une période (0 à 1).
            let t = ((x + y) as f32 / period).fract();
            // Bord adouci sur un pixel, pour éviter l'escalier.
            let edge = 1.5 / period;
            let dark = smooth_band(t, 0.5, edge);
            let i = y * DETAIL_SIZE + x;
            albedo.data[i] = 0.95 - 0.89 * dark;
            // La peinture noire fait une très légère surépaisseur.
            height.data[i] = 0.5 + 0.06 * dark;
        }
    }
    // Usure : petites écailles claires (métal à nu) et taches de saleté.
    for _ in 0..60 {
        let (x, y, r) = (rng.next() * n, rng.next() * n, 0.8 + rng.next() * 2.2);
        albedo.fill_circle(x, y, r, 0.55);
        height.fill_circle(x, y, r, 0.4);
    }
    for _ in 0..8 {
        let (x, y, r) = (rng.next() * n, rng.next() * n, 14.0 + rng.next() * 24.0);
        albedo.soft_spot(x, y, r, 0.8);
    }
    (height, albedo)
}

/// 1 quand `t` (entre 0 et 1, périodique) est entre `start` et 1, 0 ailleurs,
/// avec des transitions adoucies de largeur `edge`.
fn smooth_band(t: f32, start: f32, edge: f32) -> f32 {
    let ramp = |x: f32| (x / edge + 0.5).clamp(0.0, 1.0);
    // Montée en `start`, descente en 1 (qui est aussi 0 : la bande déborde
    // un demi-bord au début de la période suivante).
    ramp(t - start).min(ramp(1.0 - t)).max(ramp(-t))
}

/// Image en niveaux de gris, en nombres à virgule, qu'on peut dessiner.
/// Toutes les coordonnées « bouclent » sur les bords (motif raccordable).
struct Canvas {
    size: usize,
    data: Vec<f32>,
}

impl Canvas {
    fn new(size: usize, value: f32) -> Self {
        Self { size, data: vec![value; size * size] }
    }

    fn index(&self, x: i32, y: i32) -> usize {
        let n = self.size as i32;
        // `rem_euclid` : reste toujours positif, même pour x < 0 (bouclage).
        (y.rem_euclid(n) * n + x.rem_euclid(n)) as usize
    }

    fn get(&self, x: i32, y: i32) -> f32 {
        self.data[self.index(x, y)]
    }

    /// Applique `f` à chaque pixel du rectangle.
    fn apply_rect(&mut self, x: f32, y: f32, w: f32, h: f32, f: impl Fn(f32) -> f32) {
        for py in y.round() as i32..(y + h).round() as i32 {
            for px in x.round() as i32..(x + w).round() as i32 {
                let i = self.index(px, py);
                self.data[i] = f(self.data[i]);
            }
        }
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, value: f32) {
        self.apply_rect(x, y, w, h, |_| value);
    }

    fn multiply_rect(&mut self, x: f32, y: f32, w: f32, h: f32, factor: f32) {
        self.apply_rect(x, y, w, h, |v| v * factor);
    }

    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32, value: f32) {
        let half = width / 2.0;
        self.fill_rect(x - half, y - half, w + width, width, value);
        self.fill_rect(x - half, y + h - half, w + width, width, value);
        self.fill_rect(x - half, y - half, width, h + width, value);
        self.fill_rect(x + w - half, y - half, width, h + width, value);
    }

    fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, value: f32) {
        for py in (cy - r).floor() as i32..=(cy + r).ceil() as i32 {
            for px in (cx - r).floor() as i32..=(cx + r).ceil() as i32 {
                if Vec2::new(px as f32 - cx, py as f32 - cy).length() <= r {
                    let i = self.index(px, py);
                    self.data[i] = value;
                }
            }
        }
    }

    /// Tache aux bords doux qui multiplie la valeur (assombrit si `factor` < 1).
    fn soft_spot(&mut self, cx: f32, cy: f32, r: f32, factor: f32) {
        for py in (cy - r) as i32..=(cy + r) as i32 {
            for px in (cx - r) as i32..=(cx + r) as i32 {
                let d = Vec2::new(px as f32 - cx, py as f32 - cy).length() / r;
                if d < 1.0 {
                    let i = self.index(px, py);
                    self.data[i] *= 1.0 + (factor - 1.0) * (1.0 - d);
                }
            }
        }
    }

    /// Trait épais : un disque tous les demi-pixels le long du segment.
    /// `value` > 1 éclaircit (multiplie), sinon la valeur est imposée.
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, value: f32) {
        let steps = (Vec2::new(x1 - x0, y1 - y0).length() * 2.0).ceil().max(1.0) as usize;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            if value > 1.0 {
                let i = self.index(x.round() as i32, y.round() as i32);
                self.data[i] = (self.data[i] * value).min(1.0);
            } else {
                self.fill_circle(x, y, width / 2.0, value);
            }
        }
    }

    /// Image de détail en niveaux de gris (espace sRGB, comme une photo).
    fn to_albedo_image(&self) -> Image {
        let data = self
            .data
            .iter()
            .flat_map(|&v| {
                let g = (v.clamp(0.0, 1.0) * 255.0) as u8;
                [g, g, g, 255]
            })
            .collect();
        rgba_image(self.size, data, TextureFormat::Rgba8UnormSrgb, false)
    }

    /// Normal map tirée de la carte de hauteur : la pente en x et en y donne
    /// l'inclinaison de la surface. `strength` exagère plus ou moins le relief.
    fn to_normal_image(&self, strength: f32) -> Image {
        let n = self.size as i32;
        let mut data = Vec::with_capacity(self.data.len() * 4);
        for y in 0..n {
            for x in 0..n {
                let dx = (self.get(x + 1, y) - self.get(x - 1, y)) * strength;
                let dy = (self.get(x, y + 1) - self.get(x, y - 1)) * strength;
                let normal = Vec3::new(-dx, dy, 1.0).normalize();
                let encoded = normal * 0.5 + 0.5;
                data.extend_from_slice(&[
                    (encoded.x * 255.0) as u8,
                    (encoded.y * 255.0) as u8,
                    (encoded.z * 255.0) as u8,
                    255,
                ]);
            }
        }
        // Une normal map n'est pas une couleur : format linéaire, pas sRGB.
        rgba_image(self.size, data, TextureFormat::Rgba8Unorm, true)
    }
}

/// Construit une image répétable, avec toute sa chaîne de **mipmaps** :
/// des copies de plus en plus petites (½, ¼…) que la carte graphique utilise
/// quand la surface est vue de loin. Sans elles, une texture fine scintille
/// dès qu'on dézoome.
fn rgba_image(size: usize, data: Vec<u8>, format: TextureFormat, is_normal_map: bool) -> Image {
    let mut all_levels = data.clone();
    let mut level = data;
    let mut level_size = size;
    let mut mip_count = 1;
    while level_size > 1 {
        let half = level_size / 2;
        let mut next = vec![0u8; half * half * 4];
        for y in 0..half {
            for x in 0..half {
                // Moyenne des 4 pixels du niveau précédent.
                let mut sum = Vec4::ZERO;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let i = ((y * 2 + dy) * level_size + x * 2 + dx) * 4;
                    sum += Vec4::new(level[i] as f32, level[i + 1] as f32, level[i + 2] as f32, level[i + 3] as f32);
                }
                let mut avg = sum / 4.0;
                if is_normal_map {
                    // Une moyenne de normales n'a plus une longueur de 1 : on la renormalise.
                    let n = (avg.truncate() / 127.5 - 1.0).normalize_or(Vec3::Z);
                    avg = ((n + 1.0) * 127.5).extend(255.0);
                }
                let o = (y * half + x) * 4;
                next[o..o + 4].copy_from_slice(&[avg.x as u8, avg.y as u8, avg.z as u8, avg.w as u8]);
            }
        }
        all_levels.extend_from_slice(&next);
        level = next;
        level_size = half;
        mip_count += 1;
    }

    let mut image = Image::new_uninit(
        Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(all_levels);
    image.texture_descriptor.mip_level_count = mip_count;
    // Répéter l'image au-delà de [0, 1], filtrage lisse entre pixels et entre
    // mipmaps, et filtrage anisotrope pour les surfaces vues en biais (le sol).
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

/// Bruit de valeur raccordable : une grille `cells` × `cells` de valeurs
/// aléatoires, interpolées en douceur. Valeurs entre 0 et 1.
fn value_noise(size: usize, cells: usize, rng: &mut Rng) -> Vec<f32> {
    let lattice: Vec<f32> = (0..cells * cells).map(|_| rng.next()).collect();
    let at = |x: usize, y: usize| lattice[(y % cells) * cells + (x % cells)];
    let scale = cells as f32 / size as f32;
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 * scale, y as f32 * scale);
            let (ix, iy) = (fx as usize, fy as usize);
            // Courbe en S (smoothstep) : pas d'angle visible entre les cellules.
            let (tx, ty) = (fx.fract(), fy.fract());
            let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
            let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * sx;
            let bottom = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * sx;
            out.push(top + (bottom - top) * sy);
        }
    }
    out
}

/// Petit générateur pseudo-aléatoire (Park-Miller) : toujours la même suite
/// pour une même graine, donc les mêmes textures à chaque lancement.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Nombre entre 0 et 1.
    fn next(&mut self) -> f32 {
        self.0 = self.0 * 16807 % 2_147_483_647;
        (self.0 - 1) as f32 / 2_147_483_646.0
    }
}
