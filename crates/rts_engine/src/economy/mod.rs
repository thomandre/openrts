//! Économie : les deux ressources, les gisements, les stocks des joueurs,
//! les coûts.
//!
//! Le moteur connaît deux sortes de ressources ([`ResourceKind`]) : une
//! principale, abondante, et une rare. Leurs noms, leurs couleurs et les
//! réglages (charge d'un ouvrier, temps d'extraction…) viennent du jeu.
//!
//! Sur la carte, les ressources sont dans des **gisements**
//! ([`ResourceNode`]) : des obstacles fixes qui contiennent une quantité
//! finie. Ceux de ressource rare (les « sources ») ne se récoltent qu'une
//! fois couverts par un bâtiment d'extraction terminé (voir
//! [`crate::buildings::PlacementRule::OnResource`]).
//!
//! Tout ce qui change l'état du jeu (stocks, gisements) se fait dans la
//! simulation à pas fixe ; les messages à l'écran ([`Notice`]) et les
//! visuels ne font que refléter cet état.

pub mod harvest;
pub mod production;
pub mod ui;

use std::ops::{Add, AddAssign};

use bevy::prelude::*;

use crate::buildings::Footprint;
use crate::collision::StaticObstacle;
use crate::navigation::GridFrame;
use crate::players::PlayerId;
use crate::simulation::SimSet;

/// Les deux ressources du jeu (voir GAME_DESIGN.md, « Économie et ressources »).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResourceKind {
    /// Ressource principale, abondante : le gros des unités et des bâtiments.
    Primary,
    /// Ressource rare, limitée : technologie avancée, unités d'élite.
    Rare,
}

impl ResourceKind {
    pub const ALL: [ResourceKind; 2] = [ResourceKind::Primary, ResourceKind::Rare];

    /// Position dans les tableaux à deux cases (`[principale, rare]`).
    pub fn index(self) -> usize {
        match self {
            ResourceKind::Primary => 0,
            ResourceKind::Rare => 1,
        }
    }
}

/// Une quantité de chacune des deux ressources : un coût, un remboursement…
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cost {
    /// Ressource principale (abondante).
    pub primary: u32,
    /// Ressource rare.
    pub rare: u32,
}

/// Un stock de ressources : la même chose qu'un coût. `type` donne
/// simplement un second nom à un type existant, pour la lisibilité.
pub type Stock = Cost;

impl Cost {
    pub const fn new(primary: u32, rare: u32) -> Self {
        Self { primary, rare }
    }

    /// La quantité d'une ressource.
    pub fn get(&self, kind: ResourceKind) -> u32 {
        match kind {
            ResourceKind::Primary => self.primary,
            ResourceKind::Rare => self.rare,
        }
    }

    /// Ajoute `amount` d'une ressource.
    pub fn deposit(&mut self, kind: ResourceKind, amount: u32) {
        match kind {
            ResourceKind::Primary => self.primary += amount,
            ResourceKind::Rare => self.rare += amount,
        }
    }

    /// Ce stock suffit-il à payer `cost` ?
    pub fn covers(&self, cost: Cost) -> bool {
        self.primary >= cost.primary && self.rare >= cost.rare
    }

    /// `percent` % de ce coût, arrondi à l'unité inférieure (remboursements).
    pub fn percent(self, percent: u32) -> Cost {
        Cost::new(self.primary * percent / 100, self.rare * percent / 100)
    }

    /// Paie `cost` si le stock suffit (et renvoie vrai) ; sinon ne touche à rien.
    pub fn try_pay(&mut self, cost: Cost) -> bool {
        if !self.covers(cost) {
            return false;
        }
        self.primary -= cost.primary;
        self.rare -= cost.rare;
        true
    }
}

// Implémenter les traits `Add` et `AddAssign` permet d'écrire `a + b` et
// `a += b` avec nos propres types (« surcharge d'opérateurs »).
impl Add for Cost {
    type Output = Cost;
    fn add(self, other: Cost) -> Cost {
        Cost::new(self.primary + other.primary, self.rare + other.rare)
    }
}

impl AddAssign for Cost {
    fn add_assign(&mut self, other: Cost) {
        *self = *self + other;
    }
}

/// Nom et couleur d'une ressource (données du jeu).
#[derive(Debug, Clone)]
pub struct ResourceInfo {
    pub name: String,
    /// Couleur lumineuse : charge portée par les ouvriers, compteurs.
    pub color: Color,
}

/// Les règles économiques d'une faction, fournies par le jeu.
#[derive(Resource, Debug, Clone)]
pub struct EconomyRules {
    /// Les deux ressources, dans l'ordre de [`ResourceKind::index`].
    pub resources: [ResourceInfo; 2],
    /// Types d'unités (indices du catalogue d'unités) qui récoltent.
    pub harvesters: Vec<usize>,
    /// Charge rapportée à chaque voyage, par ressource.
    pub carry: [u32; 2],
    /// Temps d'extraction d'une charge, en secondes, par ressource.
    pub gather_time: [f32; 2],
    /// Coût, temps de production et population de chaque type d'unité
    /// (dans l'ordre du catalogue d'unités).
    pub units: Vec<production::UnitEconomy>,
}

impl EconomyRules {
    /// Les données économiques d'un type d'unité (tout à zéro s'il en manque).
    pub fn unit(&self, unit_type: usize) -> production::UnitEconomy {
        self.units.get(unit_type).copied().unwrap_or_default()
    }

    pub fn resource(&self, kind: ResourceKind) -> &ResourceInfo {
        &self.resources[kind.index()]
    }

    /// Ajoute les règles d'une autre faction, dont les types d'unités
    /// commencent à l'indice `first_unit` du catalogue commun (voir
    /// `UnitCatalog::add_faction`). Ressources, charges et cadences de
    /// récolte restent celles-ci : elles sont communes à la partie.
    pub fn merge(&mut self, other: EconomyRules, first_unit: usize) {
        self.units.resize(first_unit, production::UnitEconomy::default());
        self.units.extend(other.units);
        self.harvesters.extend(other.harvesters.into_iter().map(|h| h + first_unit));
    }
}

/// Un gisement : une quantité finie d'une ressource, posée sur un bloc de
/// cases de la grille (c'est un obstacle fixe, que les unités contournent).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ResourceNode {
    pub kind: ResourceKind,
    /// Ce qu'il reste à extraire.
    pub amount: u32,
    /// Quantité de départ (pour savoir à quel point il est entamé).
    pub initial: u32,
    /// Vrai pour une source qu'il faut couvrir d'un bâtiment d'extraction
    /// avant de la récolter (ressource rare).
    pub needs_extractor: bool,
    /// Le bâtiment d'extraction posé dessus, s'il y en a un.
    pub extractor: Option<Entity>,
    /// L'ouvrier qui extrait en ce moment (un seul à la fois : les autres
    /// attendent leur tour, voir [`harvest`]).
    pub occupant: Option<Entity>,
}

impl ResourceNode {
    /// Part restante, de 0 (épuisé) à 1 (intact).
    pub fn fraction(&self) -> f32 {
        if self.initial == 0 { 0.0 } else { self.amount as f32 / self.initial as f32 }
    }
}

/// Fait apparaître un gisement de `amount` unités de `kind` sur le bloc de
/// `size` cases dont le coin est `min_cell`. L'entité est placée au centre
/// du bloc, au niveau du sol : le jeu y accroche ses visuels (en enfants),
/// en marquant d'un [`DepletingVisual`] ceux qui rétrécissent quand le
/// gisement s'épuise.
pub fn spawn_resource_node(
    commands: &mut Commands,
    frame: GridFrame,
    kind: ResourceKind,
    min_cell: IVec2,
    size: UVec2,
    amount: u32,
    needs_extractor: bool,
) -> Entity {
    let rect = frame.rect(min_cell, size);
    let center = rect.center();
    commands
        .spawn((
            Name::new("Gisement"),
            ResourceNode { kind, amount, initial: amount, needs_extractor, extractor: None, occupant: None },
            Footprint { min_cell, size },
            StaticObstacle(rect),
            Transform::from_xyz(center.x, 0.0, center.y),
            Visibility::default(),
        ))
        .id()
}

/// Marqueur d'un visuel de gisement qui rétrécit à mesure qu'on le vide
/// (enfant direct de l'entité du gisement).
#[derive(Component, Debug, Clone, Copy)]
pub struct DepletingVisual {
    /// Échelle (largeur, hauteur) quand le gisement est presque vide ;
    /// 1 quand il est plein.
    pub min_scale: Vec2,
}

impl Default for DepletingVisual {
    fn default() -> Self {
        Self { min_scale: Vec2::new(0.55, 0.3) }
    }
}

/// Un court message pour le joueur, quand la simulation refuse un ordre
/// (ou que l'interface le refuse d'avance).
///
/// `#[derive(Message)]` : un « message » Bevy, que des systèmes écrivent
/// (`MessageWriter`) et que d'autres lisent (`MessageReader`) pendant
/// l'image suivante au plus tard.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notice {
    /// Le joueur concerné : l'interface n'affiche que ceux du joueur local.
    pub player: PlayerId,
    pub kind: NoticeKind,
}

/// Les raisons d'un refus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    NotEnoughResources,
    PopulationCap,
    MustBeOnSource,
    QueueFull,
    MissingRequirement,
    Blocked,
    /// Aucun ouvrier libre parmi la sélection (style « ouvrier consommé » :
    /// chacun ne fait qu'un bâtiment).
    NoWorker,
    /// Le bâtiment doit être posé dans un champ de construction du joueur
    /// (voir `buildings::field`).
    OutsideField,
}

impl NoticeKind {
    /// Le texte affiché.
    pub fn text(self) -> &'static str {
        match self {
            NoticeKind::NotEnoughResources => "Ressources insuffisantes",
            NoticeKind::PopulationCap => "Population maximale",
            NoticeKind::MustBeOnSource => "Doit être posé sur une source",
            NoticeKind::QueueFull => "File de production pleine",
            NoticeKind::MissingRequirement => "Prérequis manquant",
            NoticeKind::Blocked => "Emplacement occupé",
            NoticeKind::NoWorker => "Aucun ouvrier libre",
            NoticeKind::OutsideField => "Hors champ : à poser près d'un relais ou du cœur de base",
        }
    }
}

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        // Ordre dans le pas : déplacements, ordres de récolte, poses de
        // bâtiments (un ouvrier envoyé construire quitte sa récolte), puis
        // le cycle de récolte lui-même.
        app.add_message::<Notice>()
            .init_resource::<production::PopulationSettings>()
            .add_systems(
                FixedUpdate,
                (
                    release_lost_extractors.before(harvest::harvest),
                    harvest::apply_harvest_orders
                        .after(crate::orders::apply_unit_orders)
                        .before(crate::buildings::apply_build_orders),
                    harvest::harvest.after(crate::buildings::apply_build_orders),
                    // La population est comptée avant les ordres de production.
                    (production::count_population, production::apply_production_orders)
                        .chain()
                        .after(crate::buildings::apply_build_orders),
                )
                    .in_set(SimSet::Orders),
            )
            .add_systems(FixedUpdate, production::progress_production.in_set(SimSet::Construction))
            .add_systems(Update, (shrink_nodes, harvest::show_carried_loads));

    }
}

/// L'interface de l'économie (compteurs, panneaux, messages, repère de
/// ralliement) : séparée de [`EconomyPlugin`] pour que les tests de la
/// simulation tournent sans rendu.
pub struct EconomyUiPlugin;

impl Plugin for EconomyUiPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(
                Update,
                (
                    ui::handle_production_actions,
                    ui::spawn_economy_ui,
                    ui::update_counters,
                    ui::update_production_panel,
                    ui::update_rally_marker,
                    ui::update_site_panel,
                    ui::show_notices,
                )
                    .chain()
                    .in_set(crate::RtsSystems::Actions),
            );
    }
}

/// Une source dont le bâtiment d'extraction a disparu (chantier annulé,
/// bâtiment détruit) redevient libre.
fn release_lost_extractors(mut nodes: Query<&mut ResourceNode>, buildings: Query<()>) {
    for mut node in &mut nodes {
        if let Some(extractor) = node.extractor
            && !buildings.contains(extractor)
        {
            node.extractor = None;
        }
    }
}

/// Affichage : les visuels d'un gisement rétrécissent à mesure qu'il se vide.
fn shrink_nodes(
    nodes: Query<(&ResourceNode, &Children), Changed<ResourceNode>>,
    mut visuals: Query<(&DepletingVisual, &mut Transform)>,
) {
    for (node, children) in &nodes {
        let fraction = node.fraction();
        for child in children.iter() {
            if let Ok((visual, mut transform)) = visuals.get_mut(child) {
                let scale = visual.min_scale.lerp(Vec2::ONE, fraction);
                transform.scale = Vec3::new(scale.x, scale.y, scale.x);
            }
        }
    }
}
