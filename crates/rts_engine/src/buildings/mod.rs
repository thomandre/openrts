//! Bâtiments : description en données, chantiers et construction.
//!
//! Comme pour les unités, le moteur ne sait pas quels bâtiments existent : le
//! jeu lui donne une liste de [`BuildingType`] (nom, empreinte en cases, points
//! de vie, temps de construction, coût, prérequis, raccourci, modèle) dans un
//! [`BuildingCatalog`]. Le modèle se décrit avec le même `ModelBuilder` que les
//! unités et reprend la palette de la faction : mêmes matériaux, mêmes
//! animations, même dessin en lot.
//!
//! Un bâtiment occupe un bloc de cases de la grille de navigation : c'est un
//! obstacle fixe ([`StaticObstacle`]) que les unités contournent.
//!
//! La construction suit un [`ConstructionStyle`], propre à chaque faction :
//!
//! - **l'ouvrier reste sur le chantier** (`WorkerStays`, la Garde) : le
//!   chantier est posé tout de suite (obstacle, points de vie qui montent) et
//!   n'avance que tant qu'un ouvrier le touche ;
//! - **l'ouvrier devient le bâtiment** (`WorkerConsumed`, les Sylves) : à la
//!   pose, l'emplacement est seulement promis ([`PlannedSite`], déjà payé) ;
//!   l'ouvrier s'y rend, et à l'arrivée il disparaît (sa population est
//!   libérée) pour laisser place au chantier, qui pousse ensuite tout seul.
//!   Annulé, le chantier rend une part du coût et l'ouvrier ressort de terre.
//! - **sertissage à distance** (`RemoteAnchor`, le Collectif) : comme
//!   ci-dessus, l'emplacement est d'abord promis ; l'ouvrier s'y rend, y pose
//!   une « gemme d'ancrage » (le chantier apparaît) puis **repart aussitôt**
//!   à sa récolte ; le bâtiment s'assemble seul pendant son temps de
//!   construction. Annulé, il rend une part de son coût.
//!
//! Certains bâtiments émettent un **champ de construction** (rayon en
//! données) et d'autres ne se posent qu'à l'intérieur d'un champ du même
//! joueur : voir le module [`field`].
//!
//! C'est l'asymétrie voulue par le design : mêmes règles du moteur, données
//! différentes.
//!
//! Tout ce qui change l'état du jeu (pose effective, progression, obstacles)
//! se fait dans la simulation à pas fixe ; le module [`placement`] (mode pose,
//! fantôme) et le module [`ui`] (barre de boutons, barres de progression,
//! bâtiment qui monte du sol) ne font que de l'affichage et des demandes.

pub mod field;
pub mod placement;
pub(crate) mod ui;

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::collision::{Collider, StaticObstacle};
use crate::navigation::{GridFrame, MoveGroup, NavGrid, NavPath};
use crate::orders::{owned_units, PlayerCommand, TickCommands};
use crate::players::{Owner, PlayerId};
use crate::selection::{Selectable, SelectionRing};
use crate::simulation::{MoveTarget, SimPosition, SimSet};
use crate::units::{position_jitter, spawn_model, MaterialSlot, ModelMaterials, Part, TypeVisual, Unit, UnitCatalog};
use crate::RtsSystems;

// `pub use` : le type `Cost` (défini dans `economy`) est aussi accessible
// sous le nom `buildings::Cost`.
pub use crate::economy::Cost;
use crate::economy::harvest::{HarvestPhase, Harvesting};
use field::{ConstructionFields, FieldBuildings, FieldVisuals};
use crate::economy::production::ProductionQueue;
use crate::economy::{Notice, NoticeKind, ResourceKind, ResourceNode};
use crate::players::Players;

/// Où un bâtiment peut se poser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlacementRule {
    /// Sur n'importe quel terrain libre.
    #[default]
    Anywhere,
    /// Exactement sur une source libre de cette ressource (même empreinte
    /// que la source) : c'est le bâtiment d'extraction de la ressource rare.
    OnResource(ResourceKind),
}

/// Description d'un type de bâtiment, purement en données.
///
/// `Default` permet d'écrire `BuildingType { name: …, ..default() }` en ne
/// donnant que les champs utiles : les autres prennent leur valeur par défaut.
#[derive(Debug, Clone, Default)]
pub struct BuildingType {
    pub name: String,
    /// Empreinte au sol en cases de la grille de navigation : (x, z).
    pub footprint: UVec2,
    pub max_health: f32,
    /// Temps de construction, en secondes.
    pub build_time: f32,
    pub cost: Cost,
    /// Indices (dans le catalogue) des bâtiments qu'il faut déjà posséder,
    /// terminés, pour pouvoir construire celui-ci.
    pub requires: Vec<usize>,
    /// Touche du raccourci, après la touche du menu de construction
    /// ([`BuildingCatalog::menu_key`]).
    pub hotkey: char,
    /// Les pièces du modèle, pieds au sol (y = 0), centré sur l'empreinte,
    /// avant vers −z.
    pub parts: Vec<Part>,
    /// Où il peut se poser.
    pub placement: PlacementRule,
    /// Dépôt : les ouvriers y rapportent leur récolte (bâtiment principal).
    pub dropoff: bool,
    /// Population fournie une fois terminé (voir `economy::production`).
    pub supply: u32,
    /// Les unités qu'il produit, dans l'ordre des boutons.
    pub production: Vec<ProductionOption>,
    /// Rayon (mètres, depuis le centre de l'empreinte) du champ de
    /// construction qu'il émet une fois terminé ; 0 : aucun champ.
    pub field_radius: f32,
    /// Ne se pose que si le centre de son empreinte est dans un champ de
    /// construction actif du même joueur (voir [`field`]).
    pub needs_field: bool,
}

/// Une unité qu'un bâtiment sait produire.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductionOption {
    /// Type d'unité (indice du catalogue d'unités).
    pub unit_type: usize,
    /// Touche du raccourci (sans menu : le bâtiment sélectionné suffit).
    pub hotkey: char,
    /// Ce qu'il faut posséder pour la produire.
    pub requires: Vec<Requirement>,
}

/// Un prérequis de production.
#[derive(Debug, Clone, PartialEq)]
pub enum Requirement {
    /// Un bâtiment terminé de ce type (indice du catalogue des bâtiments).
    Building(usize),
    /// Un bâtiment qui n'existe pas encore dans le jeu (son nom, pour
    /// l'afficher) : l'option reste grisée.
    NotYetAvailable(String),
}

impl Requirement {
    /// Le prérequis est-il rempli (`completed` : voir [`completed_types`]) ?
    pub fn met(&self, completed: &[bool]) -> bool {
        match self {
            Requirement::Building(index) => completed.get(*index).copied().unwrap_or(false),
            Requirement::NotYetAvailable(_) => false,
        }
    }

    /// Le nom de ce qu'il faut posséder.
    pub fn label(&self, catalog: &BuildingCatalog) -> String {
        match self {
            Requirement::Building(index) => catalog.types.get(*index).map_or_else(String::new, |t| t.name.clone()),
            Requirement::NotYetAvailable(name) => name.clone(),
        }
    }
}

/// Façon de construire d'une faction.
///
/// Une variante d'`enum` peut porter ses propres données, comme une petite
/// structure : `WorkerConsumed { refund_percent: 75 }`. On les lit avec
/// `match` (ou `matches!`), qui donne accès aux champs de la variante trouvée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionStyle {
    /// L'ouvrier se rend sur le chantier et y reste jusqu'à la fin : la
    /// construction n'avance que tant qu'il le touche. Un chantier annulé
    /// est remboursé en entier.
    WorkerStays,
    /// L'ouvrier se rend à l'emplacement et s'y transforme en chantier : il
    /// disparaît, et le chantier pousse seul jusqu'au bout. Un chantier
    /// annulé rend `refund_percent` % de son coût, et l'ouvrier.
    WorkerConsumed { refund_percent: u32 },
    /// Sertissage à distance : l'ouvrier se rend à l'emplacement, y pose le
    /// chantier (une « gemme d'ancrage ») et repart aussitôt à sa récolte ;
    /// le chantier s'assemble seul jusqu'au bout. Un chantier annulé rend
    /// `refund_percent` % de son coût (l'ouvrier, lui, n'a rien perdu).
    RemoteAnchor { refund_percent: u32 },
}

impl ConstructionStyle {
    /// L'ouvrier disparaît-il dans le bâtiment ?
    pub fn consumes_worker(self) -> bool {
        matches!(self, ConstructionStyle::WorkerConsumed { .. })
    }

    /// Le chantier avance-t-il seul, sans ouvrier au contact ?
    pub fn builds_alone(self) -> bool {
        !matches!(self, ConstructionStyle::WorkerStays)
    }

    /// L'emplacement est-il d'abord promis à un ouvrier qui doit s'y rendre
    /// avant que le chantier n'apparaisse ([`PlannedSite`]) ? Oui pour les
    /// deux styles où le bâtiment se construit seul.
    pub fn promises_site(self) -> bool {
        self.builds_alone()
    }

    /// Ce que rend un chantier de coût `cost` quand on l'annule.
    pub fn cancel_refund(self, cost: Cost) -> Cost {
        match self {
            ConstructionStyle::WorkerStays => cost,
            ConstructionStyle::WorkerConsumed { refund_percent } | ConstructionStyle::RemoteAnchor { refund_percent } => {
                cost.percent(refund_percent)
            }
        }
    }
}

/// Tous les types de bâtiments d'une faction, avec leurs maillages prêts.
#[derive(Resource)]
pub struct BuildingCatalog {
    pub types: Vec<BuildingType>,
    /// Types d'unités (indices du catalogue d'unités) qui savent construire.
    pub builders: Vec<usize>,
    pub style: ConstructionStyle,
    /// Touche qui ouvre le menu de construction (« B » puis la touche du bâtiment).
    pub menu_key: char,
    /// Orientation des bâtiments posés, en radians autour de la verticale :
    /// π (par défaut) tourne leur façade (−z dans le modèle) vers la caméra.
    /// Seuls 0 et π gardent l'empreinte telle quelle.
    pub facing: f32,
    visuals: Vec<TypeVisual>,
    /// Les matériaux de la faction, partagés avec les unités.
    materials: ModelMaterials,
    /// Pour chaque type : le cadre de sélection, le cadre du chantier et la
    /// dalle (fantôme) à ses dimensions.
    outlines: Vec<Handle<Mesh>>,
    site_frames: Vec<Handle<Mesh>>,
    pads: Vec<Handle<Mesh>>,
    ring_material: Handle<StandardMaterial>,
    site_material: Handle<StandardMaterial>,
    /// Échafaudage des chantiers : poteau (cylindre de 1 m de haut, mis à
    /// l'échelle) et son matériau.
    pole_mesh: Handle<Mesh>,
    pole_material: Handle<StandardMaterial>,
    /// Fantôme : vert (emplacement libre) ou rouge (emplacement interdit).
    ghost_materials: [Handle<StandardMaterial>; 2],
    /// Dessin au sol des champs de construction (si la faction en a).
    pub(crate) field_visuals: Option<FieldVisuals>,
    /// Sertissage à distance : le modèle de la gemme d'ancrage, qui flotte
    /// au-dessus d'un chantier pendant qu'il s'assemble.
    anchor_visual: Option<TypeVisual>,
}

/// Marge (mètres) entre l'empreinte et le cadre de sélection.
const OUTLINE_MARGIN: f32 = 0.25;

impl BuildingCatalog {
    /// Prépare les maillages des bâtiments avec les matériaux du catalogue
    /// d'unités de la même faction (mêmes couleurs, couleur d'équipe comprise).
    pub fn new(
        types: Vec<BuildingType>,
        builders: Vec<usize>,
        style: ConstructionStyle,
        units: &UnitCatalog,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let visuals = types.iter().map(|t| units.make_visual(&t.parts, meshes)).collect();
        let half_sizes: Vec<Vec2> = types.iter().map(|t| t.footprint.as_vec2() / 2.0).collect();
        let outlines = half_sizes.iter().map(|h| meshes.add(frame_mesh(*h + OUTLINE_MARGIN, 0.18))).collect();
        let site_frames = half_sizes.iter().map(|h| meshes.add(frame_mesh(*h + 0.35, 0.45))).collect();
        let pads = half_sizes
            .iter()
            .map(|h| meshes.add(Plane3d::default().mesh().size(h.x * 2.0, h.y * 2.0).build()))
            .collect();
        let ghost = |color: Color| StandardMaterial {
            base_color: color,
            emissive: color.to_linear() * 0.6,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            fog_enabled: false,
            ..default()
        };
        Self {
            types,
            builders,
            style,
            menu_key: 'b',
            facing: std::f32::consts::PI,
            visuals,
            materials: units.model_materials(),
            outlines,
            site_frames,
            pads,
            ring_material: units.ring_material(),
            // Cadre de chantier : chevrons de danger pour un chantier
            // d'ouvriers ; un simple anneau (de mousse, chez les Sylves) autour
            // d'un bâtiment qui pousse seul.
            site_material: units.material(if style.builds_alone() { MaterialSlot::Steel } else { MaterialSlot::Hazard }),
            pole_mesh: meshes.add(Cylinder::new(0.07, 1.0)),
            pole_material: units.material(MaterialSlot::Dark),
            ghost_materials: [
                materials.add(ghost(Color::srgba(0.25, 1.0, 0.45, 0.38))),
                materials.add(ghost(Color::srgba(1.0, 0.18, 0.12, 0.42))),
            ],
            field_visuals: None,
            anchor_visual: None,
        }
    }

    /// Prépare le dessin au sol des champs de construction, de la couleur
    /// `color` (celle de la faction). S'utilise en chaîne après `new` :
    /// `BuildingCatalog::new(…).with_construction_field(…)`. `self` (sans
    /// `&`) : la fonction prend le catalogue, le modifie et le rend.
    pub fn with_construction_field(
        mut self,
        color: Color,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let radii = self.types.iter().map(|t| t.field_radius);
        self.field_visuals = Some(FieldVisuals::new(radii, color, meshes, materials));
        self
    }

    /// Sertissage à distance : le modèle (`parts`, pieds en y = 0) de la
    /// gemme d'ancrage qui flotte au-dessus des chantiers, avec la palette du
    /// catalogue d'unités `units`.
    pub fn with_anchor_marker(mut self, parts: &[Part], units: &UnitCatalog, meshes: &mut Assets<Mesh>) -> Self {
        self.anchor_visual = Some(units.make_visual(parts, meshes));
        self
    }

    /// Les prérequis du type `type_index` sont-ils remplis ? `completed[i]`
    /// dit si le joueur possède un bâtiment terminé du type `i`.
    pub fn requirements_met(&self, type_index: usize, completed: &[bool]) -> bool {
        self.types[type_index].requires.iter().all(|&r| completed.get(r).copied().unwrap_or(false))
    }

    /// Le type `type_index` peut-il se poser sur le bloc de cases dont le coin
    /// est `min_cell` ? Vérifie les prérequis (`completed`, voir
    /// [`completed_types`]), le champ de construction s'il en faut un
    /// (`fields` : ceux du joueur), puis le terrain (ou la source, pour un
    /// bâtiment d'extraction). En cas de succès, renvoie la source couverte,
    /// s'il y en a une.
    pub fn check_placement<'a>(
        &self,
        type_index: usize,
        min_cell: IVec2,
        grid: &NavGrid,
        completed: &[bool],
        fields: &ConstructionFields,
        nodes: impl IntoIterator<Item = (Entity, &'a ResourceNode, &'a Footprint)>,
    ) -> Result<Option<Entity>, NoticeKind> {
        let building = &self.types[type_index];
        if !self.requirements_met(type_index, completed) {
            return Err(NoticeKind::MissingRequirement);
        }
        if building.needs_field && !fields.covers(grid.frame().rect(min_cell, building.footprint).center()) {
            return Err(NoticeKind::OutsideField);
        }
        match building.placement {
            PlacementRule::Anywhere => {
                if grid.area_free(min_cell, building.footprint) {
                    Ok(None)
                } else {
                    Err(NoticeKind::Blocked)
                }
            }
            // La source est elle-même un obstacle : on ne regarde pas la
            // grille, seulement s'il y a là une source libre de même taille.
            PlacementRule::OnResource(kind) => nodes
                .into_iter()
                .find(|(_, node, footprint)| {
                    node.kind == kind
                        && node.needs_extractor
                        && node.extractor.is_none()
                        && footprint.min_cell == min_cell
                        && footprint.size == building.footprint
                })
                .map(|(entity, ..)| Some(entity))
                .ok_or(NoticeKind::MustBeOnSource),
        }
    }

    /// Case de coin où poser le type `type_index` visé au point `ground` :
    /// centré sous le pointeur, ou, pour un bâtiment d'extraction, aimanté
    /// sur la source la plus proche (à moins de 3 m de son bord).
    pub fn snap_min_cell<'a>(
        &self,
        type_index: usize,
        ground: Vec2,
        frame: GridFrame,
        nodes: impl IntoIterator<Item = (Entity, &'a ResourceNode, &'a Footprint)>,
    ) -> IVec2 {
        let building = &self.types[type_index];
        let centered = frame.min_cell_centered(ground, building.footprint);
        let PlacementRule::OnResource(kind) = building.placement else { return centered };
        nodes
            .into_iter()
            .filter(|(_, node, footprint)| node.kind == kind && node.needs_extractor && footprint.size == building.footprint)
            .map(|(entity, _, footprint)| {
                let rect = frame.rect(footprint.min_cell, footprint.size);
                (distance_to_rect(ground, rect), entity, footprint.min_cell)
            })
            .filter(|(distance, ..)| *distance <= 3.0)
            // Le plus proche ; à égalité, le plus petit identifiant.
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map_or(centered, |(.., min_cell)| min_cell)
    }

    /// Fait apparaître un bâtiment du type `type_index` sur le bloc de cases
    /// dont le coin est `min_cell`. `progress` : `None` pour un bâtiment
    /// terminé, `Some(p)` pour un chantier avancé de `p` (entre 0 et 1).
    /// Le bâtiment appartient au joueur `owner`.
    pub fn spawn(
        &self,
        commands: &mut Commands,
        frame: GridFrame,
        owner: PlayerId,
        type_index: usize,
        min_cell: IVec2,
        progress: Option<f32>,
    ) -> Entity {
        let building = &self.types[type_index];
        let visual = &self.visuals[type_index];
        let rect = frame.rect(min_cell, building.footprint);
        let center = rect.center();
        let half = rect.half_size();
        let height = visual.center_height;
        let health = match progress {
            Some(p) => construction_health(building.max_health, p),
            None => building.max_health,
        };
        let jitter = position_jitter(center);

        let mut entity = commands.spawn((
            Name::new(building.name.clone()),
            Building { type_index },
            Owner(owner),
            Footprint { min_cell, size: building.footprint },
            StaticObstacle(rect),
            Health { current: health, max: building.max_health },
            Selectable { radius: half.length(), half_extents: Some(half + OUTLINE_MARGIN) },
            // Comme pour les unités, l'origine de la racine est à mi-hauteur :
            // c'est là que la sélection teste les clics.
            Transform::from_xyz(center.x, height, center.y).with_rotation(Quat::from_rotation_y(self.facing)),
            Visibility::default(),
        ));
        if let Some(progress) = progress {
            entity.insert(UnderConstruction { progress: progress.clamp(0.0, 1.0) });
        }
        if !building.production.is_empty() {
            entity.insert(ProductionQueue::default());
        }
        entity.with_children(|parent| {
            parent.spawn((
                SelectionRing,
                Mesh3d(self.outlines[type_index].clone()),
                MeshMaterial3d(self.ring_material.clone()),
                Transform::from_xyz(0.0, -height + 0.04, 0.0),
                NotShadowCaster,
                Visibility::Hidden,
            ));
            if progress.is_some() && self.style.builds_alone() {
                // Un bâtiment qui pousse ou s'assemble seul n'a pas
                // d'échafaudage : juste un anneau au sol qui marque l'empreinte
                // pendant la construction. En sertissage à distance, la gemme
                // d'ancrage flotte à mi-hauteur du bâtiment à venir (enfant du cadre :
                // elle disparaît avec lui, à la fin de la construction).
                let top = visual.top;
                parent
                    .spawn((
                        SiteFrame,
                        Mesh3d(self.site_frames[type_index].clone()),
                        MeshMaterial3d(self.site_material.clone()),
                        Transform::from_xyz(0.0, -height + 0.03, 0.0),
                        NotShadowCaster,
                    ))
                    .with_children(|frame| {
                        if let (ConstructionStyle::RemoteAnchor { .. }, Some(anchor)) = (self.style, &self.anchor_visual) {
                            frame
                                .spawn((Name::new("Gemme d'ancrage"), Transform::from_xyz(0.0, top * 0.55, 0.0), Visibility::default()))
                                .with_children(|gem| spawn_model(gem, anchor, jitter, &self.materials));
                        }
                    });
            } else if progress.is_some() {
                // Cadre de chantier rayé de chevrons, au sol autour de
                // l'empreinte, et échafaudage à la hauteur du bâtiment fini :
                // quatre poteaux d'angle et un cadre au niveau du toit. Vu de
                // dessus comme de biais, on voit ce qui reste à construire.
                let top = visual.top;
                parent
                    .spawn((
                        SiteFrame,
                        Mesh3d(self.site_frames[type_index].clone()),
                        MeshMaterial3d(self.site_material.clone()),
                        Transform::from_xyz(0.0, -height + 0.03, 0.0),
                        NotShadowCaster,
                    ))
                    .with_children(|frame| {
                        let corner = half + 0.2;
                        for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                            frame.spawn((
                                Mesh3d(self.pole_mesh.clone()),
                                MeshMaterial3d(self.pole_material.clone()),
                                Transform::from_xyz(x * corner.x, top / 2.0, z * corner.y).with_scale(Vec3::new(1.0, top, 1.0)),
                            ));
                        }
                        frame.spawn((
                            Mesh3d(self.outlines[type_index].clone()),
                            MeshMaterial3d(self.site_material.clone()),
                            Transform::from_xyz(0.0, top, 0.0),
                        ));
                    });
            }
            parent
                .spawn((
                    Name::new("Corps"),
                    BuildingBody { center_height: height, top: visual.top, grows: self.style.consumes_worker() },
                    Transform::from_xyz(0.0, -height, 0.0),
                    Visibility::default(),
                ))
                .with_children(|body| spawn_model(body, visual, jitter, &self.materials));
        });
        entity.id()
    }

    /// Fait apparaître un fantôme (aperçu semi-transparent) du type `type_index`.
    pub fn spawn_ghost(&self, commands: &mut Commands, ghost: PlacementGhost, frame: GridFrame) -> Entity {
        let visual = &self.visuals[ghost.type_index];
        let material = self.ghost_materials[usize::from(!ghost.valid)].clone();
        let size = self.types[ghost.type_index].footprint;
        let center = frame.rect(ghost.min_cell, size).center();
        commands
            .spawn((
                Name::new("Fantôme"),
                ghost,
                Transform::from_xyz(center.x, 0.0, center.y).with_rotation(Quat::from_rotation_y(self.facing)),
                Visibility::default(),
            ))
            .with_children(|parent| {
                let mut piece = |mesh: &Handle<Mesh>, transform: Transform| {
                    parent.spawn((
                        GhostPiece,
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        transform,
                        NotShadowCaster,
                    ));
                };
                piece(&self.pads[ghost.type_index], Transform::from_xyz(0.0, 0.05, 0.0));
                for (mesh, _) in &visual.meshes {
                    piece(mesh, Transform::IDENTITY);
                }
                // Les groupes animés restent immobiles sur leur pivot.
                for animated in &visual.animated {
                    for (mesh, _) in &animated.meshes {
                        piece(mesh, animated.group.pivot);
                    }
                }
            })
            .id()
    }

    pub(crate) fn ghost_material(&self, valid: bool) -> Handle<StandardMaterial> {
        self.ghost_materials[usize::from(!valid)].clone()
    }

    /// Rectangle au sol d'un emplacement promis.
    pub fn plan_rect(&self, frame: GridFrame, plan: &PlannedSite) -> Rect {
        frame.rect(plan.min_cell, self.types[plan.type_index].footprint)
    }

    /// Repère au sol d'un emplacement promis à l'ouvrier `worker` : un cadre
    /// vert translucide à la taille du bâtiment (affichage seulement).
    pub(crate) fn spawn_plan_marker(&self, commands: &mut Commands, worker: Entity, plan: PlannedSite, frame: GridFrame) -> Entity {
        let center = self.plan_rect(frame, &plan).center();
        commands
            .spawn((
                Name::new("Emplacement promis"),
                PlannedMarker { worker, plan },
                Mesh3d(self.site_frames[plan.type_index].clone()),
                MeshMaterial3d(self.ghost_materials[0].clone()),
                Transform::from_xyz(center.x, 0.04, center.y),
                NotShadowCaster,
            ))
            .id()
    }
}

/// Points de vie d'un chantier avancé de `progress` : il démarre à 10 % et
/// monte avec la construction (comme dans la plupart des RTS).
fn construction_health(max: f32, progress: f32) -> f32 {
    max * (0.1 + 0.9 * progress.clamp(0.0, 1.0))
}

/// Cadre rectangulaire plat (dans le plan du sol, y = 0) : bord extérieur à
/// `half` du centre, épaisseur `thickness` vers l'intérieur. Les coordonnées
/// de texture sont en mètres, pour que les chevrons se répètent à la bonne taille.
fn frame_mesh(half: Vec2, thickness: f32) -> Mesh {
    let inner = (half - thickness).max(Vec2::ZERO);
    // Quatre coins extérieurs puis quatre coins intérieurs, dans le même ordre.
    let corners = [Vec2::new(-1.0, -1.0), Vec2::new(1.0, -1.0), Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0)];
    let mut positions = Vec::new();
    for c in corners {
        positions.push(Vec3::new(c.x * half.x, 0.0, c.y * half.y));
    }
    for c in corners {
        positions.push(Vec3::new(c.x * inner.x, 0.0, c.y * inner.y));
    }
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p.x + p.z, p.z - p.x]).collect();
    let mut indices = Vec::new();
    for i in 0..4u32 {
        let (o0, o1, i0, i1) = (i, (i + 1) % 4, i + 4, (i + 1) % 4 + 4);
        // Deux triangles par côté, tournés vers le haut (+y).
        indices.extend_from_slice(&[o0, i0, o1, o1, i0, i1]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.iter().map(|p| p.to_array()).collect::<Vec<_>>())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 8])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

// ---------------------------------------------------------------------------
// Composants.
// ---------------------------------------------------------------------------

/// Un bâtiment, et son type (indice dans le catalogue).
#[derive(Component, Debug, Clone, Copy)]
pub struct Building {
    pub type_index: usize,
}

/// Bloc de cases occupé par un bâtiment.
#[derive(Component, Debug, Clone, Copy)]
pub struct Footprint {
    pub min_cell: IVec2,
    pub size: UVec2,
}

/// Points de vie.
#[derive(Component, Debug, Clone, Copy)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

/// Chantier : le bâtiment n'est pas encore terminé.
#[derive(Component, Debug, Clone, Copy)]
pub struct UnderConstruction {
    /// Avancement, de 0 à 1.
    pub progress: f32,
}

/// Les chantiers confiés à un ouvrier, dans l'ordre (Maj + clic en pose
/// plusieurs à la suite). Il travaille sur le premier.
#[derive(Component, Debug, Clone, Default)]
pub struct ConstructionTask {
    pub sites: Vec<Entity>,
}

/// Style « ouvrier consommé » : l'emplacement promis à un ouvrier en route.
/// Le coût est déjà payé (`paid`) ; si l'ouvrier reçoit un autre ordre avant
/// d'arriver, il est remboursé en entier et la promesse disparaît.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PlannedSite {
    pub type_index: usize,
    pub min_cell: IVec2,
    pub paid: Cost,
}

/// Sertissage à distance : la récolte qu'un ouvrier menait avant d'aller
/// poser un chantier, qu'il reprend aussitôt la gemme d'ancrage posée.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ResumeHarvest(pub Harvesting);

/// Rayon (mètres) dans lequel un ouvrier qui vient de poser un chantier, et
/// qui ne récoltait rien, cherche un gisement de ressource principale.
const RESUME_RADIUS: f32 = 30.0;

/// Style « ouvrier consommé » : le type de l'ouvrier devenu ce chantier,
/// qui ressort de terre si le chantier est annulé.
#[derive(Component, Debug, Clone, Copy)]
pub struct RootedWorker {
    pub unit_type: usize,
}

/// Repère au sol d'un emplacement promis (affichage seulement).
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct PlannedMarker {
    pub(crate) worker: Entity,
    pub(crate) plan: PlannedSite,
}

/// Le corps d'un bâtiment (ses pièces), qui monte du sol pendant la
/// construction, ou qui grandit (`grows`) s'il pousse seul.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct BuildingBody {
    center_height: f32,
    top: f32,
    grows: bool,
}

/// Le cadre rayé d'un chantier, retiré à la fin de la construction.
#[derive(Component, Debug)]
pub(crate) struct SiteFrame;

/// Un fantôme de bâtiment : aperçu de la pose (affichage seulement).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PlacementGhost {
    pub type_index: usize,
    pub min_cell: IVec2,
    /// Suit le pointeur (le fantôme du mode pose), ou reste en place
    /// (démonstration, futur aperçu de file de construction).
    pub follows_pointer: bool,
    /// Emplacement libre : vert ; sinon rouge.
    pub valid: bool,
}

/// Une pièce d'un fantôme (son matériau change avec la validité).
#[derive(Component, Debug)]
pub(crate) struct GhostPiece;

/// Distance (en plus de son rayon) sous laquelle un ouvrier touche son chantier.
pub(crate) const CONTACT_MARGIN: f32 = 0.35;

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<placement::Placement>()
            .add_systems(
                FixedUpdate,
                (
                    // Après les déplacements : un ordre de pose donné juste
                    // après un ordre de marche l'emporte.
                    (apply_build_orders, drive_builders, root_planned_sites)
                        .chain()
                        .in_set(SimSet::Orders)
                        .after(crate::orders::apply_unit_orders),
                    progress_construction.in_set(SimSet::Construction),
                ),
            )
            .add_systems(
                Update,
                (
                    placement::handle_build_actions,
                    placement::update_pointer_ghost,
                    placement::refresh_ghosts,
                    ui::show_planned_sites,
                    field::show_construction_fields,
                    ui::update_command_bar,
                    ui::raise_buildings,
                    ui::update_progress_bars,
                )
                    .chain()
                    .in_set(RtsSystems::Actions),
            );
    }
}

/// `completed[i]` : le joueur `player` possède-t-il un bâtiment terminé du
/// type `i` ? `buildings` donne, pour chaque bâtiment, son type, son
/// propriétaire et s'il est encore en chantier.
pub fn completed_types<'a>(
    catalog: &BuildingCatalog,
    player: PlayerId,
    buildings: impl Iterator<Item = (&'a Building, &'a Owner, bool)>,
) -> Vec<bool> {
    let mut completed = vec![false; catalog.types.len()];
    for (building, owner, under_construction) in buildings {
        if owner.0 == player
            && !under_construction
            && let Some(slot) = completed.get_mut(building.type_index)
        {
            *slot = true;
        }
    }
    completed
}

/// Applique les ordres de pose du pas : vérifie l'emplacement et les
/// prérequis du joueur, puis, selon le style de construction, crée le
/// chantier et le confie à l'ouvrier le plus proche (`WorkerStays`), ou
/// promet l'emplacement à l'ouvrier libre le plus proche, qui s'y rendra
/// (`WorkerConsumed`, voir [`root_planned_sites`]).
pub(crate) fn apply_build_orders(
    mut commands: Commands,
    tick: Res<TickCommands>,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    buildings: Query<(&Building, &Owner, Has<UnderConstruction>)>,
    field_buildings: FieldBuildings,
    mut builders: Query<
        (Entity, &Owner, &SimPosition, Option<&mut ConstructionTask>, Has<PlannedSite>, Option<&Harvesting>),
        With<Unit>,
    >,
    plans: Query<&PlannedSite>,
    mut nodes: Query<(Entity, &mut ResourceNode, &Footprint)>,
    mut players: ResMut<Players>,
    mut notices: MessageWriter<Notice>,
) {
    // Pas encore de grille (tout premier pas) ou pas de bâtiments dans ce jeu.
    let (Some(grid), Some(catalog)) = (grid, catalog) else { return };
    // Styles où l'emplacement est d'abord promis à un ouvrier libre.
    let promises = catalog.style.promises_site();
    // Chantiers posés pendant ce pas : la grille ne les connaîtra qu'au
    // prochain, il faut donc aussi vérifier qu'ils ne se chevauchent pas.
    // On y ajoute les emplacements déjà promis à un ouvrier en route.
    let mut placed: Vec<Rect> = plans.iter().map(|plan| catalog.plan_rect(grid.frame(), plan)).collect();
    // Ouvriers qui ont reçu un chantier pendant ce pas (pour l'équité).
    let mut assigned: Vec<(Entity, Vec<Entity>)> = Vec::new();
    // Ouvriers qui ont reçu un emplacement promis pendant ce pas.
    let mut promised: Vec<Entity> = Vec::new();

    for issued in &tick.0 {
        let PlayerCommand::Build { type_index, min_cell, builders: ordered } = &issued.command else { continue };
        let (type_index, min_cell, player) = (*type_index, *min_cell, issued.player);
        let Some(building) = catalog.types.get(type_index) else { continue };
        let completed = completed_types(&catalog, player, buildings.iter());
        let fields = ConstructionFields::of_player(&catalog, grid.frame(), player, field_buildings.iter());
        let rect = grid.frame().rect(min_cell, building.footprint);
        let overlaps = placed.iter().any(|p| !p.intersect(rect).is_empty());
        let check = catalog.check_placement(type_index, min_cell, &grid, &completed, &fields, nodes.iter());
        let source = match (check, overlaps) {
            (Ok(source), false) => source,
            (result, _) => {
                info!("Pose refusée : {} en {}", building.name, min_cell);
                notices.write(Notice { player, kind: result.err().unwrap_or(NoticeKind::Blocked) });
                continue;
            }
        };
        // L'ouvrier : d'abord ceux qui n'ont rien à faire, puis le plus proche
        // du chantier, puis le plus petit identifiant (déterministe).
        let mine = owned_units(ordered, player, |e| builders.get(e).ok().map(|(_, o, ..)| *o));
        let mut candidates: Vec<(bool, f32, Entity)> = mine
            .iter()
            .filter_map(|&e| builders.get(e).ok())
            .map(|(entity, _, position, task, planned, _)| {
                let busy = task.is_some_and(|t| !t.sites.is_empty())
                    || planned
                    || assigned.iter().any(|(b, _)| *b == entity)
                    || promised.contains(&entity);
                (busy, position.current.distance(rect.center()), entity)
            })
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        // Un ouvrier à qui l'on promet un emplacement n'en a qu'un à la fois :
        // seuls les libres comptent.
        let Some(&(_, _, builder)) = candidates.iter().find(|c| !promises || !c.0) else {
            if promises {
                notices.write(Notice { player, kind: NoticeKind::NoWorker });
            }
            continue;
        };
        // Le coût est débité à la pose (remboursé si le chantier est annulé).
        if !players.get_mut(player).stock.try_pay(building.cost) {
            notices.write(Notice { player, kind: NoticeKind::NotEnoughResources });
            continue;
        }
        placed.push(rect);

        if promises {
            // L'emplacement est promis ; l'ouvrier lâche ce qu'il faisait et
            // s'y rendra (voir `root_planned_sites`). En sertissage à
            // distance, il retient sa récolte pour la reprendre ensuite.
            let resume = builders.get(builder).ok().and_then(|(.., harvesting)| harvesting.copied());
            let mut worker = commands.entity(builder);
            worker
                .insert(PlannedSite { type_index, min_cell, paid: building.cost })
                .remove::<(Harvesting, ConstructionTask, MoveTarget, NavPath, MoveGroup)>();
            match resume {
                Some(job) if !catalog.style.consumes_worker() => worker.insert(ResumeHarvest(job)),
                _ => worker.remove::<ResumeHarvest>(),
            };
            promised.push(builder);
            continue;
        }

        let site = catalog.spawn(&mut commands, grid.frame(), player, type_index, min_cell, Some(0.0));
        // Le bâtiment d'extraction se réserve sa source.
        if let Some(source) = source
            && let Ok((_, mut node, _)) = nodes.get_mut(source)
        {
            node.extractor = Some(site);
        }
        match assigned.iter_mut().find(|(b, _)| *b == builder) {
            Some((_, sites)) => sites.push(site),
            None => assigned.push((builder, vec![site])),
        }
    }
    for (builder, sites) in assigned {
        let Ok((_, _, _, task, ..)) = builders.get_mut(builder) else { continue };
        // Un ouvrier envoyé construire abandonne sa récolte.
        commands.entity(builder).remove::<Harvesting>();
        match task {
            Some(mut task) => task.sites.extend(sites),
            None => {
                commands.entity(builder).insert(ConstructionTask { sites });
            }
        }
    }
}

/// Rembourse au joueur `player` l'emplacement promis `plan`, abandonné
/// parce que l'ouvrier a reçu un autre ordre (déplacement, récolte).
pub(crate) fn refund_plan(players: &mut Players, player: PlayerId, plan: &PlannedSite) {
    players.get_mut(player).stock += plan.paid;
}

/// Styles où l'emplacement est promis : mène chaque ouvrier à son
/// emplacement. Arrivé, le chantier apparaît, qui se construira seul :
///
/// - ouvrier consommé : il s'enracine et disparaît (sa population est
///   libérée) ;
/// - sertissage à distance : il pose la gemme d'ancrage et repart aussitôt
///   à la récolte qu'il menait ([`ResumeHarvest`]), ou, s'il ne récoltait
///   rien, au gisement de ressource principale le plus proche.
///
/// Si l'emplacement n'est plus valable entre-temps (un autre bâtiment s'y est
/// posé, son champ de construction a disparu), la promesse est remboursée et
/// l'ouvrier s'arrête.
///
/// Arrivée : au centre de l'emplacement (à 60 cm près), ou arrêté en le
/// touchant (bousculé par d'autres) ; au bord, pour un bâtiment d'extraction,
/// dont la source est un obstacle.
fn root_planned_sites(
    mut commands: Commands,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    buildings: Query<(&Building, &Owner, Has<UnderConstruction>)>,
    field_buildings: FieldBuildings,
    mut nodes: Query<(Entity, &mut ResourceNode, &Footprint)>,
    workers: Query<(Entity, &Owner, &Unit, &SimPosition, &Collider, &PlannedSite, Has<MoveTarget>, Option<&ResumeHarvest>)>,
    mut players: ResMut<Players>,
    mut notices: MessageWriter<Notice>,
) {
    let (Some(catalog), Some(grid)) = (catalog, grid) else { return };
    let frame = grid.frame();
    // Ordre fixe : le plus petit identifiant d'abord.
    let mut order: Vec<Entity> = workers.iter().map(|w| w.0).collect();
    order.sort_unstable();
    // Chantiers apparus pendant ce pas (la grille ne les connaît pas encore).
    let mut placed: Vec<Rect> = Vec::new();

    for entity in order {
        let Ok((_, owner, unit, position, collider, plan, moving, resume)) = workers.get(entity) else { continue };
        let Some(building) = catalog.types.get(plan.type_index) else { continue };
        let rect = catalog.plan_rect(frame, plan);
        let (pos, radius) = (position.current, collider.radius);
        let on_source = matches!(building.placement, PlacementRule::OnResource(_));
        let arrived = if on_source {
            distance_to_rect(pos, rect) <= radius + if moving { CONTACT_MARGIN } else { 0.75 }
        } else {
            pos.distance(rect.center()) <= 0.6 || (!moving && distance_to_rect(pos, rect) <= radius + CONTACT_MARGIN)
        };
        if !arrived {
            // Ni arrivé, ni en route (bousculé, arrêté ailleurs) : on y va.
            if !moving {
                let goal = if on_source { approach_point(&grid, pos, radius, rect) } else { rect.center() };
                commands.entity(entity).insert(MoveTarget(goal));
            }
            continue;
        }
        // On revérifie l'emplacement et les prérequis au moment de s'enraciner.
        let player = owner.0;
        let completed = completed_types(&catalog, player, buildings.iter());
        let fields = ConstructionFields::of_player(&catalog, frame, player, field_buildings.iter());
        let overlaps = placed.iter().any(|p| !p.intersect(rect).is_empty());
        match (catalog.check_placement(plan.type_index, plan.min_cell, &grid, &completed, &fields, nodes.iter()), overlaps) {
            (Ok(source), false) => {
                let site = catalog.spawn(&mut commands, frame, player, plan.type_index, plan.min_cell, Some(0.0));
                if catalog.style.consumes_worker() {
                    // `despawn` retire l'ouvrier du monde (et ses enfants : son
                    // modèle, son anneau de sélection).
                    commands.entity(entity).despawn();
                    commands.entity(site).insert(RootedWorker { unit_type: unit.type_index });
                } else {
                    // Sertissage à distance : la gemme est posée, l'ouvrier repart.
                    let job = resume.map(|r| r.0).or_else(|| nearest_primary(&nodes, frame, pos));
                    let mut worker = commands.entity(entity);
                    worker.remove::<(PlannedSite, ResumeHarvest, MoveTarget, NavPath, MoveGroup)>();
                    if let Some(job) = job {
                        // Chargé, il rapporte d'abord sa charge ; sinon il retourne au gisement.
                        let phase = if job.carrying.is_some() { HarvestPhase::Returning } else { HarvestPhase::ToNode };
                        worker.insert(Harvesting { phase, ..job });
                    }
                }
                placed.push(rect);
                if let Some(source) = source
                    && let Ok((_, mut node, _)) = nodes.get_mut(source)
                {
                    node.extractor = Some(site);
                }
            }
            (result, _) => {
                info!("Enracinement impossible : {} en {}", building.name, plan.min_cell);
                refund_plan(&mut players, player, plan);
                notices.write(Notice { player, kind: result.err().unwrap_or(NoticeKind::Blocked) });
                commands.entity(entity).remove::<(PlannedSite, ResumeHarvest, MoveTarget, NavPath, MoveGroup)>();
            }
        }
    }
}

/// Le gisement de ressource principale (non épuisé) le plus proche de
/// `from`, à moins de [`RESUME_RADIUS`] : la récolte que reprend un ouvrier
/// qui vient de poser un chantier sans rien récolter avant.
fn nearest_primary(
    nodes: &Query<(Entity, &mut ResourceNode, &Footprint)>,
    frame: GridFrame,
    from: Vec2,
) -> Option<Harvesting> {
    nodes
        .iter()
        .filter(|(_, node, _)| node.kind == ResourceKind::Primary && !node.needs_extractor && node.amount > 0)
        .map(|(entity, _, footprint)| {
            let rect = frame.rect(footprint.min_cell, footprint.size);
            (distance_to_rect(from, rect), entity, rect.center())
        })
        .filter(|(distance, ..)| *distance <= RESUME_RADIUS)
        // Le plus proche ; à égalité, le plus petit identifiant (déterministe).
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, node, node_position)| Harvesting {
            node,
            kind: ResourceKind::Primary,
            node_position,
            phase: HarvestPhase::ToNode,
            carrying: None,
        })
}

/// Distance d'un point à un rectangle (0 à l'intérieur).
pub(crate) fn distance_to_rect(point: Vec2, rect: Rect) -> f32 {
    point.distance(point.clamp(rect.min, rect.max))
}

/// Point où un ouvrier de rayon `radius` doit se tenir pour toucher le
/// chantier `rect` : le point praticable du pourtour (agrandi de son rayon)
/// le plus proche de lui.
pub(crate) fn approach_point(grid: &NavGrid, position: Vec2, radius: f32, rect: Rect) -> Vec2 {
    let ring = rect.inflate(radius + 0.15);
    // On échantillonne le pourtour tous les 25 cm, dans un ordre fixe.
    let size = ring.size();
    let steps = ((size.x + size.y) * 2.0 / 0.25).ceil() as usize;
    let perimeter = 2.0 * (size.x + size.y);
    let point_at = |d: f32| -> Vec2 {
        let d = d % perimeter;
        if d < size.x {
            Vec2::new(ring.min.x + d, ring.min.y)
        } else if d < size.x + size.y {
            Vec2::new(ring.max.x, ring.min.y + d - size.x)
        } else if d < 2.0 * size.x + size.y {
            Vec2::new(ring.max.x - (d - size.x - size.y), ring.max.y)
        } else {
            Vec2::new(ring.min.x, ring.max.y - (d - 2.0 * size.x - size.y))
        }
    };
    // Aux coins, le pourtour est arrondi : un point du coin du rectangle
    // agrandi serait trop loin (diagonale) pour toucher le bâtiment.
    let gap = radius + 0.15;
    let round = |p: Vec2| {
        let closest = p.clamp(rect.min, rect.max);
        closest + (p - closest).normalize_or_zero() * gap
    };
    (0..steps)
        .map(|i| round(point_at(i as f32 * perimeter / steps as f32)))
        .filter(|p| grid.passable(grid.cell_of(*p), radius))
        .fold(None, |best: Option<(f32, Vec2)>, p| {
            let d = p.distance_squared(position);
            if best.is_none_or(|b| d < b.0) { Some((d, p)) } else { best }
        })
        .map_or_else(|| position.clamp(ring.min, ring.max), |(_, p)| p)
}

/// Envoie chaque ouvrier vers son chantier, et l'arrête une fois au contact.
fn drive_builders(
    mut commands: Commands,
    grid: Option<Res<NavGrid>>,
    sites: Query<(&StaticObstacle, Has<UnderConstruction>), With<Building>>,
    mut builders: Query<(Entity, &SimPosition, &Collider, &mut ConstructionTask, Option<&MoveTarget>)>,
) {
    let Some(grid) = grid else { return };
    for (entity, position, collider, mut task, target) in &mut builders {
        // Les chantiers terminés ou disparus sont retirés de la liste.
        task.sites.retain(|site| sites.get(*site).is_ok_and(|(_, building)| building));
        let Some(&site) = task.sites.first() else {
            commands.entity(entity).remove::<ConstructionTask>();
            continue;
        };
        let Ok((obstacle, _)) = sites.get(site) else { continue };
        let touching = distance_to_rect(position.current, obstacle.0) <= collider.radius + CONTACT_MARGIN;
        match (touching, target) {
            // Au contact : on s'arrête là et on construit.
            (true, Some(_)) => {
                commands.entity(entity).remove::<(MoveTarget, NavPath, MoveGroup)>();
            }
            // Ni au contact, ni en route (arrivé ailleurs, ou bousculé) : on y va.
            (false, None) => {
                let goal = approach_point(&grid, position.current, collider.radius, obstacle.0);
                commands.entity(entity).insert(MoveTarget(goal));
            }
            _ => {}
        }
    }
}

/// Fait avancer les chantiers dont l'ouvrier est au contact (ou tous, pour
/// un style où le bâtiment pousse seul).
fn progress_construction(
    mut commands: Commands,
    time: Res<Time>,
    catalog: Option<Res<BuildingCatalog>>,
    mut sites: Query<(Entity, &Building, &StaticObstacle, &mut UnderConstruction, &mut Health)>,
    builders: Query<(&SimPosition, &Collider, &ConstructionTask)>,
) {
    let Some(catalog) = catalog else { return };
    let mut worked: Vec<Entity> = if catalog.style.builds_alone() {
        // Les bâtiments qui poussent ou s'assemblent seuls avancent tous, sans ouvrier.
        sites.iter().map(|(entity, ..)| entity).collect()
    } else {
        // Chantiers sur lesquels un ouvrier travaille : son premier chantier, au contact.
        builders
            .iter()
            .filter_map(|(position, collider, task)| {
                let site = *task.sites.first()?;
                let (_, _, obstacle, ..) = sites.get(site).ok()?;
                (distance_to_rect(position.current, obstacle.0) <= collider.radius + CONTACT_MARGIN).then_some(site)
            })
            .collect()
    };
    worked.sort_unstable();
    worked.dedup();
    let dt = time.delta_secs();
    for site in worked {
        let Ok((entity, building, _, mut construction, mut health)) = sites.get_mut(site) else { continue };
        let build_time = catalog.types[building.type_index].build_time.max(0.1);
        construction.progress = (construction.progress + dt / build_time).min(1.0);
        health.current = construction_health(health.max, construction.progress);
        if construction.progress >= 1.0 {
            commands.entity(entity).remove::<UnderConstruction>();
            info!("Construction terminée : {}", catalog.types[building.type_index].name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::CollisionLayer;
    use crate::input::{GameAction, InputContext};
    use crate::economy::EconomyPlugin;
    use crate::orders::{CommandQueue, OrdersPlugin};
    use crate::players::PlayersPlugin;
    use crate::simulation::{MoveSpeed, SimulationPlugin};
    use crate::units::{Palette, SurfaceDetail, SurfaceStyle};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

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

    /// Une simulation sans fenêtre : un ouvrier, un catalogue d'un seul
    /// bâtiment de 3 × 3 cases qui se construit en 2 s.
    fn app() -> (App, Entity) {
        app_with(ConstructionStyle::WorkerStays, Cost::default())
    }

    /// Comme [`app`], avec un style de construction et un coût d'abri au choix.
    fn app_with(style: ConstructionStyle, cost: Cost) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<GameAction>()
            .init_resource::<InputContext>()
            .init_resource::<crate::cursor::Pointer>()
            .add_plugins((SimulationPlugin, PlayersPlugin, OrdersPlugin, EconomyPlugin, BuildingsPlugin))
            // Chaque `update` avance le temps d'exactement un pas de simulation.
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 32.0)));
        let (mut meshes, mut materials, mut images) = (Assets::default(), Assets::default(), Assets::default());
        // Un type d'unité (l'ouvrier, sans pièces) : il ressort de terre
        // quand on annule un chantier dans lequel il s'est enraciné.
        let worker_type =
            crate::units::UnitType { name: "Ouvrier".into(), parts: Vec::new(), footprint_radius: 0.6, mass: 1.0, speed: 4.0, hover: None };
        let units = UnitCatalog::new(vec![worker_type], &palette(), &mut meshes, &mut materials, &mut images);
        let building = BuildingType {
            name: "Abri".into(),
            footprint: UVec2::new(3, 3),
            max_health: 100.0,
            build_time: 2.0,
            cost,
            requires: Vec::new(),
            hotkey: 'a',
            ..default()
        };
        // Un second type, qui se pose sur une source de ressource rare.
        let extractor = BuildingType {
            name: "Puits".into(),
            footprint: UVec2::new(4, 4),
            placement: PlacementRule::OnResource(ResourceKind::Rare),
            ..building.clone()
        };
        // Champs de construction : un relais de 2 × 2 qui en émet un de 6 m,
        // un atelier qui en a besoin, et un cœur de base qui en émet un de 8 m.
        let relay = BuildingType { name: "Relais".into(), footprint: UVec2::new(2, 2), field_radius: 6.0, ..building.clone() };
        let workshop = BuildingType { name: "Atelier".into(), needs_field: true, ..building.clone() };
        let core = BuildingType { name: "Cœur".into(), footprint: UVec2::new(5, 5), field_radius: 8.0, ..building.clone() };
        let catalog = BuildingCatalog::new(
            vec![building, extractor, relay, workshop, core],
            vec![0],
            style,
            &units,
            &mut meshes,
            &mut materials,
        );
        app.insert_resource(catalog).insert_resource(units).insert_resource(crate::economy::EconomyRules {
            resources: [
                crate::economy::ResourceInfo { name: "P".into(), color: Color::WHITE },
                crate::economy::ResourceInfo { name: "R".into(), color: Color::WHITE },
            ],
            harvesters: vec![0],
            carry: [5, 4],
            gather_time: [2.0, 1.5],
            units: Vec::new(),
        });
        let worker = app
            .world_mut()
            .spawn((
                Unit { type_index: 0 },
                Owner(PlayerId(0)),
                SimPosition::new(Vec2::new(-10.0, 0.0)),
                MoveSpeed(4.0),
                Collider { radius: 0.6, mass: 1.0, layer: CollisionLayer::Ground },
                Selectable { radius: 0.8, half_extents: None },
            ))
            .id();
        // Quelques pas : la grille de navigation est créée au premier.
        for _ in 0..3 {
            app.update();
        }
        (app, worker)
    }

    /// Le joueur 0 demande un chantier en `min_cell`, confié à `worker`.
    fn build(app: &mut App, min_cell: IVec2, worker: Entity) {
        build_type(app, 0, min_cell, worker);
    }

    fn build_type(app: &mut App, type_index: usize, min_cell: IVec2, worker: Entity) {
        app.world_mut()
            .resource_mut::<CommandQueue>()
            .push(PlayerId(0), PlayerCommand::Build { type_index, min_cell, builders: vec![worker] });
    }

    #[test]
    fn extractor_must_cover_a_free_source() {
        let (mut app, worker) = app();
        let frame = app.world().resource::<NavGrid>().frame();
        let source = {
            let mut commands = app.world_mut().commands();
            crate::economy::spawn_resource_node(&mut commands, frame, ResourceKind::Rare, IVec2::new(60, 60), UVec2::new(4, 4), 500, true)
        };
        app.world_mut().flush();
        // À côté de la source : refusé.
        build_type(&mut app, 1, IVec2::new(40, 40), worker);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&Building>().iter(world).count(), 0, "pas de source ici");
        // Pile dessus : accepté, et la source est réservée.
        build_type(&mut app, 1, IVec2::new(60, 60), worker);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&Building>().iter(world).count(), 1);
        assert!(world.get::<ResourceNode>(source).unwrap().extractor.is_some());
        // Une seconde fois : la source est prise.
        build_type(&mut app, 1, IVec2::new(60, 60), worker);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&Building>().iter(world).count(), 1, "source déjà couverte");
    }

    #[test]
    fn worker_walks_to_the_site_and_builds_it() {
        let (mut app, worker) = app();
        build(&mut app, IVec2::new(55, 49), worker);
        // 3 s de marche au plus, puis 2 s de construction.
        for _ in 0..32 * 7 {
            app.update();
        }
        let world = app.world_mut();
        let mut sites = world.query::<(&Building, &Health, Has<UnderConstruction>, &StaticObstacle)>();
        let (_, health, under_construction, obstacle) = sites.single(world).expect("un chantier aurait dû être posé");
        assert!(!under_construction, "le bâtiment devrait être terminé");
        assert_eq!(health.current, health.max);
        // L'ouvrier est resté au contact, sans entrer dans le bâtiment.
        let position = world.get::<SimPosition>(worker).unwrap().current;
        let distance = distance_to_rect(position, obstacle.0);
        assert!((0.5..=0.6 + CONTACT_MARGIN).contains(&distance), "ouvrier à {distance} m du bâtiment");
        assert!(world.get::<ConstructionTask>(worker).is_none(), "la tâche aurait dû se terminer");
        // Le chantier est bien un obstacle de la grille.
        let grid = world.resource::<NavGrid>();
        assert!(grid.is_blocked(IVec2::new(56, 50)));
    }

    #[test]
    fn overlapping_orders_are_refused() {
        let (mut app, worker) = app();
        for x in [55, 56] {
            build(&mut app, IVec2::new(x, 49), worker);
        }
        app.update();
        let world = app.world_mut();
        let count = world.query::<&Building>().iter(world).count();
        assert_eq!(count, 1, "le second chantier chevauchait le premier");
    }

    // --- Style « ouvrier consommé » (les Sylves) -------------------------

    const GROWS: ConstructionStyle = ConstructionStyle::WorkerConsumed { refund_percent: 75 };
    const ABRI_COST: Cost = Cost::new(100, 20);

    fn stock(app: &App) -> Cost {
        app.world().resource::<Players>().get(PlayerId(0)).stock
    }

    fn set_stock(app: &mut App, stock: Cost) {
        app.world_mut().resource_mut::<Players>().get_mut(PlayerId(0)).stock = stock;
    }

    fn notices(app: &App) -> Vec<NoticeKind> {
        app.world().resource::<Messages<Notice>>().iter_current_update_messages().map(|n| n.kind).collect()
    }

    fn run(app: &mut App, seconds: f32) {
        for _ in 0..(seconds * 32.0) as usize {
            app.update();
        }
    }

    fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query_filtered::<(), F>().iter(world).count()
    }

    #[test]
    fn consumed_worker_roots_and_the_site_grows_alone() {
        let (mut app, worker) = app_with(GROWS, ABRI_COST);
        set_stock(&mut app, Cost::new(150, 50));
        build(&mut app, IVec2::new(55, 49), worker);
        app.update();
        // Payé à la pose ; l'emplacement est promis, rien n'est encore posé.
        assert_eq!(stock(&app), Cost::new(50, 30));
        assert!(app.world().get::<PlannedSite>(worker).is_some());
        assert_eq!(count::<With<Building>>(&mut app), 0, "pas de chantier avant l'arrivée");
        // 15 m de marche à 4 m/s : l'ouvrier arrive et disparaît dans le chantier.
        run(&mut app, 5.0);
        assert!(app.world().get_entity(worker).is_err(), "l'ouvrier est consommé");
        assert_eq!(count::<With<Unit>>(&mut app), 0);
        assert_eq!(count::<(With<UnderConstruction>, With<RootedWorker>)>(&mut app), 1);
        // Sans personne pour s'en occuper, il pousse jusqu'au bout (2 s).
        run(&mut app, 2.5);
        let world = app.world_mut();
        let (health, under_construction) = world.query::<(&Health, Has<UnderConstruction>)>().single(world).unwrap();
        assert!(!under_construction, "le bâtiment devrait être terminé");
        assert_eq!(health.current, health.max);
        // Posé au centre de l'emplacement promis : c'est un obstacle.
        assert!(app.world().resource::<NavGrid>().is_blocked(IVec2::new(56, 50)));
        assert_eq!(stock(&app), Cost::new(50, 30), "rien de remboursé");
    }

    #[test]
    fn cancelling_a_growing_site_refunds_part_and_returns_the_worker() {
        let (mut app, worker) = app_with(GROWS, ABRI_COST);
        set_stock(&mut app, ABRI_COST);
        build(&mut app, IVec2::new(55, 49), worker);
        run(&mut app, 5.0);
        let site = {
            let world = app.world_mut();
            world.query_filtered::<Entity, With<UnderConstruction>>().single(world).unwrap()
        };
        app.world_mut().resource_mut::<CommandQueue>().push(PlayerId(0), PlayerCommand::CancelConstruction { building: site });
        app.update();
        assert!(app.world().get_entity(site).is_err(), "le chantier a disparu");
        // 75 % de 100 + 20 (arrondi en dessous).
        assert_eq!(stock(&app), Cost::new(75, 15));
        // L'ouvrier ressort de terre, au bord de l'ancien chantier.
        let world = app.world_mut();
        let position = world.query_filtered::<&SimPosition, With<Unit>>().single(world).expect("l'ouvrier revient").current;
        let rect = app.world().resource::<NavGrid>().frame().rect(IVec2::new(55, 49), UVec2::new(3, 3));
        assert!(distance_to_rect(position, rect) < 1.5, "ouvrier rendu à {position}");
    }

    #[test]
    fn consumed_build_is_refused_without_a_free_worker_or_resources() {
        let (mut app, worker) = app_with(GROWS, ABRI_COST);
        set_stock(&mut app, Cost::new(150, 30));
        // Aucun ouvrier dans l'ordre : refusé, rien de payé.
        app.world_mut()
            .resource_mut::<CommandQueue>()
            .push(PlayerId(0), PlayerCommand::Build { type_index: 0, min_cell: IVec2::new(55, 49), builders: vec![] });
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::NoWorker]);
        assert_eq!(stock(&app), Cost::new(150, 30));
        // Un premier emplacement promis : accepté.
        build(&mut app, IVec2::new(55, 49), worker);
        app.update();
        assert_eq!(stock(&app), Cost::new(50, 10));
        // Le même ouvrier, déjà promis ailleurs : refusé (un seul bâtiment par ouvrier).
        set_stock(&mut app, Cost::new(500, 500));
        build(&mut app, IVec2::new(40, 40), worker);
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::NoWorker]);
        assert_eq!(stock(&app), Cost::new(500, 500));
        // Un ouvrier disparu ne construit rien non plus.
        let gone = app.world_mut().spawn((Unit { type_index: 0 }, Owner(PlayerId(0)), SimPosition::new(Vec2::ZERO))).id();
        app.world_mut().despawn(gone);
        build(&mut app, IVec2::new(40, 40), gone);
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::NoWorker]);
        // Ressources insuffisantes : refusé, l'ouvrier libre n'a rien reçu.
        let other = app
            .world_mut()
            .spawn((
                Unit { type_index: 0 },
                Owner(PlayerId(0)),
                SimPosition::new(Vec2::new(-10.0, 5.0)),
                MoveSpeed(4.0),
                Collider { radius: 0.6, mass: 1.0, layer: CollisionLayer::Ground },
            ))
            .id();
        set_stock(&mut app, Cost::new(99, 50));
        build(&mut app, IVec2::new(40, 40), other);
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::NotEnoughResources]);
        assert!(app.world().get::<PlannedSite>(other).is_none());
        assert_eq!(stock(&app), Cost::new(99, 50));
    }

    #[test]
    fn another_order_cancels_the_promise_and_refunds_it() {
        let (mut app, worker) = app_with(GROWS, ABRI_COST);
        set_stock(&mut app, ABRI_COST);
        build(&mut app, IVec2::new(55, 49), worker);
        run(&mut app, 0.5);
        assert_eq!(stock(&app), Cost::default());
        app.world_mut()
            .resource_mut::<CommandQueue>()
            .push(PlayerId(0), PlayerCommand::Move { units: vec![worker], target: Vec2::new(-20.0, 0.0) });
        run(&mut app, 5.0);
        assert_eq!(stock(&app), ABRI_COST, "remboursé en entier");
        assert!(app.world().get::<PlannedSite>(worker).is_none());
        assert_eq!(count::<With<Building>>(&mut app), 0);
    }

    // --- Sertissage à distance (le Collectif) et champs de construction ---

    const ANCHOR: ConstructionStyle = ConstructionStyle::RemoteAnchor { refund_percent: 75 };
    const RELAY: usize = 2;
    const WORKSHOP: usize = 3;
    const CORE: usize = 4;

    /// Pose directement un bâtiment du joueur `player` (terminé si `progress`
    /// vaut `None`), comme le fait la carte au démarrage.
    fn spawn_building(app: &mut App, player: u8, type_index: usize, min_cell: IVec2, progress: Option<f32>) -> Entity {
        let frame = app.world().resource::<NavGrid>().frame();
        // On sort le catalogue du monde le temps de s'en servir (le monde ne
        // peut pas être emprunté deux fois, en lecture et en écriture).
        let catalog = app.world_mut().remove_resource::<BuildingCatalog>().unwrap();
        let entity = catalog.spawn(&mut app.world_mut().commands(), frame, PlayerId(player), type_index, min_cell, progress);
        app.world_mut().flush();
        app.insert_resource(catalog);
        entity
    }

    /// Fait de l'ouvrier une unité volante (couche de collision de l'air).
    fn make_flying(app: &mut App, worker: Entity) {
        app.world_mut().get_mut::<Collider>(worker).unwrap().layer = CollisionLayer::Air;
    }

    #[test]
    fn anchoring_worker_leaves_and_the_site_assembles_alone() {
        let (mut app, worker) = app_with(ANCHOR, ABRI_COST);
        make_flying(&mut app, worker);
        set_stock(&mut app, Cost::new(150, 50));
        // L'ouvrier récolte un gisement à côté de lui (x de −14 à −12).
        let frame = app.world().resource::<NavGrid>().frame();
        let patch = crate::economy::spawn_resource_node(
            &mut app.world_mut().commands(),
            frame,
            ResourceKind::Primary,
            IVec2::new(36, 52),
            UVec2::new(2, 1),
            500,
            false,
        );
        app.world_mut().flush();
        app.world_mut().resource_mut::<CommandQueue>().push(PlayerId(0), PlayerCommand::Harvest { units: vec![worker], node: patch });
        run(&mut app, 1.0);
        assert!(app.world().get::<Harvesting>(worker).is_some());
        // Envoyé sertir un abri à 17 m : payé, emplacement promis, récolte retenue.
        build(&mut app, IVec2::new(55, 49), worker);
        app.update();
        assert_eq!(stock(&app), Cost::new(50, 30));
        assert!(app.world().get::<PlannedSite>(worker).is_some());
        assert_eq!(app.world().get::<ResumeHarvest>(worker).map(|r| r.0.node), Some(patch));
        assert_eq!(count::<With<Building>>(&mut app), 0, "pas de chantier avant l'arrivée");
        // Arrivé, il pose la gemme : le chantier apparaît, et il repart récolter.
        run(&mut app, 5.5);
        assert_eq!(count::<With<UnderConstruction>>(&mut app), 1, "le chantier est ancré");
        assert!(app.world().get_entity(worker).is_ok(), "l'ouvrier n'est pas consommé");
        assert!(app.world().get::<PlannedSite>(worker).is_none());
        assert_eq!(app.world().get::<Harvesting>(worker).map(|h| h.node), Some(patch), "il reprend sa récolte");
        assert_eq!(count::<With<RootedWorker>>(&mut app), 0);
        // Le chantier s'assemble seul, l'ouvrier loin de lui (2 s de construction).
        run(&mut app, 2.5);
        let world = app.world_mut();
        let (health, under_construction, obstacle) =
            world.query::<(&Health, Has<UnderConstruction>, &StaticObstacle)>().single(world).unwrap();
        assert!(!under_construction, "le bâtiment devrait être terminé");
        assert_eq!(health.current, health.max);
        let rect = obstacle.0;
        let position = app.world().get::<SimPosition>(worker).unwrap().current;
        assert!(distance_to_rect(position, rect) > 3.0, "l'ouvrier est reparti ({position})");
    }

    #[test]
    fn anchoring_worker_without_a_harvest_goes_to_the_nearest_patch() {
        let (mut app, worker) = app_with(ANCHOR, Cost::default());
        make_flying(&mut app, worker);
        let frame = app.world().resource::<NavGrid>().frame();
        let patch = crate::economy::spawn_resource_node(
            &mut app.world_mut().commands(),
            frame,
            ResourceKind::Primary,
            IVec2::new(60, 52),
            UVec2::new(2, 1),
            500,
            false,
        );
        app.world_mut().flush();
        build(&mut app, IVec2::new(55, 49), worker);
        run(&mut app, 5.5);
        assert_eq!(count::<With<UnderConstruction>>(&mut app), 1);
        assert_eq!(app.world().get::<Harvesting>(worker).map(|h| h.node), Some(patch));
    }

    #[test]
    fn workshop_is_refused_outside_any_active_field() {
        let (mut app, worker) = app_with(ANCHOR, Cost::default());
        // Aucun champ : refusé, avec le bon message.
        build_type(&mut app, WORKSHOP, IVec2::new(55, 49), worker);
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::OutsideField]);
        assert!(app.world().get::<PlannedSite>(worker).is_none());
        // Un relais encore en chantier n'émet rien.
        spawn_building(&mut app, 0, RELAY, IVec2::new(60, 49), Some(0.5));
        build_type(&mut app, WORKSHOP, IVec2::new(55, 49), worker);
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::OutsideField]);
        // Un relais d'un autre joueur non plus.
        spawn_building(&mut app, 1, RELAY, IVec2::new(60, 46), None);
        build_type(&mut app, WORKSHOP, IVec2::new(55, 49), worker);
        app.update();
        assert_eq!(notices(&app), vec![NoticeKind::OutsideField]);
        // Le relais terminé du joueur, à 4,5 m : accepté.
        spawn_building(&mut app, 0, RELAY, IVec2::new(60, 52), None);
        build_type(&mut app, WORKSHOP, IVec2::new(55, 49), worker);
        app.update();
        assert!(notices(&app).is_empty(), "refusé : {:?}", notices(&app));
        assert!(app.world().get::<PlannedSite>(worker).is_some());
        // Un relais, lui, se pose n'importe où.
        let other = app
            .world_mut()
            .spawn((
                Unit { type_index: 0 },
                Owner(PlayerId(0)),
                SimPosition::new(Vec2::new(-10.0, 5.0)),
                MoveSpeed(4.0),
                Collider { radius: 0.6, mass: 1.0, layer: CollisionLayer::Air },
            ))
            .id();
        build_type(&mut app, RELAY, IVec2::new(20, 20), other);
        app.update();
        assert!(app.world().get::<PlannedSite>(other).is_some());
    }

    #[test]
    fn core_and_relay_fields() {
        let (mut app, _) = app_with(ANCHOR, Cost::default());
        // Un cœur de 5 × 5 centré en (2,5 ; 2,5), un relais de 2 × 2 centré en (21 ; 1).
        spawn_building(&mut app, 0, CORE, IVec2::new(50, 50), None);
        spawn_building(&mut app, 0, RELAY, IVec2::new(70, 50), None);
        spawn_building(&mut app, 0, RELAY, IVec2::new(30, 30), Some(0.3));
        spawn_building(&mut app, 1, CORE, IVec2::new(80, 80), None);
        let frame = app.world().resource::<NavGrid>().frame();
        let catalog = app.world_mut().remove_resource::<BuildingCatalog>().unwrap();
        let world = app.world_mut();
        let mut query = world.query::<(&Building, &Owner, &Footprint, Has<UnderConstruction>)>();
        let fields = ConstructionFields::of_player(&catalog, frame, PlayerId(0), query.iter(world));
        assert_eq!(fields.circles.len(), 2, "le chantier et le cœur adverse n'émettent rien pour moi");
        assert!(fields.covers(Vec2::new(2.5, 10.4)), "dans le champ du cœur (8 m)");
        assert!(!fields.covers(Vec2::new(2.5, 10.6)));
        assert!(fields.covers(Vec2::new(26.9, 1.0)), "dans le champ du relais (6 m)");
        assert!(!fields.covers(Vec2::new(27.1, 1.0)));
        assert!(!fields.covers(Vec2::new(-19.0, -19.0)), "le relais en chantier n'émet pas");
        // La même question, posée comme le fera le combat (relais détruit ?).
        let point = Vec2::new(32.0, 32.0);
        assert!(field::in_active_field(&catalog, frame, PlayerId(1), point, query.iter(world)));
        assert!(!field::in_active_field(&catalog, frame, PlayerId(0), point, query.iter(world)));
    }

    #[test]
    fn cancelling_an_anchored_site_refunds_part_and_keeps_the_worker() {
        let (mut app, worker) = app_with(ANCHOR, ABRI_COST);
        make_flying(&mut app, worker);
        set_stock(&mut app, ABRI_COST);
        build(&mut app, IVec2::new(55, 49), worker);
        run(&mut app, 5.5);
        let site = {
            let world = app.world_mut();
            world.query_filtered::<Entity, With<UnderConstruction>>().single(world).unwrap()
        };
        app.world_mut().resource_mut::<CommandQueue>().push(PlayerId(0), PlayerCommand::CancelConstruction { building: site });
        app.update();
        assert!(app.world().get_entity(site).is_err(), "le chantier a disparu");
        // 75 % de 100 + 20 (arrondi en dessous) ; l'ouvrier n'a rien perdu,
        // aucun autre ne sort du chantier.
        assert_eq!(stock(&app), Cost::new(75, 15));
        assert!(app.world().get_entity(worker).is_ok());
        assert_eq!(count::<With<Unit>>(&mut app), 1);
    }
}
