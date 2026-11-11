//! Rendu : réglages graphiques génériques de la caméra RTS et de la scène.
//!
//! Ce module ne contient aucune couleur de faction : le jeu fournit une
//! [`Ambiance`] (couleurs des lumières, du ciel, du brouillard…) et choisit
//! une [`GraphicsQuality`] ; le moteur en déduit tout le reste :
//!
//! - **HDR** (« high dynamic range ») : l'image est d'abord calculée avec des
//!   valeurs de lumière qui peuvent dépasser le blanc (un réacteur peut être
//!   cinq fois plus lumineux que le blanc de l'écran) ;
//! - **tone mapping** filmique : ramène ces valeurs dans ce que l'écran sait
//!   afficher, en adoucissant les hautes lumières comme une pellicule ;
//! - **bloom** : un halo autour de ce qui dépasse le blanc (visières, réacteurs) ;
//! - **ombres en cascade** recadrées en permanence sur ce que voit la caméra ;
//! - **brouillard** de distance, **IBL** (voir [`environment`]), anticrénelage,
//!   lumière principale et contre-jours colorés, particules d'ambiance,
//!   pièces animées (voir [`animation`]).
//!
//! La qualité Basse coupe ce qui coûte cher (HDR, bloom, ombres fines) pour
//! tenir 300 unités par joueur sur iPad.

pub mod animation;
pub mod environment;
mod particles;
pub mod shapes;
pub mod textures;

use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::{Exposure, Hdr};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap, ShadowFilteringMethod};
use bevy::post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::prelude::*;

use crate::camera::RtsCamera;
use environment::{EnvironmentDescription, Lobe};

/// Niveau de qualité graphique, choisi par le jeu selon l'appareil.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsQuality {
    /// Pour l'iPad et les machines modestes : pas de HDR ni de bloom, une
    /// seule carte d'ombre, anticrénelage FXAA (une passe légère sur l'image).
    Low,
    /// Pour ordinateur : HDR, bloom, ombres en 3 cascades, MSAA ×4.
    #[default]
    High,
}

impl GraphicsQuality {
    /// Côté de chaque carte d'ombre, en pixels.
    fn shadow_map_size(self) -> usize {
        match self {
            GraphicsQuality::Low => 2048,
            GraphicsQuality::High => 2048,
        }
    }

    /// Nombre de cascades d'ombre (voir [`fit_shadow_cascades`]).
    fn shadow_cascades(self) -> usize {
        match self {
            GraphicsQuality::Low => 1,
            GraphicsQuality::High => 3,
        }
    }
}

/// Une lumière directionnelle (soleil, contre-jour).
#[derive(Debug, Clone, Copy)]
pub struct SceneLight {
    pub color: Color,
    /// Éclairement en lux.
    pub illuminance: f32,
    /// Direction *d'où vient* la lumière (pas besoin de la normaliser).
    pub from: Vec3,
}

/// L'éclairage d'ambiance par image (IBL) : un ciel, un sol, et le reflet
/// des lumières de la scène dans les surfaces métalliques.
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentSettings {
    /// Couleur de la lumière qui tombe du ciel.
    pub sky: Color,
    /// Couleur de la lumière renvoyée par le sol.
    pub ground: Color,
    /// Couleur du fond, dans toutes les directions.
    pub background: Color,
    /// Intensité de chaque lumière dans les reflets (0 : pas de reflet).
    pub light_reflections: f32,
    /// Multiplicateur général, en cd/m².
    pub intensity: f32,
}

/// Brouillard de distance, exprimé en multiples de la distance entre la
/// caméra et le point visé : il reste ainsi cohérent à tous les niveaux de zoom.
#[derive(Debug, Clone, Copy)]
pub struct FogSettings {
    pub start: f32,
    pub end: f32,
}

/// Particules d'ambiance (braises, spores, poussière).
#[derive(Debug, Clone, Copy)]
pub struct ParticleSettings {
    pub color: Color,
    /// Nombre de particules en qualité Haute (un tiers en Basse).
    pub count: usize,
    /// Luminosité (au-delà de 1, les particules produisent du bloom).
    pub glow: f32,
}

/// Un panneau lumineux qui ne se voit que dans les reflets (comme les boîtes
/// à lumière d'un photographe de bijoux) : il n'éclaire pas la scène, mais
/// dessine un reflet net sur les métaux polis et les pierres.
#[derive(Debug, Clone, Copy)]
pub struct ReflectionPanel {
    pub color: Color,
    /// Intensité, relative au ciel de l'environnement (1 : aussi lumineux).
    pub intensity: f32,
    /// Direction d'où vient le reflet (pas besoin de la normaliser).
    pub from: Vec3,
    /// Netteté : 14 ≈ les reflets des lumières de la scène ; 60 ≈ une petite
    /// tache de 7°, des reflets nets et contrastés.
    pub sharpness: f32,
}

/// Panneaux de reflet ajoutés à l'éclairage d'ambiance (facultatif) : le jeu
/// insère cette ressource pour une ambiance « vitrine de bijoutier ».
#[derive(Resource, Debug, Clone, Default)]
pub struct ReflectionPanels(pub Vec<ReflectionPanel>);

/// L'ambiance visuelle d'une carte ou d'une faction : tout ce qui colore la
/// lumière. Le jeu insère cette ressource ; le moteur l'applique, et la
/// réapplique si elle change.
#[derive(Resource, Debug, Clone)]
pub struct Ambiance {
    /// Couleur du fond et du brouillard.
    pub background: Color,
    /// Exposition de la caméra (EV100) : plus elle est haute, plus l'image est sombre.
    pub exposure: f32,
    /// Lumière principale, la seule qui projette des ombres.
    pub sun: SceneLight,
    /// Lumières d'appoint sans ombre, typiquement des contre-jours colorés
    /// qui détachent les silhouettes du sol.
    pub fill_lights: Vec<SceneLight>,
    pub environment: EnvironmentSettings,
    pub fog: Option<FogSettings>,
    /// Force du bloom (qualité Haute seulement).
    pub bloom: f32,
    pub particles: Option<ParticleSettings>,
}

impl Default for Ambiance {
    /// Une lumière de jour neutre, sans contre-jour.
    fn default() -> Self {
        Self {
            background: Color::srgb(0.05, 0.06, 0.08),
            exposure: Exposure::EV100_BLENDER,
            sun: SceneLight {
                color: Color::WHITE,
                illuminance: 4000.0,
                from: Vec3::new(-0.4, 1.0, 0.5),
            },
            fill_lights: Vec::new(),
            environment: EnvironmentSettings {
                sky: Color::srgb(0.6, 0.65, 0.75),
                ground: Color::srgb(0.2, 0.2, 0.2),
                background: Color::srgb(0.1, 0.1, 0.1),
                light_reflections: 1.0,
                intensity: 300.0,
            },
            fog: None,
            bloom: 0.0,
            particles: None,
        }
    }
}

/// Animation de flottement (unités volantes) : monte et descend doucement.
/// À placer sur une entité enfant, jamais sur la racine pilotée par la simulation.
#[derive(Component, Debug, Clone, Copy)]
pub struct Hover {
    /// Amplitude du mouvement, en mètres.
    pub amplitude: f32,
    /// Oscillations par seconde.
    pub frequency: f32,
    /// Décalage pour que deux unités voisines ne flottent pas en même temps.
    pub phase: f32,
    /// Hauteur de repos de l'entité.
    pub base_height: f32,
}

/// Marque la lumière principale (celle qui projette les ombres).
#[derive(Component)]
struct SunLight;

/// Marque toutes les lumières créées à partir de l'ambiance.
#[derive(Component)]
struct AmbianceLight;

/// Cubemaps d'éclairage d'ambiance, recalculées quand l'ambiance change.
#[derive(Resource)]
struct EnvironmentMaps {
    diffuse: Handle<Image>,
    specular: Handle<Image>,
}

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<animation::PulsingMaterials>()
            .init_resource::<GraphicsQuality>()
            .init_resource::<Ambiance>()
            // L'IBL remplace la lumière ambiante uniforme de Bevy.
            .insert_resource(GlobalAmbientLight { brightness: 0.0, ..default() })
            .add_systems(
                Update,
                (
                    (rebuild_environment, configure_cameras, spawn_lights).chain(),
                    fit_shadow_cascades,
                    update_fog,
                    animate_hover,
                    // Pièces animées et matériaux qui pulsent (voir `animation`).
                    animation::animate_parts,
                    (animation::apply_pulse_materials, animation::animate_pulses).chain(),
                    particles::respawn_particles,
                    particles::animate_particles,
                ),
            );
    }
}

/// Recalcule les cubemaps d'IBL (au démarrage, puis si l'ambiance change).
///
/// `Option<Res<…>>` : la ressource `ReflectionPanels` peut ne pas exister ;
/// on reçoit alors `None` au lieu d'une erreur.
fn rebuild_environment(
    mut commands: Commands,
    ambiance: Res<Ambiance>,
    panels: Option<Res<ReflectionPanels>>,
    mut images: ResMut<Assets<Image>>,
) {
    let panels_changed = panels.as_ref().is_some_and(|p| p.is_changed());
    if !ambiance.is_changed() && !panels_changed {
        return;
    }
    let env = &ambiance.environment;
    let linear = |c: Color| c.to_linear().to_vec3();
    // Le ciel et le sol : deux grandes taches très floues, en haut et en bas.
    let mut lobes = vec![
        Lobe { direction: Vec3::Y, color: linear(env.sky), sharpness: 1.2 },
        Lobe { direction: Vec3::NEG_Y, color: linear(env.ground), sharpness: 1.2 },
    ];
    // Chaque lumière se reflète comme une boîte à lumière de photographe :
    // une tache assez nette dans sa direction.
    for light in std::iter::once(&ambiance.sun).chain(&ambiance.fill_lights) {
        let strength = env.light_reflections * light.illuminance / ambiance.sun.illuminance.max(1.0);
        lobes.push(Lobe {
            direction: light.from.normalize_or(Vec3::Y),
            color: linear(light.color) * strength * 4.0,
            sharpness: 14.0,
        });
    }
    for panel in panels.iter().flat_map(|p| p.0.iter()) {
        lobes.push(Lobe {
            direction: panel.from.normalize_or(Vec3::Y),
            color: linear(panel.color) * panel.intensity,
            sharpness: panel.sharpness,
        });
    }
    let description = EnvironmentDescription { background: linear(env.background), lobes };
    let (diffuse, specular) = description.build_cubemaps();
    commands.insert_resource(EnvironmentMaps {
        diffuse: images.add(diffuse),
        specular: images.add(specular),
    });
    commands.insert_resource(ClearColor(ambiance.background));
}

/// Applique qualité et ambiance aux caméras RTS : à leur création, puis à
/// chaque changement de réglage.
fn configure_cameras(
    mut commands: Commands,
    quality: Res<GraphicsQuality>,
    ambiance: Res<Ambiance>,
    env: Option<Res<EnvironmentMaps>>,
    cameras: Query<(Entity, Ref<RtsCamera>)>,
) {
    let Some(env) = env else { return };
    let settings_changed = quality.is_changed() || ambiance.is_changed() || env.is_changed();
    for (entity, camera) in &cameras {
        if !settings_changed && !camera.is_added() {
            continue;
        }
        let mut entity = commands.entity(entity);
        entity.insert((
            // AcesFitted : la courbe filmique la plus proche de la page de direction artistique.
            Tonemapping::AcesFitted,
            Exposure { ev100: ambiance.exposure },
            EnvironmentMapLight {
                diffuse_map: env.diffuse.clone(),
                specular_map: env.specular.clone(),
                intensity: ambiance.environment.intensity,
                ..default()
            },
        ));
        match *quality {
            GraphicsQuality::High => {
                entity.remove::<Fxaa>().insert((
                    Hdr,
                    // MSAA : la carte graphique calcule 4 échantillons par pixel sur
                    // les bords des triangles, ce qui lisse les arêtes en escalier.
                    Msaa::Sample4,
                    Bloom {
                        intensity: ambiance.bloom,
                        // Seul ce qui dépasse nettement le blanc produit un halo :
                        // la scène reste nette, les pièces émissives brillent.
                        prefilter: BloomPrefilter { threshold: 1.0, threshold_softness: 0.5 },
                        composite_mode: BloomCompositeMode::Additive,
                        ..Bloom::NATURAL
                    },
                    // Ombres filtrées par un flou gaussien : bords doux.
                    ShadowFilteringMethod::Gaussian,
                ));
            }
            GraphicsQuality::Low => {
                entity.remove::<(Hdr, Bloom)>().insert((
                    Msaa::Off,
                    Fxaa::default(),
                    ShadowFilteringMethod::Hardware2x2,
                ));
            }
        }
    }
}

/// (Re)crée la lumière principale et les contre-jours selon l'ambiance.
fn spawn_lights(
    mut commands: Commands,
    quality: Res<GraphicsQuality>,
    ambiance: Res<Ambiance>,
    existing: Query<Entity, With<AmbianceLight>>,
) {
    if !ambiance.is_changed() && !quality.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    commands.insert_resource(DirectionalLightShadowMap { size: quality.shadow_map_size() });

    let transform = |light: &SceneLight| {
        Transform::default().looking_to(-light.from.normalize_or(Vec3::Y), Vec3::Y)
    };
    let sun = &ambiance.sun;
    commands.spawn((
        Name::new("Soleil"),
        AmbianceLight,
        SunLight,
        DirectionalLight {
            color: sun.color,
            illuminance: sun.illuminance,
            shadow_maps_enabled: true,
            // Petit décalage pour éviter que les surfaces s'ombrent elles-mêmes
            // en rayures (« acné d'ombre »), sans décoller l'ombre des pieds.
            shadow_depth_bias: 0.03,
            shadow_normal_bias: 1.2,
            ..default()
        },
        transform(sun),
    ));
    for light in &ambiance.fill_lights {
        commands.spawn((
            Name::new("Contre-jour"),
            AmbianceLight,
            DirectionalLight {
                color: light.color,
                illuminance: light.illuminance,
                shadow_maps_enabled: false,
                ..default()
            },
            transform(light),
        ));
    }
}

/// Recadre les cascades d'ombre sur la portion de sol réellement visible.
///
/// Les **ombres en cascade** découpent la vue en tranches de profondeur, chacune
/// avec sa propre carte d'ombre : les tranches proches sont fines et précises,
/// les lointaines plus grossières. Une caméra RTS ne voit jamais l'horizon :
/// du plus proche au plus lointain point visible du sol, l'écart est faible
/// (de 0,8 à 1,6 fois la distance de zoom). En recalculant ces bornes à chaque
/// mouvement de caméra, aucune résolution n'est gaspillée sur ce qui est hors
/// de l'écran, de 10 m comme de 70 m.
fn fit_shadow_cascades(
    quality: Res<GraphicsQuality>,
    cameras: Query<(Ref<GlobalTransform>, &Projection), With<RtsCamera>>,
    mut suns: Query<(Ref<SunLight>, &mut CascadeShadowConfig)>,
) {
    let Some((camera_transform, projection)) = cameras.iter().next() else { return };
    let Projection::Perspective(perspective) = projection else { return };
    for (sun, mut cascades) in &mut suns {
        if !camera_transform.is_changed() && !quality.is_changed() && !sun.is_added() {
            continue;
        }
        let (near, far) = visible_depth_range(&camera_transform, perspective);
        let count = quality.shadow_cascades();
        *cascades = CascadeShadowConfigBuilder {
            num_cascades: count,
            minimum_distance: near * 0.8,
            maximum_distance: far * 1.1,
            first_cascade_far_bound: near + (far - near) * 0.3,
            overlap_proportion: 0.2,
        }
        .build();
    }
}

/// Profondeurs (le long de l'axe de visée) du point visible le plus proche et
/// du plus lointain, entre le sol et une hauteur d'unité.
fn visible_depth_range(transform: &GlobalTransform, perspective: &PerspectiveProjection) -> (f32, f32) {
    // Hauteur maximale des objets qui portent ou reçoivent des ombres.
    const OBJECT_HEIGHT: f32 = 4.0;
    const FALLBACK_FAR: f32 = 200.0;
    let origin = transform.translation();
    let forward = transform.forward().as_vec3();
    let half_height = (perspective.fov / 2.0).tan();
    let half_width = half_height * perspective.aspect_ratio;
    let (mut near, mut far) = (f32::INFINITY, 0.0f32);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        // Rayon qui passe par un coin de l'écran.
        let ray = (transform.rotation() * Vec3::new(sx * half_width, sy * half_height, -1.0)).normalize();
        for height in [0.0, OBJECT_HEIGHT] {
            let depth = if ray.y < -1e-3 && origin.y > height {
                let distance = (origin.y - height) / -ray.y;
                (ray * distance).dot(forward)
            } else {
                FALLBACK_FAR
            };
            near = near.min(depth);
            far = far.max(depth.min(FALLBACK_FAR));
        }
    }
    let near = near.max(perspective.near + 0.1);
    (near, far.max(near + 1.0))
}

/// Brouillard proportionnel au zoom (voir [`FogSettings`]).
fn update_fog(
    mut commands: Commands,
    ambiance: Res<Ambiance>,
    cameras: Query<(Entity, &RtsCamera, Option<&DistanceFog>)>,
) {
    for (entity, camera, fog) in &cameras {
        let Some(settings) = ambiance.fog else {
            if fog.is_some() {
                commands.entity(entity).remove::<DistanceFog>();
            }
            continue;
        };
        let (start, end) = (settings.start * camera.distance, settings.end * camera.distance);
        // Rien à faire si le brouillard est déjà réglé pour ce zoom.
        let up_to_date = matches!(
            fog.map(|f| &f.falloff),
            Some(FogFalloff::Linear { start: s, end: e }) if *s == start && *e == end
        );
        if up_to_date && !ambiance.is_changed() {
            continue;
        }
        let falloff = FogFalloff::Linear { start, end };
        commands.entity(entity).insert(DistanceFog {
            color: ambiance.background,
            // Vers le soleil, le brouillard prend un peu de sa couleur.
            directional_light_color: ambiance.sun.color.with_alpha(0.3),
            directional_light_exponent: 12.0,
            falloff,
        });
    }
}

fn animate_hover(time: Res<Time>, mut hovering: Query<(&Hover, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (hover, mut transform) in &mut hovering {
        let wave = (t * hover.frequency * std::f32::consts::TAU + hover.phase).sin();
        transform.translation.y = hover.base_height + wave * hover.amplitude;
    }
}
