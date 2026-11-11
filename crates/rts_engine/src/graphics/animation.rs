//! Animations des pièces d'unités : purement visuelles.
//!
//! Elles tournent dans `Update` (à chaque image) et ne touchent qu'à
//! l'affichage : la simulation (`FixedUpdate`, positions, collisions) n'en
//! sait rien, ce qui la garde déterministe.
//!
//! Deux sortes d'animations, décrites en données :
//!
//! - un [`Motion`] fait bouger un groupe de pièces autour d'un pivot (tourelle
//!   qui balaie, radar qui tourne, flamme qui scintille). Le groupe devient un
//!   maillage séparé, enfant du corps de l'unité : à utiliser avec parcimonie ;
//! - une [`Pulse`] fait varier l'intensité d'un matériau émissif (réacteurs,
//!   gyrophares). Le matériau est partagé par toutes les pièces qui pulsent de
//!   la même façon : on ne modifie que lui, pas chaque unité (voir
//!   [`animate_pulses`] pour les précautions de performance).

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::prelude::*;

/// Mouvement d'un groupe de pièces animées, autour de son pivot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// Balayage : rotation aller-retour de ±`amplitude` radians autour de
    /// `axis`, `frequency` allers-retours par seconde.
    Swing { axis: Vec3, amplitude: f32, frequency: f32 },
    /// Rotation continue (radar, hélice) : `speed` radians par seconde.
    Spin { axis: Vec3, speed: f32 },
    /// Scintillement : l'échelle varie vite et irrégulièrement de ±`amplitude`
    /// le long de `axis` (`Vec3::Y` : une flamme qui s'allonge et se raccourcit).
    Flicker { axis: Vec3, amplitude: f32, frequency: f32 },
}

impl Motion {
    /// Déplacement à l'instant `t` (en secondes), relatif à la position de repos.
    pub fn sample(&self, t: f32) -> Transform {
        match *self {
            Motion::Swing { axis, amplitude, frequency } => {
                // Un sinus plus une harmonique lente : le balayage n'est pas
                // parfaitement régulier, comme un tireur qui cherche une cible.
                let phase = t * frequency * TAU;
                let angle = amplitude * (0.85 * phase.sin() + 0.15 * (phase * 0.5).sin());
                Transform::from_rotation(Quat::from_axis_angle(axis.normalize_or(Vec3::Y), angle))
            }
            Motion::Spin { axis, speed } => {
                Transform::from_rotation(Quat::from_axis_angle(axis.normalize_or(Vec3::Y), speed * t))
            }
            Motion::Flicker { axis, amplitude, frequency } => {
                // Deux sinus de fréquences sans rapport simple : le motif ne se
                // répète pas de façon visible.
                let phase = t * frequency * TAU;
                let wave = 0.6 * phase.sin() + 0.4 * (phase * 2.37 + 1.3).sin();
                Transform::from_scale(Vec3::ONE + axis * amplitude * wave)
            }
        }
    }
}

/// Pulsation de l'intensité d'un matériau émissif : elle varie entre `min` et
/// `max` fois sa valeur d'origine, `frequency` fois par seconde.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pulse {
    pub min: f32,
    pub max: f32,
    pub frequency: f32,
}

/// Une entité animée par un [`Motion`] : sa transformation vaut à chaque
/// image `rest` (le pivot) suivi du mouvement.
#[derive(Component, Debug, Clone, Copy)]
pub struct AnimatedPart {
    pub motion: Motion,
    pub rest: Transform,
    /// Décalage dans le temps, en secondes : deux unités voisines ne bougent pas en chœur.
    pub phase: f32,
}

pub(super) fn animate_parts(time: Res<Time>, mut parts: Query<(&AnimatedPart, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (part, mut transform) in &mut parts {
        // `Transform * Transform` : applique d'abord le mouvement, puis le pivot.
        *transform = part.rest * part.motion.sample(t + part.phase);
    }
}

impl Pulse {
    /// Facteur d'intensité à l'instant `t`.
    pub fn factor(&self, t: f32) -> f32 {
        let wave = 0.5 + 0.5 * (t * self.frequency * TAU).sin();
        self.min + (self.max - self.min) * wave
    }
}

/// À placer sur une pièce dessinée avec un `StandardMaterial` émissif : le
/// moteur remplace son matériau par une copie qui pulse (une seule copie par
/// matériau d'origine, pulsation et décalage : le dessin en lot est préservé).
#[derive(Component, Debug, Clone, Copy)]
pub struct PulsingPiece {
    pub pulse: Pulse,
    /// Décalage dans le temps, en secondes. Garder peu de valeurs différentes
    /// (chaque valeur crée un matériau).
    pub phase: f32,
}

/// Un matériau qui pulse : sa couleur émise de référence et sa pulsation.
struct PulsingMaterial {
    handle: Handle<StandardMaterial>,
    emissive: LinearRgba,
    pulse: Pulse,
    phase: f32,
}

/// Les matériaux qui pulsent, créés à la demande. La clé : le matériau
/// d'origine, la pulsation et le décalage. Un `f32` n'est pas utilisable
/// comme clé de `HashMap` (NaN ≠ NaN) : on se sert de ses bits, de simples entiers.
#[derive(Resource, Default)]
pub(super) struct PulsingMaterials {
    by_key: HashMap<(AssetId<StandardMaterial>, [u32; 4]), usize>,
    list: Vec<PulsingMaterial>,
    /// Instant de la dernière mise à jour (voir [`animate_pulses`]).
    last_update: f32,
}

/// Remplace, une seule fois, le matériau des nouvelles pièces qui pulsent.
pub(super) fn apply_pulse_materials(
    mut pieces: Query<(&PulsingPiece, &mut MeshMaterial3d<StandardMaterial>), Added<PulsingPiece>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut pulsing: ResMut<PulsingMaterials>,
) {
    for (piece, mut material) in &mut pieces {
        let Pulse { min, max, frequency } = piece.pulse;
        let key = (material.id(), [min, max, frequency, piece.phase].map(f32::to_bits));
        let index = match pulsing.by_key.get(&key) {
            Some(&index) => index,
            None => {
                let Some(base) = materials.get(&material.0).cloned() else { continue };
                let entry = PulsingMaterial {
                    emissive: base.emissive,
                    handle: materials.add(base),
                    pulse: piece.pulse,
                    phase: piece.phase,
                };
                pulsing.list.push(entry);
                let index = pulsing.list.len() - 1;
                pulsing.by_key.insert(key, index);
                index
            }
        };
        material.0 = pulsing.list[index].handle.clone();
    }
}

/// Fait varier l'émission des matériaux qui pulsent.
///
/// Attention au coût : quand un matériau change, Bevy revérifie la
/// configuration de dessin de *chaque* objet qui l'utilise. Mis à jour à
/// chaque image pour toutes les visières de 300 unités, cela coûtait environ
/// 15 ms par image. D'où deux précautions : seules quelques pièces pulsent
/// (réacteurs, gyrophares : voir le contenu du jeu), et la mise à jour n'a
/// lieu que 15 fois par seconde, ce qui suffit à l'œil pour une lueur.
pub(super) fn animate_pulses(
    time: Res<Time>,
    mut pulsing: ResMut<PulsingMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    const UPDATE_PERIOD: f32 = 1.0 / 15.0;
    let t = time.elapsed_secs();
    if pulsing.list.is_empty() || t - pulsing.last_update < UPDATE_PERIOD {
        return;
    }
    pulsing.last_update = t;
    for entry in &pulsing.list {
        // `get_mut` signale à Bevy que le matériau a changé : il est renvoyé à la carte graphique.
        if let Some(mut material) = materials.get_mut(&entry.handle) {
            material.emissive = entry.emissive * entry.pulse.factor(t + entry.phase);
        }
    }
}

// ---------------------------------------------------------------------------
// Corps qui réagit au mouvement (toutes les unités, sans données en plus).
// ---------------------------------------------------------------------------

/// Le corps d'une unité (l'entité « Corps », enfant de la racine) et son
/// état d'animation : il rebondit à chaque pas, se penche quand l'unité
/// accélère, s'incline dans les virages et recule quand elle tire. Tout est
/// déduit du déplacement affiché de la racine : aucune donnée de faction
/// n'est nécessaire, et la simulation n'en sait rien.
#[derive(Component, Debug, Clone)]
pub struct BodyMotion {
    /// Hauteur de repos du corps (sous l'origine de la racine).
    pub base_height: f32,
    /// Longueur d'une foulée, en mètres (un rebond par foulée).
    pub stride: f32,
    /// Hauteur d'un rebond, en mètres.
    pub bob: f32,
    /// Engin lourd (char, géant) : vibration et tangage au lieu de rebonds.
    pub heavy: bool,
    /// Unité volante : inclinaison dans les virages, pas de pas (son flottement
    /// est géré par `Hover`).
    pub flying: bool,
    /// Décalage du pas, entre 0 et 1 (deux unités ne marchent pas en chœur).
    pub phase: f32,
    // État, mis à jour à chaque image.
    last_position: Option<Vec2>,
    last_yaw: f32,
    distance: f32,
    speed: f32,
    accel: f32,
    turn: f32,
    recoil: f32,
    /// Altitude gagnée en vol (unités volantes légères), en mètres.
    lift: f32,
}

impl BodyMotion {
    pub fn new(base_height: f32, footprint_radius: f32, mass: f32, flying: bool, phase: f32) -> Self {
        Self {
            base_height,
            // Une foulée d'environ deux fois la largeur : petits pas pour
            // l'infanterie, grandes enjambées pour les géants.
            stride: (footprint_radius * 2.4).clamp(0.7, 4.0),
            bob: (footprint_radius * 0.09).clamp(0.03, 0.14),
            heavy: mass >= 5.0,
            flying,
            phase,
            last_position: None,
            last_yaw: 0.0,
            distance: 0.0,
            speed: 0.0,
            accel: 0.0,
            turn: 0.0,
            recoil: 0.0,
            lift: 0.0,
        }
    }
}

/// Lissage exponentiel : rapproche `value` de `target` à la vitesse `rate`
/// (par seconde), indépendamment du nombre d'images par seconde.
fn smooth(value: f32, target: f32, rate: f32, dt: f32) -> f32 {
    value + (target - value) * (1.0 - (-rate * dt).exp())
}

/// Anime le corps des unités d'après le déplacement affiché de leur racine.
/// Tourne après le placement des racines (`sync_transforms`), avant la
/// propagation des transformations.
pub(crate) fn animate_bodies(
    time: Res<Time>,
    mut shots: MessageReader<crate::combat::CombatEvent>,
    roots: Query<(&Transform, &Children), Without<BodyMotion>>,
    mut bodies: Query<(&mut BodyMotion, &mut Transform, &ChildOf)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    // Recul : l'unité qui tire (sa racine) secoue son corps.
    for event in shots.read() {
        let crate::combat::CombatEvent::Shot { attacker, .. } = *event else { continue };
        let Ok((_, children)) = roots.get(attacker) else { continue };
        for child in children.iter() {
            if let Ok((mut motion, ..)) = bodies.get_mut(child) {
                motion.recoil = 1.0;
            }
        }
    }
    let t = time.elapsed_secs();
    for (mut motion, mut transform, child_of) in &mut bodies {
        let Ok((root, _)) = roots.get(child_of.parent()) else { continue };
        let position = root.translation.xz();
        let yaw = root.rotation.to_euler(EulerRot::YXZ).0;
        let Some(last) = motion.last_position else {
            motion.last_position = Some(position);
            motion.last_yaw = yaw;
            continue;
        };
        let step = position.distance(last);
        // Un saut (apparition, téléportation) n'est pas un pas.
        let step = if step > 2.0 { 0.0 } else { step };
        let speed = step / dt;
        let previous_speed = motion.speed;
        motion.speed = smooth(motion.speed, speed, 10.0, dt);
        motion.accel = smooth(motion.accel, (motion.speed - previous_speed) / dt, 6.0, dt);
        // Virage : vitesse de rotation, l'angle ramené entre −π et π.
        let mut turned = yaw - motion.last_yaw;
        turned = (turned + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
        motion.turn = smooth(motion.turn, turned / dt, 6.0, dt);
        motion.distance += step;
        motion.last_position = Some(position);
        motion.last_yaw = yaw;
        motion.recoil = (motion.recoil - dt * 6.0).max(0.0);

        // 0 à l'arrêt, 1 en pleine marche.
        let moving = (motion.speed / 1.5).min(1.0);
        let cycle = (motion.distance / motion.stride + motion.phase) * TAU;
        // `if` est une expression en Rust : chaque branche donne le triplet.
        let (height, mut pitch, roll) = if motion.flying {
            // En vol, il prend encore un peu d'altitude (les volants légers :
            // jetpacks, graines, guêpes) et la reperd à l'arrêt ; il pique du nez en
            // avançant et s'incline franchement dans les virages.
            // (Au repos, il vole déjà à `units::FLIGHT_FLOOR` au moins.)
            let ceiling = if motion.heavy { 0.2 } else { 0.5 };
            motion.lift = smooth(motion.lift, ceiling * moving, 2.5, dt);
            let roll = (-motion.turn * 0.22).clamp(-0.5, 0.5);
            let pitch = -0.18 * moving - (motion.accel * 0.03).clamp(-0.15, 0.15);
            (motion.lift, pitch, roll)
        } else if motion.heavy {
            // Engin lourd : vibration rapide et tangage à l'accélération.
            let height = (t * 31.0 + motion.phase * TAU).sin() * 0.012 * moving;
            (height, (motion.accel * 0.015).clamp(-0.05, 0.05), (-motion.turn * 0.03).clamp(-0.04, 0.04))
        } else {
            // Un rebond par foulée (|sin| : deux appuis par cycle), un
            // balancement d'un pied sur l'autre, le buste penché en avant.
            let height = cycle.sin().abs() * motion.bob * moving;
            (height, -0.07 * moving - (motion.accel * 0.02).clamp(-0.08, 0.08), cycle.sin() * 0.05 * moving)
        };
        // Recul : le corps part en arrière (+z : le devant est −z) et se cabre.
        let recoil = motion.recoil * motion.recoil;
        pitch += recoil * 0.08;
        let back = recoil * 0.07;

        let mut wanted = *transform;
        wanted.translation.y = if motion.flying {
            // Le flottement (`Hover`) vient de replacer le corps : on ajoute l'altitude.
            transform.translation.y + height
        } else {
            motion.base_height + height
        };
        wanted.translation.z = back;
        wanted.rotation = Quat::from_euler(EulerRot::YXZ, 0.0, pitch, roll);
        // N'écrire que si ça change : une unité immobile ne coûte rien à la
        // propagation des transformations.
        transform.set_if_neq(wanted);
    }
}
