//! Groupes de contrôle : mémoriser une sélection sous un chiffre, la
//! rappeler d'une touche.
//!
//! Ctrl (ou Cmd sur Mac) + chiffre mémorise la sélection dans un groupe,
//! Maj + chiffre l'y ajoute, le chiffre seul la rappelle ; deux appuis
//! rapides centrent en plus la caméra sur le groupe. La couche d'entrées
//! traduit ces touches en [`GameAction`] (`AssignGroup`, `AddToGroup`,
//! `RecallGroup`) ; la barre de groupes, en haut à gauche, produit les mêmes
//! actions au clic ou au doigt (ce sont des [`ActionButton`]).
//!
//! Les groupes sont de l'**interface**, pas un état de la simulation : ils
//! ne changent que la sélection et la caméra du joueur local, jamais la
//! partie. Ils ne passent donc pas par la file des ordres.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::buildings::ui::{ui_font, BORDER, BUTTON, BUTTON_ACTIVE, KEY_TEXT, PANEL, TEXT};
use crate::buildings::{Building, BuildingCatalog};
use crate::camera::RtsCamera;
use crate::input::{ActionButton, BlocksPointer, GameAction};
use crate::players::{LocalPlayer, Owner};
use crate::selection::{Selectable, Selected};
use crate::units::{Unit, UnitCatalog};
use crate::RtsSystems;

/// Nombre de groupes : touches 1 à 9, puis 0.
pub const GROUP_COUNT: usize = 10;

/// Délai maximal entre deux rappels du même groupe pour centrer la caméra
/// dessus (« double appui »), en secondes.
pub const DOUBLE_PRESS_SECS: f32 = 0.4;

/// Les groupes de contrôle du joueur local.
///
/// Chaque groupe est une liste d'entités triée et sans doublon (une entité
/// peut appartenir à plusieurs groupes). Les entités mortes en sont retirées
/// à chaque image (voir [`ControlGroups::retain`]).
#[derive(Resource, Debug, Default)]
pub struct ControlGroups {
    /// `[T; N]` : un tableau de taille fixe, ici un `Vec` par groupe.
    groups: [Vec<Entity>; GROUP_COUNT],
    /// Le dernier groupe rappelé et l'instant du rappel (double appui).
    last_recall: Option<(usize, f32)>,
}

impl ControlGroups {
    /// Remplace le contenu du groupe `group` par ces entités.
    pub fn assign(&mut self, group: usize, entities: impl IntoIterator<Item = Entity>) {
        let Some(list) = self.groups.get_mut(group) else { return };
        list.clear();
        list.extend(entities);
        list.sort_unstable();
        list.dedup();
    }

    /// Ajoute ces entités au groupe `group` (celles qui y sont déjà n'y sont pas doublées).
    pub fn add(&mut self, group: usize, entities: impl IntoIterator<Item = Entity>) {
        let Some(list) = self.groups.get_mut(group) else { return };
        list.extend(entities);
        list.sort_unstable();
        list.dedup();
    }

    /// Les entités du groupe `group`, triées (vide pour un numéro inconnu).
    pub fn members(&self, group: usize) -> &[Entity] {
        self.groups.get(group).map_or(&[], Vec::as_slice)
    }

    /// Ne garde, dans tous les groupes, que les entités pour lesquelles
    /// `keep` répond vrai (les vivantes du joueur local).
    ///
    /// `impl FnMut(Entity) -> bool` : n'importe quelle fonction (ou
    /// fermeture) qui prend une entité et répond vrai ou faux.
    pub fn retain(&mut self, mut keep: impl FnMut(Entity) -> bool) {
        for list in &mut self.groups {
            list.retain(|&e| keep(e));
        }
    }

    /// Note un rappel du groupe `group` à l'instant `now` (en secondes) ;
    /// vrai si c'est le second d'un double appui (même groupe, moins de
    /// [`DOUBLE_PRESS_SECS`] après le précédent).
    pub fn register_recall(&mut self, group: usize, now: f32) -> bool {
        let double = self.last_recall.is_some_and(|(last, at)| last == group && now - at <= DOUBLE_PRESS_SECS);
        // Après un double appui, on repart de zéro : un troisième appui rapide
        // compte comme un premier.
        self.last_recall = if double { None } else { Some((group, now)) };
        double
    }
}

/// Le « type principal » d'un groupe : le plus fréquent, et à égalité le
/// plus petit (pour que l'affichage ne saute pas d'une image à l'autre).
///
/// Générique : `K` est n'importe quel type qu'on peut comparer (`Ord`) et
/// copier (`Copy`) ; ici, un type d'unité ou de bâtiment.
pub fn main_type<K: Ord + Copy>(kinds: impl IntoIterator<Item = K>) -> Option<K> {
    let mut counts: BTreeMap<K, usize> = BTreeMap::new();
    for kind in kinds {
        *counts.entry(kind).or_default() += 1;
    }
    // `BTreeMap` parcourt les clés dans l'ordre croissant ; `max_by_key`
    // garde la dernière égalité, d'où l'ordre inversé (`rev`).
    counts.into_iter().rev().max_by_key(|&(_, count)| count).map(|(kind, _)| kind)
}

/// Le chiffre affiché d'un groupe : 1 à 9, puis 0 pour le dixième.
pub fn group_label(group: usize) -> String {
    ((group + 1) % 10).to_string()
}

/// Un type d'entité, pour trouver le type principal d'un groupe. Les unités
/// passent avant les bâtiments à nombre égal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TypeKey {
    Unit(usize),
    Building(usize),
}

/// Une case de la barre de groupes.
#[derive(Component, Debug)]
struct GroupSlot(usize);

/// Le nombre d'entités d'une case.
#[derive(Component, Debug)]
struct GroupCount;

/// Le nom du type principal d'une case.
#[derive(Component, Debug)]
struct GroupName;

/// Marqueur : le panneau de la barre de groupes.
#[derive(Component, Debug)]
struct GroupBar;

pub struct ControlGroupsPlugin;

impl Plugin for ControlGroupsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ControlGroups>().add_systems(Startup, spawn_group_bar).add_systems(
            Update,
            (prune_groups, handle_group_actions, update_group_bar).chain().in_set(RtsSystems::Actions),
        );
    }
}

/// Retire des groupes les entités disparues (mortes, devenues bâtiment…).
fn prune_groups(mut groups: ResMut<ControlGroups>, local: Res<LocalPlayer>, alive: Query<&Owner, With<Selectable>>) {
    // `bypass_change_detection` : ne pas signaler de changement à chaque image.
    groups.bypass_change_detection().retain(|e| alive.get(e).is_ok_and(|o| o.0 == local.0));
}

/// Mémorise, complète ou rappelle les groupes.
fn handle_group_actions(
    mut commands: Commands,
    mut actions: MessageReader<GameAction>,
    time: Res<Time<Real>>,
    local: Res<LocalPlayer>,
    mut groups: ResMut<ControlGroups>,
    selected: Query<(Entity, &Owner), With<Selected>>,
    positions: Query<&GlobalTransform, With<Selectable>>,
    mut cameras: Query<&mut RtsCamera>,
) {
    // Seules les entités du joueur local entrent dans un groupe (pas une
    // unité adverse qu'on regarde).
    let mine = || selected.iter().filter(|(_, o)| o.0 == local.0).map(|(e, _)| e);
    for action in actions.read() {
        match *action {
            // Une sélection vide ne vide pas le groupe : un appui par erreur
            // ne fait rien perdre.
            GameAction::AssignGroup { group } if mine().next().is_some() => groups.assign(group, mine()),
            GameAction::AddToGroup { group } => groups.add(group, mine()),
            GameAction::RecallGroup { group } => {
                let members = groups.members(group).to_vec();
                if members.is_empty() {
                    continue;
                }
                for (entity, _) in &selected {
                    commands.entity(entity).remove::<Selected>();
                }
                for &entity in &members {
                    commands.entity(entity).insert(Selected);
                }
                // Double appui : la caméra se centre sur le groupe.
                if groups.register_recall(group, time.elapsed_secs()) {
                    let points: Vec<Vec2> = members.iter().filter_map(|&e| positions.get(e).ok()).map(|t| t.translation().xz()).collect();
                    if !points.is_empty() {
                        let center = points.iter().sum::<Vec2>() / points.len() as f32;
                        for mut camera in &mut cameras {
                            camera.look_at(center);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Largeur d'une case de la barre de groupes, en pixels.
const SLOT_WIDTH: f32 = 96.0;

fn spawn_group_bar(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Barre de groupes"),
            GroupBar,
            BlocksPointer,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(14.0),
                padding: UiRect::all(Val::Px(4.0)),
                column_gap: Val::Px(4.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(BORDER),
            Visibility::Hidden,
        ))
        .with_children(|bar| {
            for group in 0..GROUP_COUNT {
                bar.spawn((
                    GroupSlot(group),
                    ActionButton { action: GameAction::RecallGroup { group }, enabled: true, hotkey: None },
                    Node {
                        width: Val::Px(SLOT_WIDTH),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        // Un nom trop long est coupé plutôt que d'élargir la case.
                        overflow: Overflow::clip(),
                        // Cachée tant que le groupe est vide (et sans prendre de place).
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                    BorderColor::all(BORDER),
                ))
                .with_children(|slot| {
                    slot.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|row| {
                        row.spawn((Text::new(group_label(group)), ui_font(15.0), TextColor(KEY_TEXT)));
                        row.spawn((GroupCount, Text::new(""), ui_font(15.0), TextColor(TEXT)));
                    });
                    slot.spawn((GroupName, Text::new(""), ui_font(11.0), TextColor(TEXT), TextLayout::linebreak(LineBreak::NoWrap)));
                });
            }
        });
}

/// Montre les groupes non vides : chiffre, nombre d'entités, nom du type
/// principal ; éclaire celui qui correspond exactement à la sélection.
fn update_group_bar(
    groups: Res<ControlGroups>,
    local: Res<LocalPlayer>,
    unit_catalog: Option<Res<UnitCatalog>>,
    building_catalog: Option<Res<BuildingCatalog>>,
    selected: Query<(Entity, &Owner), With<Selected>>,
    kinds: Query<(Option<&Unit>, Option<&Building>)>,
    mut bar: Query<&mut Visibility, With<GroupBar>>,
    mut slots: Query<(&GroupSlot, &mut Node, &mut BackgroundColor, &mut BorderColor, &Children)>,
    rows: Query<&Children, Without<GroupSlot>>,
    mut counts: Query<&mut Text, (With<GroupCount>, Without<GroupName>)>,
    mut names: Query<&mut Text, (With<GroupName>, Without<GroupCount>)>,
) {
    let Ok(mut visibility) = bar.single_mut() else { return };
    let any = (0..GROUP_COUNT).any(|g| !groups.members(g).is_empty());
    visibility.set_if_neq(if any { Visibility::Inherited } else { Visibility::Hidden });
    if !any {
        return;
    }
    let mut selection: Vec<Entity> = selected.iter().filter(|(_, o)| o.0 == local.0).map(|(e, _)| e).collect();
    selection.sort_unstable();

    for (slot, mut node, mut background, mut border, children) in &mut slots {
        let members = groups.members(slot.0);
        let display = if members.is_empty() { Display::None } else { Display::Flex };
        if node.display != display {
            node.display = display;
        }
        if members.is_empty() {
            continue;
        }
        let active = members == selection.as_slice();
        background.set_if_neq(BackgroundColor(if active { BUTTON_ACTIVE } else { BUTTON }));
        border.set_if_neq(BorderColor::all(if active { KEY_TEXT } else { BORDER }));

        let main = main_type(members.iter().filter_map(|&e| match kinds.get(e) {
            Ok((Some(unit), _)) => Some(TypeKey::Unit(unit.type_index)),
            Ok((None, Some(building))) => Some(TypeKey::Building(building.type_index)),
            _ => None,
        }));
        let name = match main {
            Some(TypeKey::Unit(t)) => unit_catalog.as_ref().and_then(|c| c.types.get(t)).map(|t| t.name.clone()),
            Some(TypeKey::Building(t)) => building_catalog.as_ref().and_then(|c| c.types.get(t)).map(|t| t.name.clone()),
            None => None,
        }
        .unwrap_or_default();
        let count = members.len().to_string();

        // Le nombre est dans la première rangée, le nom juste en dessous.
        for child in children.iter() {
            if let Ok(mut text) = names.get_mut(child)
                && text.0 != name
            {
                text.0.clone_from(&name);
            }
            for grandchild in rows.get(child).into_iter().flatten() {
                if let Ok(mut text) = counts.get_mut(*grandchild)
                    && text.0 != count
                {
                    text.0.clone_from(&count);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entities(n: u32) -> Vec<Entity> {
        (1..=n).map(|i| Entity::from_raw_u32(i).unwrap()).collect()
    }

    /// Ces entités dans l'ordre des groupes (celui de `Entity`, qui n'est pas
    /// forcément celui des numéros). `const N: usize` : un tableau de taille quelconque.
    fn sorted<const N: usize>(mut list: [Entity; N]) -> Vec<Entity> {
        list.sort_unstable();
        list.to_vec()
    }

    #[test]
    fn assign_replaces_and_add_merges_without_duplicates() {
        let e = entities(4);
        let mut groups = ControlGroups::default();
        groups.assign(0, [e[2], e[0], e[2]]);
        assert_eq!(groups.members(0), sorted([e[0], e[2]]), "trié, sans doublon");
        groups.add(0, [e[1], e[0]]);
        assert_eq!(groups.members(0), sorted([e[0], e[1], e[2]]));
        groups.assign(0, [e[3]]);
        assert_eq!(groups.members(0), &[e[3]], "mémoriser remplace le groupe");
        // Une entité peut être dans deux groupes ; les autres groupes ne bougent pas.
        groups.assign(9, [e[3]]);
        assert_eq!(groups.members(9), &[e[3]]);
        assert!(groups.members(1).is_empty());
        // Numéro inconnu : rien ne se passe.
        groups.assign(GROUP_COUNT, [e[0]]);
        assert!(groups.members(GROUP_COUNT).is_empty());
    }

    #[test]
    fn dead_entities_leave_every_group() {
        let e = entities(3);
        let mut groups = ControlGroups::default();
        groups.assign(0, [e[0], e[1]]);
        groups.assign(4, [e[1], e[2]]);
        groups.retain(|entity| entity != e[1]);
        assert_eq!(groups.members(0), &[e[0]]);
        assert_eq!(groups.members(4), &[e[2]]);
    }

    #[test]
    fn double_press_needs_the_same_group_quickly() {
        let mut groups = ControlGroups::default();
        assert!(!groups.register_recall(0, 10.0), "premier appui");
        assert!(groups.register_recall(0, 10.3), "second appui rapide");
        assert!(!groups.register_recall(0, 10.5), "un troisième appui repart de zéro");
        assert!(!groups.register_recall(1, 10.6), "autre groupe");
        assert!(!groups.register_recall(0, 10.7));
        assert!(!groups.register_recall(0, 11.5), "trop lent");
    }

    #[test]
    fn main_type_is_the_most_common_then_the_smallest() {
        assert_eq!(main_type([3, 1, 3, 2]), Some(3));
        assert_eq!(main_type([2, 1, 1, 2]), Some(1), "égalité : le plus petit");
        assert_eq!(main_type(Vec::<usize>::new()), None);
        assert_eq!(main_type([TypeKey::Building(0), TypeKey::Unit(5)]), Some(TypeKey::Unit(5)), "à égalité, les unités d'abord");
    }

    #[test]
    fn labels_follow_the_keyboard() {
        assert_eq!(group_label(0), "1");
        assert_eq!(group_label(8), "9");
        assert_eq!(group_label(9), "0");
    }
}
