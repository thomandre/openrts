//! Collisions : les unités ne se traversent plus, ni ne traversent les bâtiments.
//!
//! Chaque unité est un cercle au sol ([`Collider`]) ; chaque obstacle fixe
//! (bâtiment, rocher) est un rectangle ([`StaticObstacle`]). À chaque pas de
//! simulation, [`step`] :
//!
//! 1. fait avancer les unités qui ont une cible, en ligne droite vers leur
//!    prochain point de passage (calculé par le module `navigation`) ou vers
//!    leur cible ;
//! 2. sépare les cercles qui se chevauchent, en quelques passes. Une unité en
//!    marche pousse les unités immobiles hors de son chemin, sauf si elles sont
//!    plus lourdes qu'elle : elle glisse alors sur le côté pour les contourner.
//!    Les unités au sol sont ensuite repoussées hors des obstacles fixes
//!    (cercle contre rectangle), qui eux ne bougent jamais ;
//! 3. décide quelles unités s'arrêtent : arrivées à leur cible, au contact
//!    d'une unité déjà arrivée au même endroit (arrivée en groupe), ou
//!    bloquées depuis trop longtemps.
//!
//! Deux **couches** de collision ([`CollisionLayer`]) : les unités au sol et
//! les unités volantes ne se gênent pas. Les volantes se bousculent entre
//! elles mais passent au-dessus des unités au sol et des obstacles.
//!
//! Pour trouver vite les voisins d'une unité, on range les unités dans une
//! grille ([`SpatialGrid`]) : on ne compare chaque unité qu'à celles des cases
//! voisines, au lieu de toutes les autres (600 unités : quelques milliers de
//! tests au lieu de 180 000).
//!
//! Déterminisme (futur multijoueur en lockstep) : tout ici est fait de
//! fonctions pures, sans aléatoire ni table de hachage, et les poussées sont
//! d'abord toutes calculées puis appliquées ensemble, pour ne pas dépendre de
//! l'ordre de parcours. Limite connue : les calculs en `f32` peuvent donner des
//! résultats différents d'un processeur ou d'un compilateur à l'autre ; le
//! lockstep entre plateformes demandera plus tard des nombres à virgule fixe ou
//! des réglages de compilation stricts.

use bevy::prelude::*;

/// Empreinte d'une unité au sol, pour les collisions.
///
/// `#[require(Steering)]` : Bevy ajoute automatiquement un [`Steering`] par
/// défaut à toute entité qui reçoit un `Collider` (composant « requis »).
#[derive(Component, Debug, Clone, Copy)]
#[require(Steering)]
pub struct Collider {
    /// Rayon du cercle au sol, en mètres.
    pub radius: f32,
    /// Masse relative (1 pour un fantassin) : une unité ne peut pas bousculer
    /// une unité immobile plus lourde qu'elle, et cède le passage aux plus lourdes.
    pub mass: f32,
    /// Au sol ou en l'air.
    pub layer: CollisionLayer,
}

/// Couche de collision : deux unités ne se poussent que si elles sont dans la
/// même couche, et seules les unités au sol butent contre les obstacles fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub enum CollisionLayer {
    #[default]
    Ground,
    Air,
}

/// Un obstacle fixe au sol (bâtiment, rocher) : un rectangle aligné sur les
/// axes, en mètres (x, z). Les unités au sol ne peuvent pas y entrer, et la
/// grille de navigation (module `navigation`) le contourne.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct StaticObstacle(pub Rect);

/// État de déplacement propre aux collisions, mis à jour à chaque pas.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Steering {
    /// Cible du dernier ordre terminé : sert à reconnaître les unités du même
    /// groupe déjà arrivées, pour s'arrêter à leur contact.
    pub arrived_at: Option<Vec2>,
    /// Plus petite distance à la cible atteinte depuis le début de l'ordre.
    pub best_distance: f32,
    /// Nombre de pas sans progrès notable vers la cible.
    pub stalled_ticks: u32,
}

impl Default for Steering {
    fn default() -> Self {
        Self {
            arrived_at: None,
            best_distance: f32::INFINITY,
            stalled_ticks: 0,
        }
    }
}

impl Steering {
    /// Remet à zéro le suivi de progression (à chaque nouvel ordre).
    pub fn start_order(&mut self) {
        *self = Self::default();
    }
}

/// Limites de la carte (x, z) : les unités ne peuvent pas en sortir.
#[derive(Resource, Debug, Clone, Copy)]
pub struct MapBounds(pub Rect);

impl Default for MapBounds {
    /// 100 m × 100 m centrés sur l'origine. Le jeu peut remplacer cette
    /// ressource avec `insert_resource` pour une autre carte.
    fn default() -> Self {
        Self(Rect::new(-50.0, -50.0, 50.0, 50.0))
    }
}

/// Réglages des collisions.
#[derive(Resource, Debug, Clone)]
pub struct CollisionSettings {
    /// Nombre de passes de séparation par pas de simulation.
    pub iterations: usize,
    /// Fraction du chevauchement corrigée à chaque passe (entre 0 et 1).
    /// Moins de 1 évite que les poussées de plusieurs voisins s'additionnent
    /// et fassent osciller les groupes serrés.
    pub relaxation: f32,
    /// Chevauchement toléré (mètres), en dessous duquel on ne pousse plus :
    /// évite les micro-tremblements des unités au repos.
    pub slop: f32,
    /// Part de la correction subie par l'unité immobile quand une unité en
    /// marche la bouscule (0,5 : partage égal ; 1 : l'unité en marche passe sans ralentir).
    pub idle_push_share: f32,
    /// Distance en plus des rayons sous laquelle deux unités sont « au contact ».
    pub contact_margin: f32,
    /// Progrès minimal (mètres) vers la cible pour remettre à zéro le compteur de blocage.
    pub progress_epsilon: f32,
    /// Nombre de pas sans progrès après lequel une unité abandonne son ordre.
    pub stall_ticks: u32,
    /// Correction maximale (mètres, par passe) pour sortir une unité d'un
    /// obstacle fixe : une unité sur laquelle on pose un bâtiment est poussée
    /// dehors en quelques pas au lieu d'être téléportée.
    pub obstacle_push_max: f32,
    /// Loin de sa cible, une unité bloquée insiste `give_up_factor` fois
    /// plus longtemps avant d'abandonner son ordre.
    pub give_up_factor: u32,
    /// Densité supposée d'un groupe tassé (part du sol couverte par les
    /// unités, entre 0 et 1). Plus elle est faible, plus les unités acceptent
    /// de s'arrêter loin du point visé.
    pub crowd_packing: f32,
    /// Force du glissement sur le côté d'une unité qui bute contre un obstacle
    /// (en part de la correction de chevauchement ; 0 : pas de contournement).
    pub sidestep: f32,
}

impl Default for CollisionSettings {
    fn default() -> Self {
        Self {
            iterations: 3,
            relaxation: 0.5,
            slop: 0.01,
            idle_push_share: 0.8,
            contact_margin: 0.05,
            progress_epsilon: 0.1,
            // Une seconde à 32 pas par seconde.
            stall_ticks: 32,
            obstacle_push_max: 0.25,
            give_up_factor: 2,
            crowd_packing: 0.7,
            sidestep: 1.0,
        }
    }
}

/// Une unité vue par la simulation des collisions : des données simples,
/// sans ECS, pour pouvoir tester sans fenêtre ni `App`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Agent {
    pub position: Vec2,
    pub radius: f32,
    /// Masse relative (voir [`Collider::mass`]).
    pub mass: f32,
    /// Vitesse en mètres par seconde.
    pub speed: f32,
    /// Destination finale, ou `None` si l'unité est immobile.
    pub target: Option<Vec2>,
    /// Prochain point de passage du chemin (module `navigation`), ou `None`
    /// pour aller droit vers `target` (dernier tronçon, unités volantes).
    pub waypoint: Option<Vec2>,
    /// Longueur du chemin restant **après** `waypoint` (0 sans point de
    /// passage) : sert à mesurer le progrès le long d'un détour.
    pub remaining: f32,
    /// En attente d'un chemin : l'unité ne bouge pas encore, mais compte comme
    /// « en marche » et n'abandonne pas son ordre.
    pub hold: bool,
    pub layer: CollisionLayer,
    pub steering: Steering,
}

impl Agent {
    /// Termine l'ordre en cours : l'unité devient immobile. `arrived` est vrai
    /// si elle est arrivée (à sa cible ou au contact de son groupe), faux si
    /// elle abandonne parce qu'elle est bloquée.
    fn stop(&mut self, arrived: bool) {
        let target = self.target.take();
        // Une unité qui abandonne loin de sa cible ne doit pas servir de point
        // d'arrêt à son groupe : sinon, le groupe s'arrêterait en chaîne loin du but.
        self.steering.arrived_at = if arrived { target } else { None };
    }

    /// Vrai si `other` est immobile et a terminé un ordre vers (presque) la
    /// même cible que nous : c'est un membre de notre groupe déjà arrivé.
    fn is_blocked_by(&self, other: &Agent) -> bool {
        if self.layer != other.layer {
            return false;
        }
        match (self.target, other.target, other.steering.arrived_at) {
            (Some(target), None, Some(arrived)) => {
                // Tolérance : le plus petit des deux rayons. Les places d'une
                // formation sont espacées de plus d'un diamètre, donc deux
                // places différentes ne sont jamais confondues.
                target.distance(arrived) <= self.radius.min(other.radius)
            }
            _ => false,
        }
    }

    /// Point vers lequel l'unité se dirige maintenant : son prochain point de
    /// passage, ou sa cible (ramenée dans la carte).
    fn steer_point(&self, bounds: Rect) -> Option<Vec2> {
        let target = self.target?;
        Some(self.waypoint.unwrap_or_else(|| clamp_inside(target, bounds, self.radius)))
    }
}

/// Fait avancer la simulation des collisions d'un pas de `dt` secondes.
///
/// `agents` doit toujours être donné dans le même ordre (par exemple trié par
/// identifiant) pour que le résultat soit reproductible au bit près, de même
/// que `obstacles` (rectangles fixes, que seules les unités au sol percutent).
pub fn step(agents: &mut [Agent], dt: f32, bounds: Rect, obstacles: &[Rect], settings: &CollisionSettings) {
    // --- 1. Déplacement en ligne droite vers le point de passage ou la cible ---
    for agent in agents.iter_mut() {
        if agent.hold {
            continue;
        }
        let Some(steer) = agent.steer_point(bounds) else { continue };
        let to_steer = steer - agent.position;
        let distance = to_steer.length();
        let max_step = agent.speed * dt;
        if distance <= max_step {
            agent.position = steer;
            // Arrivé au point de passage : le module `navigation` donnera le
            // suivant au prochain pas. Arrivé à la cible : l'ordre est terminé.
            if agent.waypoint.is_none() {
                agent.stop(true);
            }
        } else {
            agent.position += to_steer / distance * max_step;
        }
    }

    // `fold` parcourt la liste en accumulant une valeur : ici le plus grand rayon.
    let max_radius = agents.iter().map(|a| a.radius).fold(0.0, f32::max);
    if max_radius <= 0.0 {
        return;
    }
    // Obstacles un peu agrandis, pour un premier tri rapide : seuls ceux dont
    // le rectangle agrandi contient le centre de l'unité peuvent la toucher.
    let obstacle_reach = max_radius + settings.contact_margin;
    // Deux cercles ne peuvent se toucher que s'ils sont à moins de deux fois
    // le plus grand rayon : avec des cases de cette taille, il suffit de
    // regarder la case de l'unité et ses 8 voisines.
    let mut grid = SpatialGrid::new(bounds, 2.0 * max_radius + settings.contact_margin);

    // --- 2. Séparation des unités qui se chevauchent ---
    // `vec![x; n]` crée un tableau de n copies de x.
    let mut pushes = vec![Vec2::ZERO; agents.len()];
    for _ in 0..settings.iterations {
        grid.rebuild(agents.iter().map(|a| a.position));
        // `fill` remet toutes les cases du tableau à la même valeur.
        pushes.fill(Vec2::ZERO);

        // On calcule toutes les poussées d'abord…
        for i in 0..agents.len() {
            grid.for_each_near(agents[i].position, |j| {
                // Chaque paire n'est traitée qu'une fois (i < j).
                if j > i {
                    push_apart(agents, i, j, bounds, settings, &mut pushes);
                }
            });
        }
        // … puis on les applique toutes ensemble : le résultat ne dépend pas
        // de l'ordre dans lequel on a parcouru les paires.
        for (agent, push) in agents.iter_mut().zip(&pushes) {
            agent.position = clamp_inside(agent.position + *push, bounds, agent.radius);
        }
        // Enfin, les obstacles fixes repoussent les unités au sol (ils ne
        // bougent jamais : toute la correction est pour l'unité).
        for agent in agents.iter_mut() {
            if agent.layer != CollisionLayer::Ground || agent.radius <= 0.0 {
                continue;
            }
            for rect in obstacles {
                if !rect.inflate(obstacle_reach).contains(agent.position) {
                    continue;
                }
                if let Some(push) = circle_rect_push(agent.position, agent.radius, *rect) {
                    let push = push.clamp_length_max(settings.obstacle_push_max);
                    let mut total = push;
                    // Comme contre une unité lourde : on glisse sur le côté pour contourner.
                    sidestep(agent, push.normalize_or_zero(), push.length(), bounds, settings, &mut total);
                    agent.position = clamp_inside(agent.position + total, bounds, agent.radius);
                }
            }
        }
    }

    // --- 3. Arrêts : arrivée en groupe et unités bloquées ---
    // Les décisions sont prises sur l'état figé de ce pas, puis appliquées :
    // une unité qui s'arrête maintenant n'en arrête une autre qu'au pas suivant.
    grid.rebuild(agents.iter().map(|a| a.position));
    let crowd = crowd_radii(agents, settings);
    let mut to_stop = Vec::new();
    for i in 0..agents.len() {
        if agents[i].target.is_none() || agents[i].hold {
            continue;
        }
        let me = agents[i];
        let Some(steer) = me.steer_point(bounds) else { continue };
        // Direction de marche actuelle (vers le point de passage).
        let to_goal = steer - me.position;

        // Est-on au contact d'un membre du groupe déjà arrivé ? On distingue
        // celui qui nous barre la route (devant nous, vers la cible) de celui
        // qu'on frôle sur le côté : dans ce cas on continue, en glissant le
        // long de lui, pour combler les trous du groupe.
        let (mut touching_group, mut blocked_ahead) = (false, false);
        grid.for_each_near(me.position, |j| {
            let other = &agents[j];
            let reach = me.radius + other.radius + settings.contact_margin;
            let offset = other.position - me.position;
            if j != i && me.is_blocked_by(other) && offset.length_squared() <= reach * reach {
                touching_group = true;
                // `dot` (produit scalaire) : positif si les deux directions
                // vont dans le même sens. Ici : angle de moins de 60° environ.
                if offset.dot(to_goal) >= 0.5 * offset.length() * to_goal.length() {
                    blocked_ahead = true;
                }
            }
        });

        // Suivi du progrès vers la cible, mesuré **le long du chemin** : un
        // détour autour d'un bâtiment éloigne parfois de la cible à vol
        // d'oiseau, sans être un blocage.
        let distance = to_goal.length() + me.remaining;
        let steering = &mut agents[i].steering;
        if distance < steering.best_distance - settings.progress_epsilon {
            steering.best_distance = distance;
            steering.stalled_ticks = 0;
        } else {
            steering.stalled_ticks += 1;
        }
        let stalled = steering.stalled_ticks >= settings.stall_ticks;

        // « Assez près » : pas plus loin de la cible que la taille du groupe
        // une fois tassé. Sans cette limite, une file d'unités en marche qui se
        // touchent s'arrêterait d'un coup tout entière, loin derrière la tête.
        let near_enough = distance <= crowd[i] + me.radius;

        // On est arrivé si l'on touche son groupe en étant assez près, ou si
        // l'on reste bloqué contre lui (ou près du but) depuis un moment.
        // Sinon, après une longue attente sans progrès, on abandonne l'ordre.
        if (blocked_ahead && near_enough) || (stalled && (touching_group || near_enough)) {
            to_stop.push((i, true));
        } else if steering.stalled_ticks >= settings.stall_ticks * settings.give_up_factor {
            to_stop.push((i, false));
        }
    }
    for (i, arrived) in to_stop {
        agents[i].stop(arrived);
    }
}

/// Pour chaque unité en marche, rayon estimé du groupe d'unités qui vont (ou
/// sont arrivées) au même point, une fois tassées autour de lui.
fn crowd_radii(agents: &[Agent], settings: &CollisionSettings) -> Vec<f32> {
    use std::f32::consts::PI;
    // Clé exacte d'un point : la couche, puis la représentation binaire de ses
    // deux nombres (les unités volantes ne s'entassent pas avec celles au sol).
    let key = |layer: CollisionLayer, p: Vec2| (layer, p.x.to_bits(), p.y.to_bits());

    // Surface au sol de chaque unité, rangée par destination.
    let mut areas: Vec<((CollisionLayer, u32, u32), f32)> = agents
        .iter()
        // `filter_map` : transforme chaque élément et ne garde que les `Some`.
        .filter_map(|a| {
            let destination = a.target.or(a.steering.arrived_at)?;
            Some((key(a.layer, destination), PI * a.radius * a.radius))
        })
        .collect();
    // `sort_by_key` est un tri « stable » : les égaux gardent leur ordre, donc
    // les sommes ci-dessous se font toujours dans le même ordre.
    areas.sort_by_key(|(k, _)| *k);

    // Somme des surfaces par destination (les égaux sont maintenant côte à côte).
    let mut totals: Vec<((CollisionLayer, u32, u32), f32)> = Vec::new();
    for (k, area) in areas {
        match totals.last_mut() {
            Some((last, total)) if *last == k => *total += area,
            _ => totals.push((k, area)),
        }
    }

    agents
        .iter()
        .map(|a| {
            let Some(target) = a.target else { return 0.0 };
            // `binary_search_by_key` : recherche rapide dans une liste triée.
            let total = totals
                .binary_search_by_key(&key(a.layer, target), |(k, _)| *k)
                .map_or(0.0, |index| totals[index].1);
            (total / (PI * settings.crowd_packing)).sqrt()
        })
        .collect()
}

/// Calcule la poussée qui sépare les unités `i` et `j` si elles se chevauchent.
fn push_apart(agents: &[Agent], i: usize, j: usize, bounds: Rect, settings: &CollisionSettings, pushes: &mut [Vec2]) {
    let (a, b) = (&agents[i], &agents[j]);
    // Une unité volante passe au-dessus des unités au sol.
    if a.layer != b.layer {
        return;
    }
    let min_distance = a.radius + b.radius;
    let delta = b.position - a.position;
    let distance_sq = delta.length_squared();
    if distance_sq >= min_distance * min_distance {
        return;
    }
    let distance = distance_sq.sqrt();
    let overlap = min_distance - distance;
    if overlap <= settings.slop {
        return;
    }
    // Direction de la poussée, de a vers b. Si les deux centres sont confondus,
    // on prend une direction fixe tirée des indices (pas d'aléatoire).
    let normal = if distance > 1e-6 {
        delta / distance
    } else {
        FALLBACK_DIRECTIONS[(i * 5 + j * 3) % FALLBACK_DIRECTIONS.len()]
    };
    let correction = (overlap - settings.slop) * settings.relaxation;

    // Qui recule ? Une unité en marche bouscule une unité immobile… sauf si
    // celle-ci est un membre de son groupe déjà arrivé : c'est alors l'unité
    // en marche qui recule, pour ne pas désorganiser le groupe en place.
    let push = settings.idle_push_share;
    let base_share_b = match (a.target.is_some(), b.target.is_some()) {
        (true, false) if a.is_blocked_by(b) => 1.0 - push,
        (true, false) => push,
        (false, true) if b.is_blocked_by(a) => push,
        (false, true) => 1.0 - push,
        _ => 0.5,
    };
    // Puis on tient compte des masses : un char écarte un fantassin sans
    // ralentir, un fantassin qui pousse un char n'arrive qu'à se repousser lui-même.
    let share_b = if b.target.is_none() && a.target.is_some() && b.mass > a.mass {
        // Une unité immobile plus lourde que celle qui la heurte ne bouge pas du tout.
        0.0
    } else if a.target.is_none() && b.target.is_some() && a.mass > b.mass {
        1.0
    } else {
        let (weight_a, weight_b) = (base_share_b * a.mass, (1.0 - base_share_b) * b.mass);
        weight_a / (weight_a + weight_b)
    };
    pushes[i] -= normal * correction * (1.0 - share_b);
    pushes[j] += normal * correction * share_b;

    // Une unité en marche qu'on repousse glisse aussi sur le côté, pour
    // contourner l'obstacle au lieu de rester bloquée contre lui.
    sidestep(a, -normal, correction * (1.0 - share_b), bounds, settings, &mut pushes[i]);
    sidestep(b, normal, correction * share_b, bounds, settings, &mut pushes[j]);
}

/// Ajoute à `push` un glissement sur le côté pour une unité en marche
/// repoussée dans la direction `away` (qui l'éloigne de l'obstacle).
fn sidestep(agent: &Agent, away: Vec2, amount: f32, bounds: Rect, settings: &CollisionSettings, push: &mut Vec2) {
    if agent.hold {
        return;
    }
    let Some(steer) = agent.steer_point(bounds) else { return };
    let to_goal = steer - agent.position;
    // On ne glisse que si l'obstacle est devant nous (pas quand on nous pousse par-derrière).
    if amount <= 0.0 || away.dot(to_goal) >= 0.0 {
        return;
    }
    // `perp` : le vecteur tourné d'un quart de tour. On choisit le côté qui
    // rapproche de la cible ; pile en face, on passe toujours par la gauche
    // (choix fixe, pour rester déterministe).
    let side = away.perp();
    let side = if side.dot(to_goal) < 0.0 { -side } else { side };
    *push += side * amount * settings.sidestep;
}

/// Huit directions fixes, pour séparer deux unités exactement superposées.
/// Écrites en dur plutôt que calculées avec `sin`/`cos`, dont le résultat
/// peut varier d'une plateforme à l'autre.
const FALLBACK_DIRECTIONS: [Vec2; 8] = {
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    [
        Vec2::new(1.0, 0.0),
        Vec2::new(D, D),
        Vec2::new(0.0, 1.0),
        Vec2::new(-D, D),
        Vec2::new(-1.0, 0.0),
        Vec2::new(-D, -D),
        Vec2::new(0.0, -1.0),
        Vec2::new(D, -D),
    ]
};

/// Poussée qui sort un cercle (centre `center`, rayon `radius`) du rectangle
/// `rect`, ou `None` s'ils ne se touchent pas.
///
/// On cherche le point du rectangle le plus proche du centre (`clamp` du
/// centre dans le rectangle) : si ce point est à moins d'un rayon, le cercle
/// mord dans le rectangle et on le repousse dans la direction opposée. Si le
/// centre est **dans** le rectangle, on le sort par le bord le plus proche.
pub fn circle_rect_push(center: Vec2, radius: f32, rect: Rect) -> Option<Vec2> {
    let closest = center.clamp(rect.min, rect.max);
    let offset = center - closest;
    let distance_sq = offset.length_squared();
    if distance_sq > 1e-12 {
        if distance_sq >= radius * radius {
            return None;
        }
        let distance = distance_sq.sqrt();
        return Some(offset / distance * (radius - distance));
    }
    // Centre à l'intérieur : distance à chacun des quatre bords. En cas
    // d'égalité, l'ordre fixe du tableau décide (déterministe).
    let exits = [
        (center.x - rect.min.x, Vec2::NEG_X),
        (rect.max.x - center.x, Vec2::X),
        (center.y - rect.min.y, Vec2::NEG_Y),
        (rect.max.y - center.y, Vec2::Y),
    ];
    let (depth, direction) = exits.into_iter().fold(exits[0], |best, e| if e.0 < best.0 { e } else { best });
    Some(direction * (depth + radius))
}

/// Ramène un cercle de rayon `radius` à l'intérieur des limites.
fn clamp_inside(point: Vec2, bounds: Rect, radius: f32) -> Vec2 {
    // Si la carte est plus petite que l'unité, on la centre plutôt que de paniquer.
    let min = (bounds.min + radius).min(bounds.center());
    let max = (bounds.max - radius).max(bounds.center());
    point.clamp(min, max)
}

/// Grille spatiale : range des points dans des cases carrées pour trouver
/// rapidement les voisins d'une position.
///
/// Construite par « tri par comptage » dans des tableaux simples (`Vec`) : pas
/// de `HashMap` (table de hachage), dont l'ordre de parcours n'est pas garanti.
/// Ici, dans chaque case, les points sont toujours rangés par indice croissant.
#[derive(Debug, Clone)]
pub struct SpatialGrid {
    origin: Vec2,
    cell_size: f32,
    columns: usize,
    rows: usize,
    /// Pour la case `c`, ses points sont `items[cell_start[c]..cell_start[c + 1]]`.
    cell_start: Vec<usize>,
    /// Indices des points, regroupés case par case.
    items: Vec<usize>,
    /// Case de chaque point (tableau de travail réutilisé).
    point_cells: Vec<usize>,
}

impl SpatialGrid {
    /// Grille vide couvrant `bounds`, avec des cases de `cell_size` mètres.
    pub fn new(bounds: Rect, cell_size: f32) -> Self {
        let cell_size = cell_size.max(0.01);
        // `as usize` convertit un nombre à virgule en entier (en tronquant).
        let columns = ((bounds.width() / cell_size).ceil() as usize).max(1);
        let rows = ((bounds.height() / cell_size).ceil() as usize).max(1);
        Self {
            origin: bounds.min,
            cell_size,
            columns,
            rows,
            cell_start: vec![0; columns * rows + 1],
            items: Vec::new(),
            point_cells: Vec::new(),
        }
    }

    /// Coordonnées (colonne, ligne) de la case qui contient `p`.
    /// Un point hors de la carte est rattaché à la case du bord la plus proche.
    fn cell_of(&self, p: Vec2) -> (usize, usize) {
        let local = (p - self.origin) / self.cell_size;
        let column = (local.x.floor().max(0.0) as usize).min(self.columns - 1);
        let row = (local.y.floor().max(0.0) as usize).min(self.rows - 1);
        (column, row)
    }

    /// Range à nouveau tous les points. Le point d'indice `k` est le k-ième
    /// élément de `positions`.
    pub fn rebuild(&mut self, positions: impl Iterator<Item = Vec2>) {
        // 1. Case de chaque point, et nombre de points par case.
        self.point_cells.clear();
        self.cell_start.fill(0);
        for p in positions {
            let (column, row) = self.cell_of(p);
            let cell = row * self.columns + column;
            self.point_cells.push(cell);
            self.cell_start[cell + 1] += 1;
        }
        // 2. Sommes cumulées : `cell_start[c]` devient le début de la case c.
        for c in 1..self.cell_start.len() {
            self.cell_start[c] += self.cell_start[c - 1];
        }
        // 3. On place chaque point dans sa case, par indice croissant.
        self.items.clear();
        self.items.resize(self.point_cells.len(), 0);
        let mut next = self.cell_start.clone();
        for (index, &cell) in self.point_cells.iter().enumerate() {
            self.items[next[cell]] = index;
            next[cell] += 1;
        }
    }

    /// Appelle `visit` pour chaque point rangé dans la case de `p` ou dans
    /// l'une de ses 8 voisines, toujours dans le même ordre.
    ///
    /// `impl FnMut(usize)` : n'importe quelle fonction ou « closure » (fonction
    /// écrite sur place, `|j| { … }`) qui reçoit un indice et peut modifier
    /// des variables autour d'elle.
    pub fn for_each_near(&self, p: Vec2, mut visit: impl FnMut(usize)) {
        let (column, row) = self.cell_of(p);
        // `saturating_sub` : soustraction qui s'arrête à 0 au lieu de déborder.
        for r in row.saturating_sub(1)..=(row + 1).min(self.rows - 1) {
            for c in column.saturating_sub(1)..=(column + 1).min(self.columns - 1) {
                let cell = r * self.columns + c;
                for &index in &self.items[self.cell_start[cell]..self.cell_start[cell + 1]] {
                    visit(index);
                }
            }
        }
    }
}

impl SpatialGrid {
    /// Appelle `visit` pour chaque point rangé dans une case qui touche le
    /// carré de demi-côté `radius` autour de `p` (à l'appelant de vérifier
    /// la distance exacte), toujours dans le même ordre. Contrairement à
    /// [`SpatialGrid::for_each_near`], la portée ne dépend pas de la taille des cases.
    pub fn for_each_within(&self, p: Vec2, radius: f32, mut visit: impl FnMut(usize)) {
        let (min_c, min_r) = self.cell_of(p - Vec2::splat(radius));
        let (max_c, max_r) = self.cell_of(p + Vec2::splat(radius));
        for r in min_r..=max_r {
            for c in min_c..=max_c {
                let cell = r * self.columns + c;
                for &index in &self.items[self.cell_start[cell]..self.cell_start[cell + 1]] {
                    visit(index);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 32.0;

    fn bounds() -> Rect {
        MapBounds::default().0
    }

    fn agent(position: Vec2, radius: f32, speed: f32) -> Agent {
        Agent {
            position,
            radius,
            mass: 1.0,
            speed,
            target: None,
            waypoint: None,
            remaining: 0.0,
            hold: false,
            layer: CollisionLayer::Ground,
            steering: Steering::default(),
        }
    }

    /// Plus grand chevauchement entre deux unités (comparaison de toutes les paires).
    fn max_overlap(agents: &[Agent]) -> f32 {
        let mut worst = 0.0f32;
        for i in 0..agents.len() {
            for j in i + 1..agents.len() {
                let d = agents[i].position.distance(agents[j].position);
                worst = worst.max(agents[i].radius + agents[j].radius - d);
            }
        }
        worst
    }

    /// Une armée de `count` unités des trois gabarits du jeu, en carré espacé de 2 m.
    fn army(count: usize, corner: Vec2) -> Vec<Agent> {
        let kinds = [(0.35, 4.0), (0.5, 6.5), (1.22, 3.0)];
        let side = (count as f32).sqrt().ceil() as usize;
        (0..count)
            .map(|i| {
                let (radius, speed) = kinds[i % kinds.len()];
                let pos = corner + Vec2::new((i % side) as f32, (i / side) as f32) * 2.0;
                agent(pos, radius, speed)
            })
            .collect()
    }

    #[test]
    fn overlapping_units_separate() {
        let mut agents = vec![agent(Vec2::new(0.0, 0.0), 0.5, 4.0), agent(Vec2::new(0.2, 0.0), 0.5, 4.0)];
        for _ in 0..32 {
            step(&mut agents, DT, bounds(), &[], &CollisionSettings::default());
        }
        assert!(max_overlap(&agents) <= 0.02, "chevauchement restant : {}", max_overlap(&agents));
        // Poussées symétriques : chacune a reculé de son côté.
        assert!(agents[0].position.x < 0.0 && agents[1].position.x > 0.2);
    }

    #[test]
    fn exactly_superposed_units_separate() {
        let mut agents = vec![agent(Vec2::ZERO, 0.5, 4.0), agent(Vec2::ZERO, 0.5, 4.0)];
        for _ in 0..32 {
            step(&mut agents, DT, bounds(), &[], &CollisionSettings::default());
        }
        assert!(max_overlap(&agents) <= 0.02);
    }

    #[test]
    fn moving_unit_pushes_idle_unit_out_of_its_way() {
        let mut agents = vec![agent(Vec2::new(-5.0, 0.0), 0.5, 4.0), agent(Vec2::new(0.0, 0.1), 0.5, 4.0)];
        agents[0].target = Some(Vec2::new(5.0, 0.0));
        for _ in 0..32 * 4 {
            step(&mut agents, DT, bounds(), &[], &CollisionSettings::default());
        }
        // L'unité en marche est arrivée, l'autre a été écartée du chemin.
        assert_eq!(agents[0].target, None);
        assert!(agents[0].position.distance(Vec2::new(5.0, 0.0)) < 0.1);
        assert!(agents[1].position.y > 0.5, "l'unité immobile aurait dû être poussée");
    }

    #[test]
    fn light_unit_goes_around_heavy_idle_unit() {
        // Un fantassin fonce pile sur un char immobile.
        let mut tank = agent(Vec2::new(0.0, 0.0), 1.8, 2.6);
        tank.mass = 12.0;
        let mut agents = vec![agent(Vec2::new(-6.0, 0.0), 0.5, 4.0), tank];
        agents[0].target = Some(Vec2::new(6.0, 0.0));
        for _ in 0..32 * 6 {
            step(&mut agents, DT, bounds(), &[], &CollisionSettings::default());
        }
        // Le char n'a pas bougé et le fantassin l'a contourné jusqu'à sa cible.
        assert_eq!(agents[1].position, Vec2::ZERO, "le char a été déplacé");
        assert_eq!(agents[0].target, None);
        assert!(agents[0].position.distance(Vec2::new(6.0, 0.0)) < 0.1);
    }

    #[test]
    fn heavy_unit_pushes_light_idle_unit() {
        let mut tank = agent(Vec2::new(-6.0, 0.0), 1.8, 2.6);
        tank.mass = 12.0;
        tank.target = Some(Vec2::new(6.0, 0.0));
        let mut agents = vec![tank, agent(Vec2::new(0.0, 0.3), 0.5, 4.0)];
        for _ in 0..32 * 6 {
            step(&mut agents, DT, bounds(), &[], &CollisionSettings::default());
        }
        // Le char passe sans dévier, le fantassin a été écarté.
        assert_eq!(agents[0].target, None);
        assert!(agents[0].position.distance(Vec2::new(6.0, 0.0)) < 0.1);
        assert!(agents[1].position.y > 2.0, "le fantassin aurait dû être écarté");
    }

    #[test]
    fn units_stay_inside_map() {
        // Cible hors de la carte : les unités s'entassent dans le coin.
        let mut agents = army(100, Vec2::new(20.0, -20.0));
        for a in &mut agents {
            a.target = Some(Vec2::new(80.0, -70.0));
        }
        for _ in 0..32 * 30 {
            step(&mut agents, DT, bounds(), &[], &CollisionSettings::default());
        }
        for a in &agents {
            assert!(a.position.x - a.radius >= -50.0 - 1e-4 && a.position.x + a.radius <= 50.0 + 1e-4);
            assert!(a.position.y - a.radius >= -50.0 - 1e-4 && a.position.y + a.radius <= 50.0 + 1e-4);
            assert_eq!(a.target, None, "toutes les unités devraient s'être arrêtées dans le coin");
        }
    }

    /// Envoie 300 unités au même point et renvoie leur état après `ticks` pas,
    /// ainsi que le déplacement maximal pendant le dernier pas.
    fn crowd_to_one_point(ticks: usize) -> (Vec<Agent>, f32) {
        let mut agents = army(300, Vec2::new(-40.0, -40.0));
        for a in &mut agents {
            a.target = Some(Vec2::new(10.0, 5.0));
        }
        let settings = CollisionSettings::default();
        let mut last_motion = 0.0;
        for _ in 0..ticks {
            let before: Vec<Vec2> = agents.iter().map(|a| a.position).collect();
            step(&mut agents, DT, bounds(), &[], &settings);
            last_motion = agents
                .iter()
                .zip(&before)
                .map(|(a, b)| a.position.distance(*b))
                .fold(0.0, f32::max);
        }
        (agents, last_motion)
    }

    /// Vérifie qu'un groupe s'est arrêté proprement : plus personne en marche,
    /// pas de chevauchement, plus aucun mouvement, et un groupe compact.
    fn assert_settled(agents: &[Agent], last_motion: f32) {
        let moving = agents.iter().filter(|a| a.target.is_some()).count();
        assert_eq!(moving, 0, "{moving} unités sont encore en marche");
        let overlap = max_overlap(agents);
        assert!(overlap < 0.05, "chevauchement restant : {overlap}");
        assert!(last_motion < 1e-4, "les unités bougent encore : {last_motion}");

        // Compacité : rayon du groupe autour de son centre, comparé au rayon
        // d'un disque qui aurait exactement la surface des unités.
        // `sum` additionne tous les éléments d'un itérateur.
        let center = agents.iter().map(|a| a.position).sum::<Vec2>() / agents.len() as f32;
        let area: f32 = agents.iter().map(|a| std::f32::consts::PI * a.radius * a.radius).sum();
        let ideal = (area / std::f32::consts::PI).sqrt();
        let extent = agents.iter().map(|a| a.position.distance(center) + a.radius).fold(0.0, f32::max);
        assert!(extent < 1.5 * ideal, "groupe trop étalé : {extent} m pour {ideal} m idéal");
    }

    #[test]
    fn crowd_sent_to_one_point_settles_without_overlap() {
        // 40 secondes de jeu : les unités lentes partent de 70 m.
        let (agents, last_motion) = crowd_to_one_point(32 * 40);
        assert_settled(&agents, last_motion);
    }

    #[test]
    fn crowd_sent_to_formation_slots_settles() {
        // Le chemin réel du jeu : chaque unité reçoit sa place dans la formation.
        let mut agents = army(300, Vec2::new(-40.0, -40.0));
        let radii: Vec<f32> = agents.iter().map(|a| a.radius).collect();
        let slots = crate::selection::formation(&radii, Vec2::new(10.0, 5.0));
        for (a, slot) in agents.iter_mut().zip(slots) {
            a.target = Some(slot);
        }
        let settings = CollisionSettings::default();
        let mut last_motion = 0.0;
        for _ in 0..32 * 40 {
            let before: Vec<Vec2> = agents.iter().map(|a| a.position).collect();
            step(&mut agents, DT, bounds(), &[], &settings);
            last_motion = agents.iter().zip(&before).map(|(a, b)| a.position.distance(*b)).fold(0.0, f32::max);
        }
        let moving = agents.iter().filter(|a| a.target.is_some()).count();
        assert_eq!(moving, 0, "{moving} unités sont encore en marche");
        assert!(max_overlap(&agents) < 0.05, "chevauchement : {}", max_overlap(&agents));
        assert!(last_motion < 1e-4, "les unités bougent encore : {last_motion}");
    }

    #[test]
    fn simulation_is_deterministic() {
        let (first, _) = crowd_to_one_point(32 * 5);
        let (second, _) = crowd_to_one_point(32 * 5);
        // Comparaison au bit près : `to_bits` donne la représentation binaire d'un `f32`.
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.position.x.to_bits(), b.position.x.to_bits());
            assert_eq!(a.position.y.to_bits(), b.position.y.to_bits());
            assert_eq!(a.target, b.target);
        }
    }

    #[test]
    fn circle_against_rectangle() {
        let rect = Rect::new(0.0, 0.0, 4.0, 2.0);
        // Loin : rien.
        assert_eq!(circle_rect_push(Vec2::new(-1.0, 1.0), 0.5, rect), None);
        // Mord de 0,2 m par la gauche : poussé de 0,2 m vers −x.
        let push = circle_rect_push(Vec2::new(-0.3, 1.0), 0.5, rect).unwrap();
        assert!((push - Vec2::new(-0.2, 0.0)).length() < 1e-5, "{push}");
        // Près d'un coin : poussé en diagonale, hors de portée du coin.
        let center = Vec2::new(4.2, 2.2);
        let push = circle_rect_push(center, 0.5, rect).unwrap();
        assert!(((center + push).distance(Vec2::new(4.0, 2.0)) - 0.5).abs() < 1e-5);
        // Centre à l'intérieur : sorti par le bord le plus proche (le bas, à 0,3 m).
        let push = circle_rect_push(Vec2::new(2.0, 0.3), 0.5, rect).unwrap();
        assert!((push - Vec2::new(0.0, -0.8)).length() < 1e-5, "{push}");
    }

    #[test]
    fn units_never_enter_obstacles_and_are_pushed_out() {
        let building = Rect::new(-3.0, -3.0, 3.0, 3.0);
        // Une unité fonce dans le mur, une autre est « sous » le bâtiment posé.
        let mut agents = vec![agent(Vec2::new(-8.0, 0.5), 0.5, 4.0), agent(Vec2::new(1.0, 0.5), 0.5, 4.0)];
        agents[0].target = Some(Vec2::new(8.0, 0.5));
        for _ in 0..32 * 4 {
            step(&mut agents, DT, bounds(), &[building], &CollisionSettings::default());
            // L'unité en marche ne rentre jamais dans le bâtiment.
            assert!(circle_rect_push(agents[0].position, 0.5 - 0.02, building).is_none());
        }
        // L'unité enfouie a été poussée dehors.
        assert!(circle_rect_push(agents[1].position, 0.5 - 0.02, building).is_none(), "{}", agents[1].position);
    }

    #[test]
    fn flying_units_ignore_ground_units_and_obstacles() {
        let building = Rect::new(-2.0, -2.0, 2.0, 2.0);
        let mut flyer = agent(Vec2::new(-6.0, 0.0), 0.5, 6.0);
        flyer.layer = CollisionLayer::Air;
        flyer.target = Some(Vec2::new(6.0, 0.0));
        // Un char immobile sur la route, et un second volant posé sur le premier.
        let mut tank = agent(Vec2::new(4.0, 0.0), 1.8, 2.6);
        tank.mass = 12.0;
        let mut other_flyer = agent(Vec2::new(-6.0, 0.2), 0.5, 6.0);
        other_flyer.layer = CollisionLayer::Air;
        let mut agents = vec![flyer, tank, other_flyer];
        for _ in 0..32 * 3 {
            step(&mut agents, DT, bounds(), &[building], &CollisionSettings::default());
        }
        // Le volant a traversé tout droit : il a survolé le bâtiment et le char.
        assert_eq!(agents[0].target, None);
        assert!(agents[0].position.distance(Vec2::new(6.0, 0.0)) < 0.6, "{}", agents[0].position);
        assert_eq!(agents[1].position, Vec2::new(4.0, 0.0), "le char a été poussé par un volant");
        // Les deux volants, eux, se sont séparés (collision air-air).
        assert!(agents[2].position.distance(Vec2::new(-6.0, 0.2)) > 0.1);
    }

    #[test]
    fn grid_finds_the_same_neighbors_as_brute_force() {
        let agents = army(200, Vec2::new(-15.0, -15.0));
        let mut grid = SpatialGrid::new(bounds(), 2.0);
        grid.rebuild(agents.iter().map(|a| a.position));
        for (i, a) in agents.iter().enumerate() {
            let mut found = Vec::new();
            grid.for_each_near(a.position, |j| found.push(j));
            for (j, b) in agents.iter().enumerate() {
                if a.position.distance(b.position) < 2.0 {
                    assert!(found.contains(&j), "voisin {j} de {i} manquant");
                }
            }
        }
    }
}
