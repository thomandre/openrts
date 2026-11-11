//! Unités : description en données et création dans le monde.
//!
//! Le moteur ne sait pas quelles unités existent : la crate jeu lui fournit
//! une liste de [`UnitType`] (des données), et le moteur sait les faire
//! apparaître.
//!
//! L'apparence d'une unité est elle aussi une donnée : une liste de [`Part`]
//! (une forme simple, sa place dans l'unité et un emplacement de matériau),
//! qu'on écrit facilement avec un [`ModelBuilder`]. Les couleurs ne sont pas
//! dans les pièces mais dans une [`Palette`] fournie par la faction : la même
//! description d'unité peut ainsi changer de faction ou de couleur d'équipe.
//!
//! Certaines pièces sont animées (voir [`crate::graphics::animation`]) :
//! un groupe qui bouge ([`ModelBuilder::animated`], tourelle, flammes) reste
//! un maillage séparé ; une pièce lumineuse qui pulse
//! ([`ModelBuilder::pulsing`]) utilise un matériau partagé dont l'intensité
//! varie.

use std::collections::HashMap;

use bevy::light::NotShadowCaster;
use bevy::math::Affine2;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::Face;

use crate::collision::{Collider, CollisionLayer};
use crate::graphics::animation::{AnimatedPart, BodyMotion, PulsingPiece};
use crate::graphics::shapes::merge_parts;
use crate::graphics::textures::detail_images;
use crate::graphics::Hover;
use crate::players::{Owner, PlayerId};
use crate::selection::{Selectable, SelectionRing};
use crate::simulation::{MoveSpeed, SimPosition};

pub use crate::graphics::animation::{Motion, Pulse};
pub use crate::graphics::shapes::PartShape;
pub use crate::graphics::textures::SurfaceDetail;

/// Rôle d'une pièce, qui décide de son matériau dans la [`Palette`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialSlot {
    /// Peinture principale de la faction.
    Paint,
    /// Métal sombre : structure, articulations.
    Dark,
    /// Acier : blindage métallique nu.
    Steel,
    /// Métal usiné brillant : canons, vérins.
    Bare,
    /// Caoutchouc, chenilles.
    Rubber,
    /// Couleur d'équipe du joueur : doit rester bien visible de loin.
    Team,
    /// Émissif froid : visières, capteurs.
    Glow,
    /// Émissif chaud : réacteurs, outils brûlants.
    Engine,
    /// Bande de danger : chevrons diagonaux aux couleurs de l'équipe et noirs
    /// (dérivé de `team` dans la palette, avec le motif [`SurfaceDetail::Chevrons`]).
    Hazard,
    /// Flamme : lumière additive et semi-transparente (elle s'ajoute à ce qui
    /// est derrière elle), de la couleur de `engine`. Ne projette pas d'ombre.
    Flame,
    /// Pierre taillée (dérivée de `glow`) : contrairement à `Glow`, elle est
    /// aussi **éclairée** (couleur, reflets nets sur ses facettes) et ne brille
    /// que faiblement de l'intérieur. Ses facettes restent donc visibles.
    Jewel,
    /// Diamant (dérivé de `bare`) : métal miroir de la couleur de `bare`, avec
    /// une légère lueur propre pour que le bloom le fasse scintiller.
    Brilliant,
}

impl MaterialSlot {
    pub const ALL: [MaterialSlot; 12] = [
        MaterialSlot::Paint,
        MaterialSlot::Dark,
        MaterialSlot::Steel,
        MaterialSlot::Bare,
        MaterialSlot::Rubber,
        MaterialSlot::Team,
        MaterialSlot::Glow,
        MaterialSlot::Engine,
        MaterialSlot::Hazard,
        MaterialSlot::Flame,
        MaterialSlot::Jewel,
        MaterialSlot::Brilliant,
    ];

    /// Les pièces lumineuses sont petites ou transparentes : inutile de
    /// calculer leur ombre.
    fn casts_shadow(self) -> bool {
        !matches!(
            self,
            MaterialSlot::Glow | MaterialSlot::Engine | MaterialSlot::Flame | MaterialSlot::Jewel | MaterialSlot::Brilliant
        )
    }
}

/// Aspect d'une surface, en termes **PBR** (« physically based rendering ») :
/// au lieu de régler à la main reflets et ombrages, on décrit la matière
/// (est-ce du métal ? est-elle polie ou rugueuse ?) et le moteur calcule
/// comment elle réagit à la lumière.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceStyle {
    pub color: Color,
    /// 0 : peinture, plastique… ; 1 : métal nu (il reflète son environnement).
    pub metallic: f32,
    /// 0 : poli miroir ; 1 : complètement mat.
    pub roughness: f32,
    /// Motif procédural (panneaux, rivets…) appliqué à la surface.
    pub detail: SurfaceDetail,
    /// Taille d'une répétition du motif, en mètres.
    pub detail_size: f32,
    /// Au-delà de 0, la surface émet sa propre lumière (× sa couleur).
    pub emissive: f32,
}

impl SurfaceStyle {
    pub fn new(color: Color, metallic: f32, roughness: f32, detail: SurfaceDetail) -> Self {
        Self { color, metallic, roughness, detail, detail_size: 1.0, emissive: 0.0 }
    }

    /// Une surface qui brille : visière, réacteur.
    pub fn glowing(color: Color, emissive: f32) -> Self {
        Self { emissive, ..Self::new(color, 0.0, 0.4, SurfaceDetail::Smooth) }
    }
}

/// Les matériaux d'une faction, un par [`MaterialSlot`] (les emplacements
/// `Hazard` et `Flame` sont déduits de `team` et `engine`).
#[derive(Debug, Clone)]
pub struct Palette {
    pub paint: SurfaceStyle,
    pub dark: SurfaceStyle,
    pub steel: SurfaceStyle,
    pub bare: SurfaceStyle,
    pub rubber: SurfaceStyle,
    /// La couleur de `team` est celle du joueur.
    pub team: SurfaceStyle,
    pub glow: SurfaceStyle,
    pub engine: SurfaceStyle,
    /// Couleur de l'anneau de sélection (valeurs > 1 : il brille en HDR).
    pub selection: LinearRgba,
}

impl Palette {
    /// Style de référence d'un emplacement. Pour `Hazard` et `Flame`, c'est
    /// le style dont ils dérivent ; [`Palette::surface`] donne le style final.
    pub fn style(&self, slot: MaterialSlot) -> &SurfaceStyle {
        match slot {
            MaterialSlot::Paint => &self.paint,
            MaterialSlot::Dark => &self.dark,
            MaterialSlot::Steel => &self.steel,
            MaterialSlot::Bare | MaterialSlot::Brilliant => &self.bare,
            MaterialSlot::Rubber => &self.rubber,
            MaterialSlot::Team | MaterialSlot::Hazard => &self.team,
            MaterialSlot::Glow | MaterialSlot::Jewel => &self.glow,
            MaterialSlot::Engine | MaterialSlot::Flame => &self.engine,
        }
    }

    /// Style final d'un emplacement, emplacements dérivés compris.
    pub fn surface(&self, slot: MaterialSlot) -> SurfaceStyle {
        let style = *self.style(slot);
        match slot {
            // Chevrons d'une vingtaine de centimètres, peinture un peu mate.
            MaterialSlot::Hazard => SurfaceStyle {
                detail: SurfaceDetail::Chevrons,
                detail_size: 0.8,
                metallic: 0.4,
                roughness: 0.55,
                emissive: 0.0,
                ..style
            },
            // Pierre polie, à demi métallique pour que ses facettes reflètent
            // nettement la lumière, et une lueur intérieure au quart de `glow`.
            MaterialSlot::Jewel => SurfaceStyle {
                detail: SurfaceDetail::Smooth,
                metallic: 0.55,
                roughness: 0.04,
                emissive: style.emissive * 0.25,
                ..style
            },
            // Miroir presque parfait, à peine lumineux.
            MaterialSlot::Brilliant => SurfaceStyle {
                detail: SurfaceDetail::Smooth,
                metallic: 1.0,
                roughness: 0.03,
                emissive: 0.35,
                ..style
            },
            _ => style,
        }
    }
}

/// Un groupe de pièces animé : son pivot (placement dans l'unité, autour
/// duquel il bouge) et son mouvement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimatedGroup {
    pub pivot: Transform,
    pub motion: Motion,
}

/// Une pièce d'unité : forme, matériau et placement par rapport au sol sous
/// l'unité (y = 0 : le sol ; −z : l'avant de l'unité).
#[derive(Debug, Clone)]
pub struct Part {
    pub shape: PartShape,
    pub slot: MaterialSlot,
    /// Placement de la pièce. Pour une pièce animée (`motion`), il est
    /// relatif au pivot du groupe animé.
    pub transform: Transform,
    /// Groupe animé auquel appartient la pièce (`None` : pièce fixe).
    pub motion: Option<AnimatedGroup>,
    /// Pulsation de l'émission (pièces lumineuses seulement).
    pub pulse: Option<Pulse>,
}

/// Aide à écrire la liste des pièces d'une unité.
///
/// Les pièces peuvent être regroupées ([`ModelBuilder::group`]) : un groupe a
/// son propre placement (une arme tenue de biais, un buste penché en avant),
/// appliqué à toutes ses pièces. [`ModelBuilder::mirrored`] dessine les pièces
/// symétriques (jambes, épaulières) une seule fois.
#[derive(Debug, Clone)]
pub struct ModelBuilder {
    parts: Vec<Part>,
    /// Placement du groupe en cours.
    group: Transform,
    /// Groupe animé en cours, s'il y en a un.
    motion: Option<AnimatedGroup>,
    /// Pulsation en cours, s'il y en a une.
    pulse: Option<Pulse>,
}

impl Default for ModelBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelBuilder {
    pub fn new() -> Self {
        Self { parts: Vec::new(), group: Transform::IDENTITY, motion: None, pulse: None }
    }

    /// Ajoute une pièce placée par `transform` (dans le groupe en cours).
    pub fn add(&mut self, slot: MaterialSlot, shape: PartShape, transform: Transform) -> &mut Self {
        self.parts.push(Part {
            shape,
            slot,
            transform: self.group * transform,
            motion: self.motion,
            pulse: self.pulse,
        });
        self
    }

    /// Plaque biseautée de dimensions `size`, centrée en `at`.
    pub fn plate(&mut self, slot: MaterialSlot, size: Vec3, at: Vec3) -> &mut Self {
        self.plate_rotated(slot, size, at, Vec3::ZERO)
    }

    /// Plaque tournée : `rotation` en radians autour de x, puis y, puis z.
    pub fn plate_rotated(&mut self, slot: MaterialSlot, size: Vec3, at: Vec3, rotation: Vec3) -> &mut Self {
        let bevel = size.min_element() * 0.18;
        self.add(slot, PartShape::Plate { size, bevel }, placed(at, rotation))
    }

    /// Plaque au biseau choisi (`bevel` en mètres), tournée de `rotation`.
    pub fn plate_beveled(&mut self, slot: MaterialSlot, size: Vec3, bevel: f32, at: Vec3, rotation: Vec3) -> &mut Self {
        self.add(slot, PartShape::Plate { size, bevel }, placed(at, rotation))
    }

    /// Cylindre d'axe y (tourné par `rotation`, par exemple ±π/2 en x pour un canon).
    pub fn cylinder(&mut self, slot: MaterialSlot, radius: f32, height: f32, at: Vec3, rotation: Vec3) -> &mut Self {
        self.add(slot, PartShape::Cylinder { radius, height }, placed(at, rotation))
    }

    /// Cône (tronqué si `radius_top` > 0) d'axe y, sommet vers +y.
    pub fn cone(
        &mut self,
        slot: MaterialSlot,
        radius_bottom: f32,
        radius_top: f32,
        height: f32,
        at: Vec3,
        rotation: Vec3,
    ) -> &mut Self {
        self.add(slot, PartShape::Frustum { radius_bottom, radius_top, height }, placed(at, rotation))
    }

    pub fn sphere(&mut self, slot: MaterialSlot, radius: f32, at: Vec3) -> &mut Self {
        self.add(slot, PartShape::Sphere { radius }, Transform::from_translation(at))
    }

    /// Prisme : `profile` est la silhouette vue de côté, en (z, y), extrudée sur `width` en x.
    pub fn prism(&mut self, slot: MaterialSlot, profile: &[Vec2], width: f32, at: Vec3) -> &mut Self {
        self.add(slot, PartShape::Prism { profile: profile.to_vec(), width }, Transform::from_translation(at))
    }

    /// Prisme aux arêtes latérales biseautées de `bevel` mètres (voir [`PartShape::BeveledPrism`]).
    pub fn beveled_prism(
        &mut self,
        slot: MaterialSlot,
        profile: &[Vec2],
        width: f32,
        bevel: f32,
        at: Vec3,
        rotation: Vec3,
    ) -> &mut Self {
        self.add(slot, PartShape::BeveledPrism { profile: profile.to_vec(), width, bevel }, placed(at, rotation))
    }

    /// Forme de révolution d'axe y : `profile` en (rayon, hauteur), du bas vers le haut.
    pub fn lathe(&mut self, slot: MaterialSlot, profile: &[Vec2], segments: u32, at: Vec3, rotation: Vec3) -> &mut Self {
        self.add(slot, PartShape::Lathe { profile: profile.to_vec(), segments }, placed(at, rotation))
    }

    /// Forme de révolution taillée à `sides` faces planes (voir [`PartShape::Faceted`]) :
    /// gemmes, prismes hexagonaux.
    pub fn faceted(&mut self, slot: MaterialSlot, profile: &[Vec2], sides: u32, at: Vec3, rotation: Vec3) -> &mut Self {
        self.add(slot, PartShape::Faceted { profile: profile.to_vec(), sides }, placed(at, rotation))
    }

    /// Tube qui suit la courbe lisse passant par `points` (durites, câbles).
    pub fn tube(&mut self, slot: MaterialSlot, points: &[Vec3], radius: f32) -> &mut Self {
        self.add(slot, PartShape::Tube { points: points.to_vec(), radius, radius_end: radius }, Transform::IDENTITY)
    }

    /// Tore (anneau) couché à plat d'axe y, tourné par `rotation` ; `arc` en
    /// radians (2π pour un anneau fermé, voir [`PartShape::Torus`]).
    pub fn torus(&mut self, slot: MaterialSlot, major: f32, minor: f32, arc: f32, at: Vec3, rotation: Vec3) -> &mut Self {
        self.add(slot, PartShape::Torus { major, minor, arc }, placed(at, rotation))
    }

    /// Ajoute des pièces dans un sous-groupe placé par `transform`.
    ///
    /// `impl FnOnce(&mut Self)` : n'importe quelle fonction (souvent une
    /// fermeture `|b| { … }`) qui reçoit le constructeur pour y ajouter des pièces.
    pub fn group(&mut self, transform: Transform, build: impl FnOnce(&mut Self)) -> &mut Self {
        let parent = self.group;
        self.group = parent * transform;
        build(self);
        self.group = parent;
        self
    }

    /// Ajoute un groupe **animé** : ses pièces sont placées par rapport à
    /// `pivot` (comme dans [`ModelBuilder::group`]) et bougent ensemble autour
    /// de lui selon `motion`. Le groupe devient un maillage séparé par
    /// matériau : en garder un ou deux par unité, pour les performances.
    ///
    /// Les groupes animés ne s'imbriquent pas : à l'intérieur d'un groupe
    /// animé, un second appel se comporte comme un simple [`ModelBuilder::group`].
    pub fn animated(&mut self, pivot: Transform, motion: Motion, build: impl FnOnce(&mut Self)) -> &mut Self {
        if self.motion.is_some() {
            warn!("Groupes animés imbriqués : le groupe intérieur suit simplement l'extérieur");
            return self.group(pivot, build);
        }
        // On retient l'état courant pour le rétablir après le groupe.
        let (parent, parent_motion) = (self.group, self.motion);
        self.motion = Some(AnimatedGroup { pivot: parent * pivot, motion });
        // Dans le groupe, les placements sont relatifs au pivot.
        self.group = Transform::IDENTITY;
        build(self);
        self.group = parent;
        self.motion = parent_motion;
        self
    }

    /// Les pièces lumineuses ajoutées dans `build` pulsent selon `pulse`
    /// (sans effet sur les autres pièces).
    ///
    /// Toutes les pièces d'un même emplacement de matériau qui pulsent de la
    /// même façon partagent quelques matériaux animés (un par décalage),
    /// quelles que soient les unités : le dessin en lot est préservé.
    pub fn pulsing(&mut self, pulse: Pulse, build: impl FnOnce(&mut Self)) -> &mut Self {
        let parent = self.pulse;
        self.pulse = Some(pulse);
        build(self);
        self.pulse = parent;
        self
    }

    /// Appelle `build` deux fois, avec `side` = 1 (droite, +x) puis −1 (gauche).
    pub fn mirrored(&mut self, build: impl Fn(&mut Self, f32)) -> &mut Self {
        build(self, 1.0);
        build(self, -1.0);
        self
    }

    pub fn build(&self) -> Vec<Part> {
        self.parts.clone()
    }
}

fn placed(at: Vec3, rotation: Vec3) -> Transform {
    Transform::from_translation(at).with_rotation(Quat::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z))
}

/// Flottement d'une unité volante (voir [`Hover`]).
#[derive(Debug, Clone, Copy)]
pub struct HoverMotion {
    pub amplitude: f32,
    pub frequency: f32,
}

/// Description d'un type d'unité, purement en données.
#[derive(Debug, Clone)]
pub struct UnitType {
    pub name: String,
    /// Les pièces qui forment l'apparence de l'unité.
    pub parts: Vec<Part>,
    /// Rayon du cercle occupé au sol (collisions, sélection), en mètres.
    pub footprint_radius: f32,
    /// Masse relative pour les collisions (1 pour un fantassin) : les unités
    /// légères ne peuvent pas bousculer les plus lourdes.
    pub mass: f32,
    /// Vitesse en mètres par seconde.
    pub speed: f32,
    /// Unités volantes : le corps flotte doucement (la racine, elle, ne bouge pas).
    pub hover: Option<HoverMotion>,
}

impl UnitType {
    /// Rayon de l'empreinte au sol (sert aux collisions, au cercle de sélection et au clic).
    pub fn footprint_radius(&self) -> f32 {
        self.footprint_radius
    }
}

/// Marque une entité comme unité et indique son type (indice dans le catalogue).
#[derive(Component, Debug, Clone, Copy)]
pub struct Unit {
    pub type_index: usize,
}

/// Matériau d'un maillage : un emplacement de la palette, et éventuellement
/// sa pulsation.
pub(crate) type MaterialKey = (MaterialSlot, Option<Pulse>);

/// Un groupe animé prêt à dessiner : son mouvement et ses maillages.
pub(crate) struct AnimatedVisual {
    pub(crate) group: AnimatedGroup,
    pub(crate) meshes: Vec<(Handle<Mesh>, MaterialKey)>,
}

/// Ce qu'il faut pour dessiner un modèle (unité ou bâtiment) : un maillage
/// par matériau pour les pièces fixes, et les groupes animés.
pub(crate) struct TypeVisual {
    pub(crate) meshes: Vec<(Handle<Mesh>, MaterialKey)>,
    pub(crate) animated: Vec<AnimatedVisual>,
    /// Hauteur du milieu du modèle : l'origine de la racine y est placée.
    pub(crate) center_height: f32,
    /// Hauteur du point le plus haut du modèle.
    pub(crate) top: f32,
}

/// Hauteur minimale (mètres) du bas du modèle d'une unité volante : elle
/// passe nettement au-dessus des unités au sol.
pub const FLIGHT_FLOOR: f32 = 3.0;

/// Nombre de variantes décalées dans le temps de chaque matériau qui pulse :
/// chaque unité en prend une, pour que toute l'armée ne clignote pas en
/// chœur, sans pour autant créer un matériau par unité.
const PULSE_PHASES: usize = 3;

/// Tous les types d'unités du jeu, avec leurs maillages et matériaux prêts à l'emploi.
///
/// `Handle<T>` est une référence légère vers une ressource chargée (maillage,
/// matériau…) : la copier ne duplique pas la donnée. Toutes les unités d'un
/// même type partagent donc les mêmes maillages, et toutes les unités de la
/// faction les mêmes matériaux. Bevy regroupe alors les objets identiques
/// pour les dessiner en lot (« instanciation ») : 300 unités ne coûtent pas
/// 300 fois plus cher qu'une.
///
/// **Couleurs d'équipe** : la même faction peut être jouée par plusieurs
/// joueurs, chacun avec sa couleur ([`UnitCatalog::set_team_colors`]). Seuls
/// les deux matériaux qui dépendent de la couleur d'équipe (`Team` et
/// `Hazard`) sont recréés pour chaque joueur : quelques matériaux par
/// joueur, jamais un par unité.
///
/// **Plusieurs factions** : le catalogue est commun à toute la partie. Chaque
/// faction y ajoute ses types à la suite des autres
/// ([`UnitCatalog::add_faction`]), avec sa propre palette : un type d'unité
/// est donc un indice unique dans toute la partie, et la faction d'un joueur
/// (voir [`crate::players::PlayerState::faction`]) dit lesquels sont à lui.
/// La faction 0 est celle passée à [`UnitCatalog::new`].
#[derive(Resource)]
pub struct UnitCatalog {
    pub types: Vec<UnitType>,
    /// La faction de chaque type (indice dans `factions`).
    type_faction: Vec<usize>,
    /// Palette et matériaux de chaque faction.
    factions: Vec<FactionLook>,
    /// Couleur d'équipe de chaque joueur (voir [`UnitCatalog::set_team_colors`]).
    team_colors: Vec<Color>,
    visuals: Vec<TypeVisual>,
    /// Pour chaque type : de combien son modèle est surélevé (volants).
    altitudes: Vec<f32>,
    /// Textures de détail déjà créées (partagées par les matériaux qui les utilisent).
    detail_textures: HashMap<SurfaceDetail, Option<(Handle<Image>, Handle<Image>)>>,
    ring_mesh: Handle<Mesh>,
    ring_material: Handle<StandardMaterial>,
    /// Anneau rouge sous une unité ou un bâtiment adverse sélectionné.
    enemy_ring_material: Handle<StandardMaterial>,
}

/// La palette d'une faction et ses matériaux pour chaque joueur (indice :
/// numéro du joueur). Le premier est celui de la palette ; un joueur au-delà
/// de la liste prend le premier.
struct FactionLook {
    palette: Palette,
    team_materials: Vec<ModelMaterials>,
}

impl UnitCatalog {
    /// Prépare les maillages de chaque type et les matériaux de la palette :
    /// c'est la faction 0.
    pub fn new(
        types: Vec<UnitType>,
        palette: &Palette,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) -> Self {
        let mut catalog = Self {
            types: Vec::new(),
            type_faction: Vec::new(),
            factions: Vec::new(),
            team_colors: Vec::new(),
            visuals: Vec::new(),
            altitudes: Vec::new(),
            detail_textures: HashMap::new(),
            // Anneau de rayon 1, mis à l'échelle de chaque unité. Assez épais
            // pour rester lisible en dézoom.
            ring_mesh: meshes.add(Annulus::new(0.8, 1.0)),
            ring_material: materials.add(StandardMaterial {
                base_color: palette.selection.into(),
                unlit: true,
                fog_enabled: false,
                ..default()
            }),
            enemy_ring_material: materials.add(StandardMaterial {
                base_color: LinearRgba::rgb(2.4, 0.25, 0.18).into(),
                unlit: true,
                fog_enabled: false,
                ..default()
            }),
        };
        catalog.add_faction(types, palette, meshes, materials, images);
        catalog
    }

    /// Ajoute une faction : ses types, à la suite de ceux déjà là, et sa
    /// palette. Renvoie son numéro de faction ; ses types commencent à
    /// l'indice [`UnitCatalog::first_type`] de ce numéro.
    pub fn add_faction(
        &mut self,
        types: Vec<UnitType>,
        palette: &Palette,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) -> usize {
        let faction = self.factions.len();
        // Les textures de détail sont partagées par tous les matériaux qui les utilisent.
        let materials_by_slot: HashMap<MaterialSlot, Handle<StandardMaterial>> = MaterialSlot::ALL
            .iter()
            .map(|&slot| (slot, materials.add(slot_material(palette, slot, &mut self.detail_textures, images))))
            .collect();
        self.factions.push(FactionLook { palette: palette.clone(), team_materials: vec![ModelMaterials(materials_by_slot)] });
        let start = self.visuals.len();
        self.visuals.extend(types.iter().map(|t| build_visual(&t.parts, palette, meshes)));
        // Les volants passent au-dessus des unités au sol : le bas de leur
        // modèle est au moins à `FLIGHT_FLOOR` du sol.
        for (t, visual) in types.iter().zip(&self.visuals[start..]) {
            let bottom = 2.0 * visual.center_height - visual.top;
            self.altitudes.push(if t.hover.is_some() { (FLIGHT_FLOOR - bottom).max(0.0) } else { 0.0 });
        }
        self.type_faction.extend(std::iter::repeat_n(faction, types.len()));
        self.types.extend(types);
        // Une faction ajoutée après coup reçoit, elle aussi, les couleurs d'équipe.
        if !self.team_colors.is_empty() {
            self.apply_team_colors(faction, materials, images);
        }
        faction
    }

    /// Nombre de factions du catalogue.
    pub fn faction_count(&self) -> usize {
        self.factions.len()
    }

    /// La faction du type `type_index` (0 pour un indice inconnu).
    pub fn faction_of(&self, type_index: usize) -> usize {
        self.type_faction.get(type_index).copied().unwrap_or(0)
    }

    /// Les indices des types de la faction `faction` (ils se suivent).
    pub fn faction_types(&self, faction: usize) -> std::ops::Range<usize> {
        let start = self.type_faction.iter().position(|&f| f == faction).unwrap_or(self.types.len());
        let end = self.type_faction.iter().rposition(|&f| f == faction).map_or(start, |i| i + 1);
        start..end
    }

    /// Indice du premier type de la faction `faction` : son type local `k`
    /// (dans sa propre liste) est le type `first_type(faction) + k` du catalogue.
    pub fn first_type(&self, faction: usize) -> usize {
        self.faction_types(faction).start
    }

    /// Donne à chaque joueur sa couleur d'équipe : `colors[i]` est celle du
    /// joueur `i` (le joueur 0 garde la palette s'il manque des couleurs),
    /// dans toutes les factions. À appeler **avant** de créer le catalogue
    /// des bâtiments, qui reprend ces matériaux.
    ///
    /// `&mut self` : la méthode modifie le catalogue (emprunt exclusif).
    pub fn set_team_colors(&mut self, colors: &[Color], materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) {
        self.team_colors = colors.to_vec();
        for faction in 0..self.factions.len() {
            self.apply_team_colors(faction, materials, images);
        }
    }

    /// Recrée les matériaux de couleur d'équipe de la faction `faction`.
    fn apply_team_colors(&mut self, faction: usize, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) {
        let look = &mut self.factions[faction];
        let base = look.team_materials[0].clone();
        look.team_materials = vec![base.clone()];
        for (index, &color) in self.team_colors.iter().enumerate() {
            let mut palette = look.palette.clone();
            palette.team.color = color;
            let mut team = base.clone();
            // Seuls les emplacements qui dérivent de la couleur d'équipe changent.
            for slot in [MaterialSlot::Team, MaterialSlot::Hazard] {
                team.0.insert(slot, materials.add(slot_material(&palette, slot, &mut self.detail_textures, images)));
            }
            if index == 0 {
                look.team_materials[0] = team;
            } else {
                look.team_materials.push(team);
            }
        }
    }

    /// Les matériaux du joueur `owner` pour un modèle de la faction `faction`.
    pub(crate) fn materials_for(&self, faction: usize, owner: PlayerId) -> &ModelMaterials {
        let team = &self.factions[faction.min(self.factions.len() - 1)].team_materials;
        team.get(usize::from(owner.0)).unwrap_or(&team[0])
    }

    /// Prépare les maillages d'un autre modèle (un bâtiment) avec la palette
    /// de la faction `faction`.
    pub(crate) fn make_visual(&self, faction: usize, parts: &[Part], meshes: &mut Assets<Mesh>) -> TypeVisual {
        build_visual(parts, &self.factions[faction].palette, meshes)
    }

    /// Le matériau partagé d'un emplacement de la palette de la faction
    /// `faction` (premier joueur).
    pub(crate) fn material(&self, faction: usize, slot: MaterialSlot) -> Handle<StandardMaterial> {
        self.factions[faction].team_materials[0].get(slot)
    }

    /// Les matériaux de tous les joueurs pour la faction `faction` (des
    /// poignées : la copie ne duplique rien).
    pub(crate) fn all_team_materials(&self, faction: usize) -> Vec<ModelMaterials> {
        self.factions[faction].team_materials.clone()
    }

    /// Le matériau de l'anneau de sélection.
    pub(crate) fn ring_material(&self) -> Handle<StandardMaterial> {
        self.ring_material.clone()
    }

    /// Le matériau de l'anneau de sélection d'une entité adverse (rouge).
    pub(crate) fn enemy_ring_material(&self) -> Handle<StandardMaterial> {
        self.enemy_ring_material.clone()
    }

    /// Hauteur du sommet du modèle d'un type d'unité (barres de vie).
    pub(crate) fn model_top(&self, type_index: usize) -> f32 {
        self.visuals.get(type_index).map_or(1.5, |v| v.top) + self.altitude(type_index)
    }

    /// De combien le modèle d'un type volant est surélevé (0 au sol).
    pub(crate) fn altitude(&self, type_index: usize) -> f32 {
        self.altitudes.get(type_index).copied().unwrap_or(0.0)
    }

    /// Épave d'une unité détruite (affichage seulement) : son modèle, sans
    /// rien de la simulation, posé au sol en `pos`, tourné de `yaw`. Mêmes
    /// maillages et matériaux partagés que les unités vivantes.
    pub(crate) fn spawn_wreck(&self, commands: &mut Commands, owner: PlayerId, type_index: usize, pos: Vec2, yaw: f32) -> Option<Entity> {
        let visual = self.visuals.get(type_index)?;
        let materials = self.materials_for(self.faction_of(type_index), owner);
        let root = commands
            .spawn((Name::new("Épave"), Transform::from_xyz(pos.x, 0.0, pos.y).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default()))
            .with_children(|body| spawn_model(body, visual, position_jitter(pos), materials))
            .id();
        Some(root)
    }

    /// Fait apparaître une unité du type `type_index` à la position (x, z) du sol.
    pub fn spawn(&self, commands: &mut Commands, type_index: usize, pos: Vec2) -> Entity {
        self.spawn_facing(commands, type_index, pos, 0.0)
    }

    /// Comme [`UnitCatalog::spawn`], tournée de `yaw` radians autour de la
    /// verticale (0 : regarde vers −z ; π : regarde vers +z). Comme `spawn`,
    /// l'unité appartient au premier joueur (`PlayerId(0)`).
    pub fn spawn_facing(&self, commands: &mut Commands, type_index: usize, pos: Vec2, yaw: f32) -> Entity {
        self.spawn_owned(commands, PlayerId(0), type_index, pos, yaw)
    }

    /// Fait apparaître une unité du joueur `owner`, tournée de `yaw` radians.
    pub fn spawn_owned(&self, commands: &mut Commands, owner: PlayerId, type_index: usize, pos: Vec2, yaw: f32) -> Entity {
        let unit_type = &self.types[type_index];
        let visual = &self.visuals[type_index];
        let center = visual.center_height;
        // Volant : toute l'unité est surélevée (clic, barre de vie suivent),
        // sauf son anneau de sélection, qui reste au sol.
        let altitude = self.altitude(type_index);
        let ring_radius = unit_type.footprint_radius() + 0.2;
        // Nombre « au hasard » mais fixe, tiré de la position (entre 0 et 1) :
        // décale animations et flottement d'une unité à l'autre.
        let jitter = position_jitter(pos);

        commands
            .spawn((
                Name::new(unit_type.name.clone()),
                Unit { type_index },
                Owner(owner),
                // L'origine de la racine est à mi-hauteur de l'unité : c'est là
                // que la sélection teste les clics.
                Transform::from_xyz(pos.x, center + altitude, pos.y).with_rotation(Quat::from_rotation_y(yaw)),
                Visibility::default(),
                SimPosition::new(pos),
                MoveSpeed(unit_type.speed),
                Selectable { radius: ring_radius, half_extents: None },
                // Empreinte au sol pour les collisions entre unités (collision.rs).
                // Les unités volantes sont dans leur propre couche : elles
                // survolent les unités au sol et les obstacles.
                Collider {
                    radius: unit_type.footprint_radius(),
                    mass: unit_type.mass,
                    layer: if unit_type.hover.is_some() { CollisionLayer::Air } else { CollisionLayer::Ground },
                },
            ))
            // `with_children` : les entités créées dans la fermeture deviennent
            // des enfants, qui suivent les déplacements de leur parent.
            .with_children(|parent| {
                parent.spawn((
                    SelectionRing,
                    Mesh3d(self.ring_mesh.clone()),
                    MeshMaterial3d(self.ring_material.clone()),
                    // L'anneau est dessiné dans le plan XY : on le couche sur le sol,
                    // juste au-dessus pour éviter qu'il clignote avec le terrain.
                    Transform::from_xyz(0.0, -center - altitude + 0.03, 0.0)
                        .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::splat(ring_radius)),
                    Visibility::Hidden,
                ));

                // Le corps : ramène l'origine des pièces au niveau du sol.
                let mut body = parent.spawn((
                    Name::new("Corps"),
                    Transform::from_xyz(0.0, -center, 0.0),
                    Visibility::default(),
                    BodyMotion::new(-center, unit_type.footprint_radius(), unit_type.mass, unit_type.hover.is_some(), jitter),
                ));
                if let Some(motion) = unit_type.hover {
                    body.insert(Hover {
                        amplitude: motion.amplitude,
                        frequency: motion.frequency,
                        // Décalage tiré de la position : les unités ne flottent pas en chœur.
                        phase: jitter * std::f32::consts::TAU,
                        base_height: -center,
                    });
                }
                body.with_children(|body| spawn_model(body, visual, jitter, self.materials_for(self.faction_of(type_index), owner)));
            })
            .id()
    }
}

/// Les matériaux d'une palette, un par emplacement.
#[derive(Debug, Clone)]
pub(crate) struct ModelMaterials(HashMap<MaterialSlot, Handle<StandardMaterial>>);

impl ModelMaterials {
    /// Le matériau d'un emplacement.
    pub(crate) fn get(&self, slot: MaterialSlot) -> Handle<StandardMaterial> {
        self.0[&slot].clone()
    }
}

/// Le matériau Bevy d'un emplacement de `palette`, avec ses textures de
/// détail (créées une seule fois, rangées dans `cache`).
fn slot_material(
    palette: &Palette,
    slot: MaterialSlot,
    cache: &mut HashMap<SurfaceDetail, Option<(Handle<Image>, Handle<Image>)>>,
    images: &mut Assets<Image>,
) -> StandardMaterial {
    let style = palette.surface(slot);
    let textures = cache
        .entry(style.detail)
        .or_insert_with(|| detail_images(style.detail).map(|t| (images.add(t.albedo), images.add(t.normal))))
        .clone();
    surface_material(&style, slot, textures)
}

/// Crée sous `body` toutes les pièces d'un modèle (unité ou bâtiment) :
/// maillages fixes et groupes animés. `jitter` (entre 0 et 1) décale les
/// animations d'un modèle à l'autre.
pub(crate) fn spawn_model(body: &mut ChildSpawnerCommands, visual: &TypeVisual, jitter: f32, materials: &ModelMaterials) {
    let phase = ((jitter * PULSE_PHASES as f32) as usize).min(PULSE_PHASES - 1);
    for (mesh, key) in &visual.meshes {
        spawn_piece(body, mesh, *key, phase, materials);
    }
    // Chaque groupe animé : une entité placée sur son pivot, qui porte les
    // maillages du groupe.
    for animated in &visual.animated {
        body.spawn((
            Name::new("Pièces animées"),
            AnimatedPart { motion: animated.group.motion, rest: animated.group.pivot, phase: jitter * 10.0 },
            animated.group.pivot,
            Visibility::default(),
        ))
        .with_children(|group| {
            for (mesh, key) in &animated.meshes {
                spawn_piece(group, mesh, *key, phase, materials);
            }
        });
    }
}

/// Crée une pièce (un maillage) sous `parent`. Les pièces qui pulsent
/// reçoivent un [`PulsingPiece`] : le moteur leur donnera un matériau qui
/// pulse, décalé selon `phase` (de 0 à `PULSE_PHASES` − 1).
fn spawn_piece(parent: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>, key: MaterialKey, phase: usize, materials: &ModelMaterials) {
    let (slot, pulse) = key;
    let mut piece = parent.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(materials.0[&slot].clone())));
    if !slot.casts_shadow() {
        piece.insert(NotShadowCaster);
    }
    if let Some(pulse) = pulse {
        // Décalage d'une fraction de période : 0, 1/3, 2/3…
        let offset = phase as f32 / PULSE_PHASES as f32 / pulse.frequency.max(1e-3);
        piece.insert(PulsingPiece { pulse, phase: offset });
    }
}

/// Nombre « au hasard » mais fixe, tiré d'une position (entre 0 et 1).
pub(crate) fn position_jitter(pos: Vec2) -> f32 {
    ((pos.x * 12.9898 + pos.y * 78.233).sin() * 43_758.547).rem_euclid(1.0)
}

/// Prépare les maillages d'un modèle : les pièces sont fusionnées en un
/// maillage par (groupe animé, matériau).
fn build_visual(parts: &[Part], palette: &Palette, meshes: &mut Assets<Mesh>) -> TypeVisual {
    // Seules les surfaces qui émettent de la lumière peuvent pulser ; pour
    // les autres, la pulsation est ignorée (pas de maillage en plus).
    let effective_pulse = |slot: MaterialSlot, pulse: Option<Pulse>| {
        pulse.filter(|_| palette.surface(slot).emissive > 0.0 || slot == MaterialSlot::Flame)
    };

    // Les pièces sont réparties en « lots » : un par (groupe animé,
    // matériau). Un `Vec` plutôt qu'une `HashMap` pour garder l'ordre
    // d'écriture (et donc un résultat identique à chaque lancement).
    let mut batches: Vec<(Option<AnimatedGroup>, MaterialKey, Vec<&Part>)> = Vec::new();
    for part in parts {
        let key = (part.slot, effective_pulse(part.slot, part.pulse));
        match batches.iter_mut().find(|(group, k, _)| *group == part.motion && *k == key) {
            Some((_, _, parts)) => parts.push(part),
            None => batches.push((part.motion, key, vec![part])),
        }
    }

    let mut bounds = (f32::INFINITY, f32::NEG_INFINITY);
    let mut visual = TypeVisual { meshes: Vec::new(), animated: Vec::new(), center_height: 0.0, top: 0.0 };
    for (group, key, parts) in batches {
        let mesh = merge_parts(parts.iter().map(|p| (&p.shape, p.transform)));
        if mesh.count_vertices() == 0 {
            continue;
        }
        // Hauteurs minimale et maximale des sommets (dans le modèle,
        // pivot compris) : l'étendue verticale du modèle.
        let pivot = group.map_or(Transform::IDENTITY, |g| g.pivot);
        if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            for p in positions {
                let y = pivot.transform_point(Vec3::from_array(*p)).y;
                bounds.0 = bounds.0.min(y);
                bounds.1 = bounds.1.max(y);
            }
        }
        let handle = meshes.add(mesh);
        match group {
            None => visual.meshes.push((handle, key)),
            Some(group) => match visual.animated.iter_mut().find(|a| a.group == group) {
                Some(animated) => animated.meshes.push((handle, key)),
                None => visual.animated.push(AnimatedVisual { group, meshes: vec![(handle, key)] }),
            },
        }
    }
    if bounds.0.is_finite() {
        visual.center_height = (bounds.0 + bounds.1) / 2.0;
        visual.top = bounds.1;
    }
    visual
}

/// Matériau PBR de Bevy correspondant à un style de surface.
fn surface_material(
    style: &SurfaceStyle,
    slot: MaterialSlot,
    textures: Option<(Handle<Image>, Handle<Image>)>,
) -> StandardMaterial {
    if slot == MaterialSlot::Flame {
        // Flamme : rien d'éclairé, seulement de la lumière émise, **ajoutée**
        // à l'image déjà dessinée derrière (`AlphaMode::Add`). Les deux faces
        // sont visibles (`cull_mode: None`) pour que la flamme paraisse pleine.
        return StandardMaterial {
            base_color: Color::BLACK,
            emissive: style.color.to_linear() * style.emissive.max(1.0) * 0.8,
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            alpha_mode: AlphaMode::Add,
            cull_mode: None::<Face>,
            fog_enabled: false,
            ..default()
        };
    }
    if matches!(slot, MaterialSlot::Jewel | MaterialSlot::Brilliant) {
        // Pierres : éclairées **et** lumineuses (les deux s'additionnent).
        return StandardMaterial {
            base_color: style.color,
            metallic: style.metallic,
            perceptual_roughness: style.roughness,
            emissive: style.color.to_linear() * style.emissive,
            ..default()
        };
    }
    if style.emissive > 0.0 {
        return StandardMaterial {
            base_color: Color::BLACK,
            emissive: style.color.to_linear() * style.emissive,
            perceptual_roughness: style.roughness,
            ..default()
        };
    }
    let (albedo, normal) = textures.unzip();
    StandardMaterial {
        base_color: style.color,
        metallic: style.metallic,
        perceptual_roughness: style.roughness,
        base_color_texture: albedo,
        normal_map_texture: normal,
        // Les UV des pièces sont en mètres : on les divise pour qu'une
        // répétition du motif mesure `detail_size` mètres.
        uv_transform: Affine2::from_scale(Vec2::splat(1.0 / style.detail_size)),
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        let plain = SurfaceStyle::new(Color::WHITE, 0.0, 0.5, SurfaceDetail::Smooth);
        Palette {
            paint: plain,
            dark: plain,
            steel: plain,
            bare: plain,
            rubber: plain,
            team: plain,
            glow: SurfaceStyle::glowing(Color::WHITE, 2.0),
            engine: SurfaceStyle::glowing(Color::WHITE, 2.0),
            selection: LinearRgba::WHITE,
        }
    }

    #[test]
    fn each_player_gets_its_own_team_color() {
        let (mut meshes, mut materials, mut images) = (Assets::default(), Assets::default(), Assets::default());
        let mut catalog = UnitCatalog::new(Vec::new(), &palette(), &mut meshes, &mut materials, &mut images);
        let orange = Color::srgb(0.9, 0.45, 0.17);
        let blue = Color::srgb(0.18, 0.42, 0.94);
        catalog.set_team_colors(&[orange, blue], &mut materials, &mut images);
        let color = |player: u8, slot: MaterialSlot| {
            let handle = catalog.materials_for(0, PlayerId(player)).get(slot);
            materials.get(&handle).unwrap().base_color
        };
        assert_eq!(color(0, MaterialSlot::Team), orange);
        assert_eq!(color(1, MaterialSlot::Team), blue);
        assert_eq!(color(1, MaterialSlot::Hazard), blue);
        // Les autres matériaux restent partagés par tous les joueurs (dessin en lot).
        let paint = |player: u8| catalog.materials_for(0, PlayerId(player)).get(MaterialSlot::Paint);
        assert_eq!(paint(0), paint(1));
        // Un joueur sans couleur prend celle du premier.
        assert_eq!(color(5, MaterialSlot::Team), orange);
    }
}
