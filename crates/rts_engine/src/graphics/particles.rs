//! Particules d'ambiance : quelques braises ou poussières lumineuses qui
//! flottent devant la caméra et donnent de la profondeur à l'air.
//!
//! Purement décoratif : rien ici ne touche à la simulation. Toutes les
//! particules partagent un seul maillage et un seul matériau, donc Bevy les
//! dessine en un lot.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::{Ambiance, GraphicsQuality};
use crate::camera::RtsCamera;

/// Une particule : sa position dans un « nuage » qui suit la caméra.
#[derive(Component)]
pub(super) struct AmbientParticle {
    /// Position de départ, entre 0 et 1 sur chaque axe du nuage.
    seed: Vec3,
    /// Vitesse de montée propre à la particule.
    speed: f32,
}

/// Recrée les particules quand l'ambiance ou la qualité changent.
pub(super) fn respawn_particles(
    mut commands: Commands,
    ambiance: Res<Ambiance>,
    quality: Res<GraphicsQuality>,
    existing: Query<Entity, With<AmbientParticle>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !ambiance.is_changed() && !quality.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let Some(settings) = &ambiance.particles else { return };
    let count = match *quality {
        GraphicsQuality::Low => settings.count / 3,
        GraphicsQuality::High => settings.count,
    };

    // Sphère de rayon 1, mise à l'échelle selon le zoom (voir `animate_particles`).
    let mesh = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap_or_else(|_| Sphere::new(1.0).mesh().build()));
    let material = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        // Émissif : la particule produit sa propre lumière (et du bloom en HDR).
        emissive: settings.color.to_linear() * settings.glow,
        unlit: true,
        fog_enabled: false,
        ..default()
    });
    for i in 0..count {
        // Suite « quasi aléatoire » mais fixe : pas besoin de vrai hasard ici.
        let seed = Vec3::new(fract(i as f32 * 0.618_034), fract(i as f32 * 0.414_214), fract(i as f32 * 0.732_051));
        commands.spawn((
            AmbientParticle { seed, speed: 0.4 + fract(i as f32 * 0.236_068) },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            NotShadowCaster,
        ));
    }
}

/// Fait monter les particules en ondulant, dans un pavé centré sur le point
/// visé par la caméra et proportionnel au zoom.
pub(super) fn animate_particles(
    time: Res<Time>,
    cameras: Query<&RtsCamera>,
    mut particles: Query<(&AmbientParticle, &mut Transform)>,
) {
    let Some(camera) = cameras.iter().next() else { return };
    let extent = Vec3::new(1.6, 0.25, 1.2) * camera.distance;
    let t = time.elapsed_secs();
    // Taille proportionnelle au zoom : les particules restent visibles de loin.
    let size = 0.0022 * camera.distance;
    for (particle, mut transform) in &mut particles {
        let rise = fract(particle.seed.y + t * particle.speed / extent.y.max(1.0) * 0.6);
        let sway = (t * 0.6 + particle.seed.x * 20.0).sin() * 0.3;
        transform.translation = camera.focus
            + Vec3::new(
                (particle.seed.x - 0.5) * extent.x + sway,
                rise * extent.y,
                (particle.seed.z - 0.5) * extent.z,
            );
        // Elles rétrécissent en haut de leur course, pour disparaître en douceur.
        transform.scale = Vec3::splat(size * (1.0 - rise).sqrt());
    }
}

fn fract(x: f32) -> f32 {
    x - x.floor()
}
