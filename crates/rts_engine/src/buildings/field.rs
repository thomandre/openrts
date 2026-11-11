//! Champs de construction : certains bâtiments émettent autour d'eux un
//! champ circulaire (rayon en données, [`super::BuildingType::field_radius`]),
//! et d'autres ne peuvent se poser qu'à l'intérieur d'un champ du même joueur
//! ([`super::BuildingType::needs_field`]).
//!
//! C'est une règle générique du moteur : le Collectif s'en sert pour son
//! « champ de résonance » (émis par son cœur de base et par ses relais),
//! mais n'importe quelle faction pourrait la reprendre avec d'autres données.
//!
//! Seul un bâtiment **terminé** émet son champ. La question « ce point est-il
//! dans un champ actif du joueur ? » ([`ConstructionFields::covers`],
//! [`in_active_field`]) sert à la pose ; elle servira aussi, une fois le
//! combat en place, à désactiver les bâtiments restés hors champ quand leur
//! relais est détruit (pas encore appliqué : rien ne peut détruire un relais
//! pour l'instant).
//!
//! L'affichage ([`show_construction_fields`]) dessine au sol, pendant une
//! pose (ou tant qu'un fantôme de démonstration est en place), un disque
//! très léger et un cercle fin autour de chaque émetteur du joueur local.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::placement::Placement;
use super::{Building, BuildingCatalog, Footprint, PlacementGhost, UnderConstruction};
use crate::navigation::{GridFrame, NavGrid};
use crate::players::{LocalPlayer, Owner, PlayerId};

/// Les champs actifs d'un joueur : un cercle (centre, rayon) par bâtiment
/// émetteur terminé. On le recalcule quand on en a besoin : il y a peu de
/// bâtiments, et c'est toujours à jour.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConstructionFields {
    pub circles: Vec<(Vec2, f32)>,
}

impl ConstructionFields {
    /// Les champs du joueur `player`. `buildings` donne, pour chaque
    /// bâtiment, son type, son propriétaire, son empreinte et s'il est encore
    /// en chantier (un chantier n'émet rien).
    pub fn of_player<'a>(
        catalog: &BuildingCatalog,
        frame: GridFrame,
        player: PlayerId,
        buildings: impl IntoIterator<Item = (&'a Building, &'a Owner, &'a Footprint, bool)>,
    ) -> Self {
        let circles = buildings
            .into_iter()
            .filter(|(_, owner, _, under_construction)| owner.0 == player && !under_construction)
            .filter_map(|(building, _, footprint, _)| {
                let radius = catalog.types.get(building.type_index)?.field_radius;
                (radius > 0.0).then(|| (frame.rect(footprint.min_cell, footprint.size).center(), radius))
            })
            .collect();
        Self { circles }
    }

    /// Le point `point` (au sol) est-il dans au moins un de ces champs ?
    pub fn covers(&self, point: Vec2) -> bool {
        self.circles.iter().any(|(center, radius)| center.distance_squared(point) <= radius * radius)
    }
}

/// Raccourci : le point `point` est-il dans un champ actif du joueur `player` ?
///
/// C'est la requête qu'utilisera le combat pour éteindre les bâtiments d'un
/// joueur qui ne sont plus dans aucun champ (relais détruit).
pub fn in_active_field<'a>(
    catalog: &BuildingCatalog,
    frame: GridFrame,
    player: PlayerId,
    point: Vec2,
    buildings: impl IntoIterator<Item = (&'a Building, &'a Owner, &'a Footprint, bool)>,
) -> bool {
    ConstructionFields::of_player(catalog, frame, player, buildings).covers(point)
}

/// Les bâtiments vus par les systèmes qui ont besoin des champs. Un `type`
/// donne un nom court à un type long (ici, une requête de Bevy).
pub(crate) type FieldBuildings<'w, 's> =
    Query<'w, 's, (&'static Building, &'static Owner, &'static Footprint, Has<UnderConstruction>)>;

/// Le dessin au sol d'un champ, rattaché au bâtiment qui l'émet.
#[derive(Component, Debug)]
pub(crate) struct FieldMarker {
    building: Entity,
}

/// Affiche les champs du joueur local au sol pendant une pose, ou tant qu'un
/// fantôme est en place (scène de démonstration) ; les retire sinon.
/// Affichage seulement : rien ici ne change l'état du jeu.
pub(crate) fn show_construction_fields(
    mut commands: Commands,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    placement: Res<Placement>,
    local: Res<LocalPlayer>,
    ghosts: Query<(), With<PlacementGhost>>,
    buildings: Query<(Entity, &Building, &Owner, &Footprint), Without<UnderConstruction>>,
    markers: Query<(Entity, &FieldMarker)>,
) {
    let (Some(catalog), Some(grid)) = (catalog, grid) else { return };
    let Some(visuals) = catalog.field_visuals.as_ref() else { return };
    let showing = placement.active.is_some() || !ghosts.is_empty();
    // Les émetteurs terminés du joueur local.
    let emitters: Vec<(Entity, usize, Vec2)> = if showing {
        buildings
            .iter()
            .filter(|(_, building, owner, _)| owner.0 == local.0 && catalog.types[building.type_index].field_radius > 0.0)
            .map(|(entity, building, _, footprint)| {
                (entity, building.type_index, grid.frame().rect(footprint.min_cell, footprint.size).center())
            })
            .collect()
    } else {
        Vec::new()
    };
    for (entity, marker) in &markers {
        if !emitters.iter().any(|e| e.0 == marker.building) {
            commands.entity(entity).despawn();
        }
    }
    for (building, type_index, center) in emitters {
        if markers.iter().any(|(_, m)| m.building == building) {
            continue;
        }
        let Some((disc, ring)) = visuals.meshes[type_index].clone() else { continue };
        commands
            .spawn((
                Name::new("Champ de construction"),
                FieldMarker { building },
                Transform::from_xyz(center.x, 0.0, center.y),
                Visibility::default(),
            ))
            .with_children(|parent| {
                // Les formes 2D de Bevy sont dans le plan (x, y) : un quart de
                // tour autour de x les couche sur le sol.
                let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                parent.spawn((
                    Mesh3d(disc),
                    MeshMaterial3d(visuals.disc_material.clone()),
                    Transform::from_xyz(0.0, 0.02, 0.0).with_rotation(flat),
                    NotShadowCaster,
                ));
                parent.spawn((
                    Mesh3d(ring),
                    MeshMaterial3d(visuals.ring_material.clone()),
                    Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(flat),
                    NotShadowCaster,
                ));
            });
    }
}

/// Maillages et matériaux des champs, préparés une fois (voir
/// [`BuildingCatalog::with_construction_field`]).
pub(crate) struct FieldVisuals {
    /// Pour chaque type : (disque, cercle) à son rayon, s'il émet un champ.
    pub(crate) meshes: Vec<Option<(Handle<Mesh>, Handle<Mesh>)>>,
    pub(crate) disc_material: Handle<StandardMaterial>,
    pub(crate) ring_material: Handle<StandardMaterial>,
}

impl FieldVisuals {
    pub(crate) fn new(
        radii: impl Iterator<Item = f32>,
        color: Color,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let meshes = radii
            .map(|r| {
                (r > 0.0).then(|| {
                    // Cercle fin : 14 cm, assez pour se voir en dézoom sans peser.
                    let disc = meshes.add(Circle::new(r).mesh().resolution(64).build());
                    let ring = meshes.add(Annulus::new(r - 0.14, r).mesh().resolution(64).build());
                    (disc, ring)
                })
            })
            .collect();
        // Translucides, sans éclairage ni brouillard : un repère d'interface posé au sol.
        let flat = |alpha: f32, glow: f32| StandardMaterial {
            base_color: color.with_alpha(alpha),
            emissive: color.to_linear() * glow,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            fog_enabled: false,
            ..default()
        };
        Self { meshes, disc_material: materials.add(flat(0.07, 0.0)), ring_material: materials.add(flat(0.55, 0.8)) }
    }
}
