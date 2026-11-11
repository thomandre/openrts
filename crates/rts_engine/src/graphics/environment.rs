//! Éclairage d'ambiance par carte d'environnement (IBL).
//!
//! L'**IBL** (« image-based lighting », éclairage par image) éclaire la scène
//! avec une image de tout ce qui entoure la caméra, rangée dans une
//! **cubemap** (six images carrées, une par face d'un cube). Chaque pixel
//! d'un objet reçoit la lumière venant de toutes les directions : c'est ce qui
//! donne leurs reflets aux métaux et un éclairage doux aux faces à l'ombre.
//!
//! Bevy attend deux cubemaps déjà « filtrées » :
//! - **diffuse** : la lumière moyenne reçue par une surface mate tournée vers
//!   chaque direction (très floue) ;
//! - **spéculaire** : les reflets, avec une chaîne de mipmaps de plus en plus
//!   floues, du métal poli (niveau 0, net) au métal rugueux (dernier niveau).
//!
//! Plutôt que de charger une photo, on décrit l'environnement par quelques
//! taches de lumière douces (des « gaussiennes sphériques ») : un ciel, un
//! sol, et un reflet par lumière de la scène. L'avantage : le flou de chaque
//! niveau se calcule exactement avec une formule, sur le processeur, une
//! seule fois. Rien ne tourne à chaque image, ce qui ménage l'iPad.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

/// Une tache de lumière dans l'environnement, de forme gaussienne sphérique :
/// `couleur × exp(netteté × (cos(angle) − 1))`, maximale dans `direction`.
#[derive(Debug, Clone, Copy)]
pub struct Lobe {
    /// Direction (normalisée) d'où vient la lumière.
    pub direction: Vec3,
    /// Couleur et intensité au centre de la tache (valeurs linéaires).
    pub color: Vec3,
    /// Plus c'est grand, plus la tache est petite (1 ≈ demi-ciel, 30 ≈ 15°).
    pub sharpness: f32,
}

/// L'environnement complet : un fond uniforme plus des taches de lumière.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentDescription {
    pub background: Vec3,
    pub lobes: Vec<Lobe>,
}

/// Côté de la cubemap spéculaire (puissance de deux).
const SPECULAR_SIZE: u32 = 64;
/// Côté de la cubemap diffuse : la lumière diffuse varie très lentement.
const DIFFUSE_SIZE: u32 = 16;
/// Netteté de la gaussienne qui imite le lobe en cosinus d'une surface mate.
const COSINE_SHARPNESS: f32 = 2.133;

impl EnvironmentDescription {
    /// Construit les cubemaps (diffuse, spéculaire) prêtes pour `EnvironmentMapLight`.
    pub fn build_cubemaps(&self) -> (Image, Image) {
        let diffuse = cubemap(DIFFUSE_SIZE, 1, |dir, _| self.convolved(dir, COSINE_SHARPNESS));

        let mip_count = SPECULAR_SIZE.trailing_zeros() + 1;
        let specular = cubemap(SPECULAR_SIZE, mip_count, |dir, mip| {
            // Bevy choisit le niveau de mip = rugosité × (nombre de niveaux − 1).
            let roughness = mip as f32 / (mip_count - 1) as f32;
            // Le reflet d'une surface de rugosité α = r² est une tache de netteté
            // ≈ 1 / (2α²) (approximation classique du lobe GGX).
            let alpha = (roughness * roughness).max(1e-3);
            self.convolved(dir, 1.0 / (2.0 * alpha * alpha))
        });
        (diffuse, specular)
    }

    /// Lumière reçue dans la direction `dir`, floutée par un noyau gaussien de
    /// netteté `kernel` (plus `kernel` est petit, plus c'est flou).
    ///
    /// Le produit de convolution de deux gaussiennes sphériques est (à peu
    /// près) une gaussienne de netteté λ₁λ₂ / (λ₁ + λ₂) : on garde la même
    /// énergie totale, mais étalée.
    fn convolved(&self, dir: Vec3, kernel: f32) -> Vec3 {
        let mut total = self.background;
        for lobe in &self.lobes {
            let l = lobe.sharpness;
            let blurred = l * kernel / (l + kernel);
            let energy = (blurred / l) * (1.0 - (-2.0 * l).exp()) / (1.0 - (-2.0 * blurred).exp());
            total += lobe.color * energy * (blurred * (lobe.direction.dot(dir) - 1.0)).exp();
        }
        total
    }
}

/// Remplit une cubemap (6 faces × `mip_count` niveaux) en appelant
/// `radiance(direction, niveau)` pour chaque pixel.
fn cubemap(size: u32, mip_count: u32, radiance: impl Fn(Vec3, u32) -> Vec3) -> Image {
    let mut data = Vec::new();
    // Ordre attendu : toutes les mipmaps de la face 0, puis celles de la face 1…
    for face in 0..6 {
        for mip in 0..mip_count {
            let level_size = (size >> mip).max(1);
            for y in 0..level_size {
                for x in 0..level_size {
                    let s = 2.0 * (x as f32 + 0.5) / level_size as f32 - 1.0;
                    let t = 2.0 * (y as f32 + 0.5) / level_size as f32 - 1.0;
                    let c = radiance(face_direction(face, s, t), mip);
                    for value in [c.x, c.y, c.z, 1.0] {
                        data.extend_from_slice(&f32_to_f16(value).to_le_bytes());
                    }
                }
            }
        }
    }

    let mut image = Image::new_uninit(
        Extent3d { width: size, height: size, depth_or_array_layers: 6 },
        TextureDimension::D2,
        // Flottants 16 bits : les lumières peuvent dépasser 1 (HDR).
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = mip_count;
    // Les 6 couches de l'image forment un cube.
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}

/// Direction du monde qui correspond au point (s, t) ∈ [-1, 1]² d'une face.
///
/// Faces dans l'ordre +X, −X, +Y, −Y, +Z, −Z (convention des cartes
/// graphiques). Les cubemaps sont « gauchères » : Bevy inverse z quand il les
/// lit, on l'inverse donc aussi ici pour retrouver la direction du monde.
fn face_direction(face: u32, s: f32, t: f32) -> Vec3 {
    let d = match face {
        0 => Vec3::new(1.0, -t, -s),
        1 => Vec3::new(-1.0, -t, s),
        2 => Vec3::new(s, 1.0, t),
        3 => Vec3::new(s, -1.0, -t),
        4 => Vec3::new(s, -t, 1.0),
        _ => Vec3::new(-s, -t, -1.0),
    };
    Vec3::new(d.x, d.y, -d.z).normalize()
}

/// Convertit un `f32` positif en flottant 16 bits (« demi-précision »).
/// Les très petites valeurs deviennent 0, les trop grandes sont plafonnées.
fn f32_to_f16(value: f32) -> u16 {
    let v = value.clamp(0.0, 65504.0);
    if v < 6.1e-5 {
        return 0;
    }
    let bits = v.to_bits();
    // Exposant : on passe du décalage 127 (f32) au décalage 15 (f16).
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    // Mantisse : on garde les 10 bits de poids fort sur 23.
    let mantissa = (bits >> 13) & 0x3ff;
    ((exponent as u32) << 10 | mantissa) as u16
}
