//! Économie : les deux ressources, les stocks des joueurs, les coûts.
//!
//! Le moteur connaît deux sortes de ressources ([`ResourceKind`]) : une
//! principale, abondante, et une rare. Leurs noms, couleurs et réglages
//! viennent du jeu.

use std::ops::{Add, AddAssign};

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
