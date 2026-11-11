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
//! La construction suit un [`ConstructionStyle`], propre à chaque faction.
//! Pour l'instant un seul : l'ouvrier reste sur le chantier (Coalition). Le
//! chantier est posé tout de suite (obstacle, points de vie qui montent) et
//! n'avance que tant qu'un ouvrier le touche. D'autres factions pourront
//! construire autrement (bâtiment qui pousse seul, ouvrier qui se transforme…) :
//! c'est l'asymétrie voulue par le design.
//!
//! Tout ce qui change l'état du jeu (pose effective, progression, obstacles)
//! se fait dans la simulation à pas fixe ; le module [`placement`] (mode pose,
//! fantôme) et le module [`ui`] (barre de boutons, barres de progression,
//! bâtiment qui monte du sol) ne font que de l'affichage et des demandes.

pub mod placement;
pub mod ui;

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
use crate::economy::{Notice, NoticeKind, ResourceKind, ResourceNode};

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
}

/// Façon de construire d'une faction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionStyle {
    /// L'ouvrier se rend sur le chantier et y reste jusqu'à la fin : la
    /// construction n'avance que tant qu'il le touche.
    WorkerStays,
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
            site_material: units.material(MaterialSlot::Hazard),
            pole_mesh: meshes.add(Cylinder::new(0.07, 1.0)),
            pole_material: units.material(MaterialSlot::Dark),
            ghost_materials: [
                materials.add(ghost(Color::srgba(0.25, 1.0, 0.45, 0.38))),
                materials.add(ghost(Color::srgba(1.0, 0.18, 0.12, 0.42))),
            ],
        }
    }

    /// Les prérequis du type `type_index` sont-ils remplis ? `completed[i]`
    /// dit si le joueur possède un bâtiment terminé du type `i`.
    pub fn requirements_met(&self, type_index: usize, completed: &[bool]) -> bool {
        self.types[type_index].requires.iter().all(|&r| completed.get(r).copied().unwrap_or(false))
    }

    /// Le type `type_index` peut-il se poser sur le bloc de cases dont le coin
    /// est `min_cell` ? Vérifie le terrain (ou la source, pour un bâtiment
    /// d'extraction) et les prérequis (`completed`, voir [`completed_types`]).
    /// En cas de succès, renvoie la source couverte, s'il y en a une.
    pub fn check_placement<'a>(
        &self,
        type_index: usize,
        min_cell: IVec2,
        grid: &NavGrid,
        completed: &[bool],
        nodes: impl IntoIterator<Item = (Entity, &'a ResourceNode, &'a Footprint)>,
    ) -> Result<Option<Entity>, NoticeKind> {
        let building = &self.types[type_index];
        if !self.requirements_met(type_index, completed) {
            return Err(NoticeKind::MissingRequirement);
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
        entity.with_children(|parent| {
            parent.spawn((
                SelectionRing,
                Mesh3d(self.outlines[type_index].clone()),
                MeshMaterial3d(self.ring_material.clone()),
                Transform::from_xyz(0.0, -height + 0.04, 0.0),
                NotShadowCaster,
                Visibility::Hidden,
            ));
            if progress.is_some() {
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
                    BuildingBody { center_height: height, top: visual.top },
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

/// Le corps d'un bâtiment (ses pièces), qui monte du sol pendant la construction.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct BuildingBody {
    center_height: f32,
    top: f32,
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
const CONTACT_MARGIN: f32 = 0.35;

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<placement::Placement>()
            .add_systems(
                FixedUpdate,
                (
                    // Après les déplacements : un ordre de pose donné juste
                    // après un ordre de marche l'emporte.
                    (apply_build_orders, drive_builders)
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
/// prérequis du joueur, crée le chantier et le confie à l'ouvrier le plus proche.
fn apply_build_orders(
    mut commands: Commands,
    tick: Res<TickCommands>,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    buildings: Query<(&Building, &Owner, Has<UnderConstruction>)>,
    mut builders: Query<(Entity, &Owner, &SimPosition, Option<&mut ConstructionTask>), With<Unit>>,
    mut nodes: Query<(Entity, &mut ResourceNode, &Footprint)>,
    mut notices: MessageWriter<Notice>,
) {
    // Pas encore de grille (tout premier pas) ou pas de bâtiments dans ce jeu.
    let (Some(grid), Some(catalog)) = (grid, catalog) else { return };
    // Chantiers posés pendant ce pas : la grille ne les connaîtra qu'au
    // prochain, il faut donc aussi vérifier qu'ils ne se chevauchent pas.
    let mut placed: Vec<Rect> = Vec::new();
    // Ouvriers qui ont reçu un chantier pendant ce pas (pour l'équité).
    let mut assigned: Vec<(Entity, Vec<Entity>)> = Vec::new();

    for issued in &tick.0 {
        let PlayerCommand::Build { type_index, min_cell, builders: ordered } = &issued.command else { continue };
        let (type_index, min_cell, player) = (*type_index, *min_cell, issued.player);
        let Some(building) = catalog.types.get(type_index) else { continue };
        let completed = completed_types(&catalog, player, buildings.iter());
        let rect = grid.frame().rect(min_cell, building.footprint);
        let overlaps = placed.iter().any(|p| !p.intersect(rect).is_empty());
        let check = catalog.check_placement(type_index, min_cell, &grid, &completed, nodes.iter());
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
            .map(|(entity, _, position, task)| {
                let busy = task.is_some_and(|t| !t.sites.is_empty()) || assigned.iter().any(|(b, _)| *b == entity);
                (busy, position.current.distance(rect.center()), entity)
            })
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        let Some(&(_, _, builder)) = candidates.first() else { continue };

        let site = catalog.spawn(&mut commands, grid.frame(), player, type_index, min_cell, Some(0.0));
        placed.push(rect);
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
        let Ok((_, _, _, task)) = builders.get_mut(builder) else { continue };
        match task {
            Some(mut task) => task.sites.extend(sites),
            None => {
                commands.entity(builder).insert(ConstructionTask { sites });
            }
        }
    }
}

/// Distance d'un point à un rectangle (0 à l'intérieur).
fn distance_to_rect(point: Vec2, rect: Rect) -> f32 {
    point.distance(point.clamp(rect.min, rect.max))
}

/// Point où un ouvrier de rayon `radius` doit se tenir pour toucher le
/// chantier `rect` : le point praticable du pourtour (agrandi de son rayon)
/// le plus proche de lui.
fn approach_point(grid: &NavGrid, position: Vec2, radius: f32, rect: Rect) -> Vec2 {
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
    (0..steps)
        .map(|i| point_at(i as f32 * perimeter / steps as f32))
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

/// Fait avancer les chantiers dont l'ouvrier est au contact.
fn progress_construction(
    mut commands: Commands,
    time: Res<Time>,
    catalog: Option<Res<BuildingCatalog>>,
    mut sites: Query<(Entity, &Building, &StaticObstacle, &mut UnderConstruction, &mut Health)>,
    builders: Query<(&SimPosition, &Collider, &ConstructionTask)>,
) {
    let Some(catalog) = catalog else { return };
    // Chantiers sur lesquels un ouvrier travaille : son premier chantier, au contact.
    let mut worked: Vec<Entity> = builders
        .iter()
        .filter_map(|(position, collider, task)| {
            let site = *task.sites.first()?;
            let (_, _, obstacle, ..) = sites.get(site).ok()?;
            (distance_to_rect(position.current, obstacle.0) <= collider.radius + CONTACT_MARGIN).then_some(site)
        })
        .collect();
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
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<GameAction>()
            .init_resource::<InputContext>()
            .init_resource::<crate::cursor::Pointer>()
            .add_plugins((SimulationPlugin, PlayersPlugin, OrdersPlugin, EconomyPlugin, BuildingsPlugin))
            // Chaque `update` avance le temps d'exactement un pas de simulation.
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 32.0)));
        let (mut meshes, mut materials, mut images) = (Assets::default(), Assets::default(), Assets::default());
        let units = UnitCatalog::new(Vec::new(), &palette(), &mut meshes, &mut materials, &mut images);
        let building = BuildingType {
            name: "Abri".into(),
            footprint: UVec2::new(3, 3),
            max_health: 100.0,
            build_time: 2.0,
            cost: Cost::default(),
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
        let catalog =
            BuildingCatalog::new(vec![building, extractor], vec![0], ConstructionStyle::WorkerStays, &units, &mut meshes, &mut materials);
        app.insert_resource(catalog);
        let worker = app
            .world_mut()
            .spawn((
                Unit { type_index: 0 },
                Owner(PlayerId(0)),
                SimPosition::new(Vec2::new(-10.0, 0.0)),
                MoveSpeed(4.0),
                Collider { radius: 0.6, mass: 1.0, layer: CollisionLayer::Ground },
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
}
