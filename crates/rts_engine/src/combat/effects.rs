//! Lisibilité du combat : barres de vie, traits de tir, impacts, épaves,
//! repères d'ordres. **Affichage seulement** : ces systèmes tournent dans
//! `Update`, lisent la simulation (PV, cibles) et les [`CombatEvent`], et
//! ne modifient jamais l'état du jeu.
//!
//! Tout est fait de quelques maillages (un carré, un cube, une sphère, un
//! anneau) et de quelques matériaux partagés (un par couleur), créés une
//! seule fois dans [`EffectAssets`] : jamais un matériau par unité. Les
//! effets « s'éteignent » en changeant d'échelle, pas de couleur, pour
//! garder ces matériaux communs ; les traits et éclats sont émissifs et
//! profitent du bloom.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::ui::{ATTACK_COLOR, MOVE_COLOR};
use super::{Attacker, CombatEvent, CombatKind, Combatant, OrderMarker};
use crate::buildings::{BuildingCatalog, Health, UnderConstruction};
use crate::camera::{screen_ray, RtsCamera};
use crate::cursor::Pointer;
use crate::players::PlayerId;
use crate::players::LocalPlayer;
use crate::selection::{pick, Pickable, Selectable, Selected};
use crate::vision::display::FogHidden;
use crate::vision::FogOfWar;
use crate::simulation::{MoveTarget, SimPosition};
use crate::terrain::Terrain;
use crate::units::{MaterialSlot, Unit, UnitCatalog};
use crate::RtsSystems;

/// Nombre maximal d'effets brefs (traits, éclats) à l'écran : au-delà, les
/// nouveaux tirs ne sont plus dessinés (300 contre 300 reste lisible et fluide).
const MAX_EFFECTS: usize = 500;

/// Maillages et matériaux partagés par tous les effets.
#[derive(Resource)]
struct EffectAssets {
    quad: Handle<Mesh>,
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
    ring: Handle<Mesh>,
    bar_back: Handle<StandardMaterial>,
    /// Remplissage des barres : vert, jaune, rouge selon les PV restants.
    bar_fill: [Handle<StandardMaterial>; 3],
    /// Traits de tir, un par joueur (couleur d'équipe, lumineuse).
    tracers: Vec<Handle<StandardMaterial>>,
    /// Éclat d'impact et de destruction (blanc chaud).
    flash: Handle<StandardMaterial>,
    marker_attack: Handle<StandardMaterial>,
    marker_move: Handle<StandardMaterial>,
}

impl EffectAssets {
    fn tracer(&self, player: PlayerId) -> Handle<StandardMaterial> {
        self.tracers.get(usize::from(player.0)).unwrap_or(&self.tracers[0]).clone()
    }
}

/// Une barre de vie, qui suit l'entité `of`.
#[derive(Component)]
struct HealthBar {
    of: Entity,
}

/// Le remplissage d'une barre de vie.
#[derive(Component)]
struct HealthFill;

/// Un effet bref : il vit `duration` secondes, en changeant d'échelle.
#[derive(Component)]
struct Flash {
    age: f32,
    duration: f32,
    /// Échelle au début de sa vie, et à la fin.
    from: Vec3,
    to: Vec3,
}

/// Une épave qui s'effondre et s'enfonce dans le sol, puis disparaît.
#[derive(Component)]
struct Wreck {
    age: f32,
    duration: f32,
    /// Bascule finale (radians) autour de l'axe de côté.
    tilt: f32,
    /// Profondeur finale sous le sol.
    depth: f32,
    rest: Transform,
}

pub struct CombatEffectsPlugin;

impl Plugin for CombatEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_assets).add_systems(
            Update,
            (spawn_shot_effects, spawn_deaths, spawn_order_markers, animate_flashes, animate_wrecks, face_targets, update_health_bars)
                .chain()
                .after(RtsSystems::Actions),
        );
    }
}

fn create_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // Matériau qui ne dépend pas de l'éclairage ; `glow` > 1 : il brille (bloom).
    let mut flat = |color: Color, glow: f32| {
        materials.add(StandardMaterial {
            base_color: if glow > 0.0 { Color::BLACK } else { color },
            emissive: color.to_linear() * glow,
            unlit: glow <= 0.0,
            fog_enabled: false,
            ..default()
        })
    };
    let assets = EffectAssets {
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap_or_else(|_| Sphere::new(1.0).mesh().build())),
        ring: meshes.add(Annulus::new(0.75, 1.0)),
        bar_back: flat(Color::srgba(0.02, 0.03, 0.04, 1.0), 0.0),
        bar_fill: [
            flat(Color::srgb(0.25, 0.95, 0.35), 0.0),
            flat(Color::srgb(0.98, 0.8, 0.2), 0.0),
            flat(Color::srgb(1.0, 0.25, 0.18), 0.0),
        ],
        // Remplacés par les couleurs d'équipe dès que le catalogue existe.
        tracers: vec![flat(Color::srgb(1.0, 0.6, 0.25), 6.0)],
        flash: flat(Color::srgb(1.0, 0.7, 0.4), 3.5),
        marker_attack: flat(ATTACK_COLOR, 0.0),
        marker_move: flat(MOVE_COLOR, 0.0),
    };
    commands.insert_resource(assets);
}

/// Un trait de tir (de la couleur d'équipe du tireur) et un petit éclat à
/// l'impact ; une arme de zone fait un éclat plus gros, une arme de contact
/// seulement l'éclat.
#[allow(clippy::too_many_arguments)]
fn spawn_shot_effects(
    mut commands: Commands,
    mut events: MessageReader<CombatEvent>,
    mut assets: ResMut<EffectAssets>,
    catalog: Option<Res<UnitCatalog>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    transforms: Query<&GlobalTransform>,
    flashes: Query<(), With<Flash>>,
    mut ready: Local<bool>,
    (fog, local): (Option<Res<FogOfWar>>, Res<LocalPlayer>),
) {
    // Une fois : un matériau de trait par couleur d'équipe.
    if !*ready && let Some(catalog) = &catalog {
        let colors: Vec<Color> = (0..8u8)
            .map(|p| materials.get(&catalog.materials_for(0, PlayerId(p)).get(MaterialSlot::Team)).map_or(Color::WHITE, |m| m.base_color))
            .collect();
        assets.tracers = colors
            .into_iter()
            .map(|c| {
                // Un peu éclaircie : un trait trop saturé se lit mal sur le sol sombre.
                let bright = c.mix(&Color::WHITE, 0.25);
                materials.add(StandardMaterial {
                    base_color: Color::BLACK,
                    emissive: bright.to_linear() * 7.0,
                    fog_enabled: false,
                    ..default()
                })
            })
            .collect();
        *ready = true;
    }
    let mut budget = MAX_EFFECTS.saturating_sub(flashes.iter().count());
    for event in events.read() {
        let CombatEvent::Shot { attacker, target, from, to, player, splash, melee } = *event else { continue };
        if budget < 2 {
            continue;
        }
        // Un tir entièrement dans le brouillard ne se voit pas.
        let seen = |p: Vec2| fog.as_ref().is_none_or(|f| f.is_visible(local.0, p));
        if player != local.0 && !seen(from) && !seen(to) {
            continue;
        }
        // Hauteurs : le milieu des modèles (l'origine de leur racine), ou 0,8 m.
        let start = transforms.get(attacker).map_or(from.extend(0.8).xzy(), |t| t.translation().with_y(t.translation().y.max(0.5)));
        let end = transforms.get(target).map_or(to.extend(0.8).xzy(), |t| t.translation());
        let impact_size = if splash > 0.0 { splash * 0.3 } else { 0.14 };
        commands.spawn((
            Flash { age: 0.0, duration: if splash > 0.0 { 0.35 } else { 0.14 }, from: Vec3::splat(impact_size * 0.4), to: Vec3::splat(impact_size) },
            Mesh3d(assets.sphere.clone()),
            MeshMaterial3d(assets.flash.clone()),
            Transform::from_translation(end).with_scale(Vec3::splat(impact_size * 0.4)),
            NotShadowCaster,
        ));
        budget -= 1;
        if melee {
            continue;
        }
        let length = start.distance(end);
        if length < 0.05 {
            continue;
        }
        // Un cube étiré entre les deux points ; il s'amincit en disparaissant.
        let width = if splash > 0.0 { 0.16 } else { 0.08 };
        let transform = Transform::from_translation((start + end) / 2.0).looking_at(end, Vec3::Y);
        commands.spawn((
            Flash { age: 0.0, duration: 0.13, from: Vec3::new(width, width, length), to: Vec3::new(0.0, 0.0, length) },
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.tracer(player)),
            transform.with_scale(Vec3::new(width, width, length)),
            NotShadowCaster,
        ));
        budget -= 1;
    }
}

/// Mort d'une unité : son épave bascule et s'enfonce, avec un éclat. Un
/// bâtiment détruit s'effondre plus lentement, avec un gros éclat.
fn spawn_deaths(
    mut commands: Commands,
    mut events: MessageReader<CombatEvent>,
    assets: Res<EffectAssets>,
    units: Option<Res<UnitCatalog>>,
    buildings: Option<Res<BuildingCatalog>>,
    (fog, local, terrain): (Option<Res<FogOfWar>>, Res<LocalPlayer>, Res<Terrain>),
) {
    for event in events.read() {
        let CombatEvent::Died { position, owner, kind, yaw, .. } = *event else { continue };
        // Une mort dans le brouillard ne se voit pas (un bâtiment détruit
        // hors de vue reste affiché comme souvenir, voir `vision::display`).
        if owner != local.0 && fog.as_ref().is_some_and(|f| !f.is_visible(local.0, position)) {
            continue;
        }
        let (wreck, size, duration, tilt, depth) = match kind {
            CombatKind::Unit(type_index) => {
                let top = units.as_ref().map_or(1.5, |u| u.model_top(type_index));
                let wreck = units.as_ref().and_then(|u| u.spawn_wreck(&mut commands, owner, type_index, position, yaw));
                (wreck, (top * 0.6).clamp(0.6, 2.5), 1.3, 1.25, top * 1.1)
            }
            CombatKind::Building(type_index) => {
                let top = buildings.as_ref().map_or(3.0, |b| b.model_top(type_index));
                let wreck = buildings.as_ref().and_then(|b| b.spawn_wreck(&mut commands, owner, type_index, position));
                (wreck, (top * 0.7).clamp(1.5, 4.5), 2.6, 0.12, top * 1.05)
            }
        };
        if let Some(wreck) = wreck {
            // Posée sur le relief (elle s'y enfonce ensuite).
            let ground = terrain.height_at(position);
            let rest = Transform::from_xyz(position.x, ground, position.y).with_rotation(Quat::from_rotation_y(yaw));
            let rest = if matches!(kind, CombatKind::Building(_)) { Transform::from_xyz(position.x, ground, position.y) } else { rest };
            commands.entity(wreck).insert(Wreck { age: 0.0, duration, tilt, depth, rest });
        }
        commands.spawn((
            Flash { age: 0.0, duration: 0.45, from: Vec3::splat(size * 0.3), to: Vec3::splat(size) },
            Mesh3d(assets.sphere.clone()),
            MeshMaterial3d(assets.flash.clone()),
            Transform::from_xyz(position.x, terrain.height_at(position) + size * 0.5, position.y).with_scale(Vec3::splat(size * 0.3)),
            NotShadowCaster,
        ));
    }
}

/// Repère au sol d'un ordre du joueur : un anneau rouge (attaque) ou vert
/// (déplacement) qui se resserre.
fn spawn_order_markers(mut commands: Commands, mut markers: MessageReader<OrderMarker>, assets: Res<EffectAssets>, terrain: Res<Terrain>) {
    for marker in markers.read() {
        let material = if marker.attack { assets.marker_attack.clone() } else { assets.marker_move.clone() };
        commands.spawn((
            Flash { age: 0.0, duration: 0.6, from: Vec3::splat(1.3), to: Vec3::splat(0.25) },
            Mesh3d(assets.ring.clone()),
            MeshMaterial3d(material),
            Transform::from_xyz(marker.point.x, terrain.height_at(marker.point) + 0.06, marker.point.y)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(1.3)),
            NotShadowCaster,
        ));
    }
}

fn animate_flashes(mut commands: Commands, time: Res<Time>, mut flashes: Query<(Entity, &mut Flash, &mut Transform)>) {
    for (entity, mut flash, mut transform) in &mut flashes {
        flash.age += time.delta_secs();
        let t = flash.age / flash.duration;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.scale = flash.from.lerp(flash.to, t);
    }
}

fn animate_wrecks(mut commands: Commands, time: Res<Time>, mut wrecks: Query<(Entity, &mut Wreck, &mut Transform)>) {
    for (entity, mut wreck, mut transform) in &mut wrecks {
        wreck.age += time.delta_secs();
        let t = wreck.age / wreck.duration;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        // Bascule rapide au début (elle tombe), puis enfoncement régulier.
        let fall = (t * 3.0).min(1.0);
        let fall = 1.0 - (1.0 - fall) * (1.0 - fall);
        let sink = (t - 0.3).max(0.0) / 0.7;
        transform.rotation = wreck.rest.rotation * Quat::from_rotation_x(wreck.tilt * fall);
        transform.translation = wreck.rest.translation - Vec3::Y * wreck.depth * sink;
    }
}

/// Une unité immobile qui tire se tourne vers sa cible (la simulation, elle,
/// ne connaît pas d'orientation).
fn face_targets(
    time: Res<Time>,
    mut units: Query<(&Attacker, &mut Transform), (With<Unit>, Without<MoveTarget>)>,
    positions: Query<(Option<&SimPosition>, &GlobalTransform)>,
) {
    for (attacker, mut transform) in &mut units {
        let Some(target) = attacker.target else { continue };
        let Ok((sim, global)) = positions.get(target) else { continue };
        let goal = sim.map_or(global.translation().xz(), |p| p.current);
        let dir = goal - transform.translation.xz();
        if dir.length_squared() < 1e-4 {
            continue;
        }
        // Le « devant » d'une entité Bevy est −Z.
        let wanted = Quat::from_rotation_y(f32::atan2(-dir.x, -dir.y));
        transform.rotation = transform.rotation.slerp(wanted, (time.delta_secs() * 10.0).min(1.0));
    }
}

/// Barres de vie : au-dessus des entités sélectionnées, blessées ou
/// survolées. Ce sont des rectangles tournés vers la caméra, agrandis quand
/// on dézoome pour rester lisibles (pilier n° 4).
#[allow(clippy::too_many_arguments)]
fn update_health_bars(
    mut commands: Commands,
    assets: Res<EffectAssets>,
    camera: Single<(&Camera, &GlobalTransform, &RtsCamera)>,
    pointer: Res<Pointer>,
    units: Option<Res<UnitCatalog>>,
    buildings: Option<Res<BuildingCatalog>>,
    // Pas de barre pour ce que cache le brouillard.
    fighters: Query<(Entity, &Health, &Combatant, &GlobalTransform, Has<Selected>, Option<&Selectable>, Has<UnderConstruction>), Without<FogHidden>>,
    pickable: Pickable,
    mut bars: Query<(Entity, &HealthBar, &mut Transform, &Children), Without<HealthFill>>,
    mut fills: Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>), With<HealthFill>>,
) {
    let (camera, camera_transform, rts) = camera.into_inner();
    let hovered = pointer.position.and_then(|p| screen_ray(camera, camera_transform, p)).and_then(|ray| pick(ray, &pickable));
    // Au-delà de 25 m de recul, la barre grandit, moins vite que la
    // distance (racine carrée) : lisible de loin sans recouvrir les armées.
    let zoom = (rts.distance / 25.0).max(1.0).sqrt();
    let facing = camera_transform.rotation();

    // Un chantier a moins que ses PV maximaux sans être blessé : il a déjà
    // sa barre de progression.
    type Item<'a> = (Entity, &'a Health, &'a Combatant, &'a GlobalTransform, bool, Option<&'a Selectable>, bool);
    let shown = |(entity, health, _, _, selected, _, site): &Item| {
        *selected || (health.current < health.max && !*site) || hovered == Some(*entity)
    };
    // Les barres dont l'entité a disparu ou n'est plus à montrer.
    for (bar_entity, bar, ..) in &bars {
        if !fighters.get(bar.of).is_ok_and(|f| shown(&f)) {
            commands.entity(bar_entity).despawn();
        }
    }
    // Barre existante de chaque entité, triée pour la retrouver vite
    // (recherche par dichotomie plutôt qu'un parcours de toutes les barres).
    let mut existing: Vec<(Entity, Entity)> = bars.iter().map(|(bar, b, ..)| (b.of, bar)).collect();
    existing.sort_unstable();
    for fighter in &fighters {
        if !shown(&fighter) {
            continue;
        }
        let (entity, health, combatant, global, _, selectable, _) = fighter;
        let (top, width) = match combatant.kind {
            CombatKind::Unit(t) => {
                let radius = selectable.map_or(0.6, |s| s.radius);
                (units.as_ref().map_or(1.5, |u| u.model_top(t)), (radius * 1.6).clamp(0.8, 2.6))
            }
            CombatKind::Building(t) => {
                let half = selectable.and_then(|s| s.half_extents).map_or(2.0, |h| h.x);
                (buildings.as_ref().map_or(3.0, |b| b.model_top(t)), (half * 1.2).clamp(2.0, 5.0))
            }
        };
        let fraction = (health.current / health.max.max(1e-3)).clamp(0.0, 1.0);
        let width = width * zoom;
        let height = 0.16 * zoom;
        let at = global.translation().xz();
        let translation = Vec3::new(at.x, top + 0.35 + height, at.y);
        let scale = Vec3::new(width, height, 1.0);
        let material = assets.bar_fill[if fraction > 0.6 { 0 } else if fraction > 0.3 { 1 } else { 2 }].clone();
        // Le remplissage, aligné à gauche, un peu devant le fond.
        let fill_transform = Transform::from_xyz(-(1.0 - fraction) / 2.0, 0.0, 0.01).with_scale(Vec3::new(fraction.max(0.001), 0.62, 1.0));

        let found = existing.binary_search_by_key(&entity, |e| e.0).ok().and_then(|i| bars.get_mut(existing[i].1).ok());
        match found {
            Some((_, _, mut transform, children)) => {
                transform.translation = translation;
                transform.rotation = facing;
                transform.scale = scale;
                for child in children.iter() {
                    if let Ok((mut fill, mut fill_material)) = fills.get_mut(child) {
                        *fill = fill_transform;
                        if fill_material.0 != material {
                            fill_material.0 = material.clone();
                        }
                    }
                }
            }
            None => {
                commands
                    .spawn((
                        HealthBar { of: entity },
                        Mesh3d(assets.quad.clone()),
                        MeshMaterial3d(assets.bar_back.clone()),
                        Transform::from_translation(translation).with_rotation(facing).with_scale(scale),
                        NotShadowCaster,
                    ))
                    .with_children(|bar| {
                        bar.spawn((HealthFill, Mesh3d(assets.quad.clone()), MeshMaterial3d(material), fill_transform, NotShadowCaster));
                    });
            }
        }
    }
}
