//! Ce qu'on a sélectionné, et ce qu'on survole : affichage seulement.
//!
//! - **Panneau de sélection**, en haut au centre : pour une seule entité,
//!   son nom, ses points de vie (et l'avancement d'un chantier) ; pour
//!   plusieurs, chaque type avec son nombre (« Sentinelle ×6 · Lancier ×2 »).
//! - **Étiquette au survol** : le nom de ce qui est sous le pointeur, en
//!   rouge pour un adversaire.
//!
//! Les noms viennent du composant `Name` que portent déjà toutes les unités
//! et tous les bâtiments (le nom de leur type, dans les données du jeu).

use bevy::prelude::*;

use crate::buildings::ui::{ui_font, BORDER, KEY_TEXT, PANEL, TEXT};
use crate::buildings::{Health, UnderConstruction};
use crate::camera::{screen_ray, RtsCamera};
use crate::cursor::Pointer;
use crate::players::{LocalPlayer, Owner};
use crate::selection::{pick, Pickable, Selected};
use crate::vision::display::FogHidden;
use crate::RtsSystems;

/// Couleur du nom d'une entité adverse.
const ENEMY_TEXT: Color = Color::srgb(1.0, 0.42, 0.36);

/// Au-delà, les types sélectionnés sont résumés (« … »).
const MAX_TYPES_SHOWN: usize = 6;

pub struct SelectionInfoPlugin;

impl Plugin for SelectionInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_widgets)
            .add_systems(Update, (update_selection_panel, update_hover_label).in_set(RtsSystems::Actions));
    }
}

/// Le panneau de sélection (sa racine), son titre et sa ligne de détails.
#[derive(Component)]
struct SelectionPanel;
#[derive(Component)]
struct SelectionTitle;
#[derive(Component)]
struct SelectionDetails;

/// L'étiquette qui suit le pointeur.
#[derive(Component)]
struct HoverLabel;

fn spawn_widgets(mut commands: Commands) {
    // Conteneur pleine largeur (transparent, il ne bloque pas les clics) qui
    // centre le panneau en haut de l'écran.
    commands
        .spawn((
            Name::new("Panneau de sélection"),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                SelectionPanel,
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                    row_gap: Val::Px(2.0),
                    border: UiRect::all(Val::Px(1.0)),
                    max_width: Val::Px(620.0),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(BORDER),
                Visibility::Hidden,
            ))
            .with_children(|panel| {
                panel.spawn((SelectionTitle, Text::new(""), ui_font(18.0), TextColor(TEXT)));
                panel.spawn((SelectionDetails, Text::new(""), ui_font(13.0), TextColor(KEY_TEXT)));
            });
        });
    commands
        .spawn((
            Name::new("Étiquette de survol"),
            HoverLabel,
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(PANEL),
            Visibility::Hidden,
            // Au-dessus des autres panneaux.
            GlobalZIndex(5),
        ))
        .with_children(|label| {
            label.spawn((Text::new(""), ui_font(13.0), TextColor(TEXT)));
        });
}

/// Le titre et la ligne de détails pour une sélection : `entries` donne,
/// pour chaque entité, son nom, ses PV et l'avancement de son chantier.
fn describe(entries: &[(&str, Option<&Health>, Option<f32>)]) -> (String, String) {
    match entries {
        [] => (String::new(), String::new()),
        [(name, health, progress)] => {
            let mut details = Vec::new();
            if let Some(health) = health {
                details.push(format!("PV {:.0} / {:.0}", health.current.max(0.0).ceil(), health.max));
            }
            if let Some(progress) = progress {
                details.push(format!("en construction : {:.0} %", progress * 100.0));
            }
            (name.to_string(), details.join(" · "))
        }
        _ => {
            // Chaque type avec son nombre : le plus nombreux d'abord, puis
            // par ordre alphabétique (ordre stable d'une image à l'autre).
            let mut counts: Vec<(&str, usize)> = Vec::new();
            for (name, ..) in entries {
                match counts.iter_mut().find(|(n, _)| n == name) {
                    Some((_, count)) => *count += 1,
                    None => counts.push((name, 1)),
                }
            }
            counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let mut parts: Vec<String> =
                counts.iter().take(MAX_TYPES_SHOWN).map(|(name, n)| if *n > 1 { format!("{name} ×{n}") } else { name.to_string() }).collect();
            if counts.len() > MAX_TYPES_SHOWN {
                parts.push("…".into());
            }
            (format!("{} sélectionnés", entries.len()), parts.join(" · "))
        }
    }
}

fn update_selection_panel(
    local: Res<LocalPlayer>,
    selected: Query<(&Name, Option<&Owner>, Option<&Health>, Option<&UnderConstruction>), (With<Selected>, Without<FogHidden>)>,
    mut panel: Query<&mut Visibility, With<SelectionPanel>>,
    mut title: Query<(&mut Text, &mut TextColor), (With<SelectionTitle>, Without<SelectionDetails>)>,
    mut details: Query<&mut Text, (With<SelectionDetails>, Without<SelectionTitle>)>,
) {
    let entries: Vec<(&str, Option<&Health>, Option<f32>)> =
        selected.iter().map(|(name, _, health, site)| (name.as_str(), health, site.map(|s| s.progress))).collect();
    let Ok(mut visibility) = panel.single_mut() else { return };
    visibility.set_if_neq(if entries.is_empty() { Visibility::Hidden } else { Visibility::Inherited });
    if entries.is_empty() {
        return;
    }
    let (wanted_title, wanted_details) = describe(&entries);
    // Une seule entité adverse : son nom en rouge.
    let enemy = entries.len() == 1 && selected.iter().any(|(_, owner, ..)| owner.is_some_and(|o| o.0 != local.0));
    if let Ok((mut text, mut color)) = title.single_mut() {
        if text.0 != wanted_title {
            text.0 = wanted_title;
        }
        color.set_if_neq(TextColor(if enemy { ENEMY_TEXT } else { TEXT }));
    }
    if let Ok(mut text) = details.single_mut()
        && text.0 != wanted_details
    {
        text.0 = wanted_details;
    }
}

fn update_hover_label(
    pointer: Res<Pointer>,
    local: Res<LocalPlayer>,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    pickable: Pickable,
    names: Query<(&Name, Option<&Owner>)>,
    mut label: Query<(&mut Node, &mut Visibility, &Children), With<HoverLabel>>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let (camera, camera_transform) = camera.into_inner();
    let Ok((mut node, mut visibility, children)) = label.single_mut() else { return };
    let hovered = pointer
        .position
        .and_then(|p| Some((p, screen_ray(camera, camera_transform, p)?)))
        .and_then(|(p, ray)| Some((p, pick(ray, &pickable)?)))
        .and_then(|(p, entity)| Some((p, names.get(entity).ok()?)));
    let Some((position, (name, owner))) = hovered else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    // Un peu en dessous à droite du pointeur, pour ne pas cacher l'entité.
    node.left = Val::Px(position.x + 16.0);
    node.top = Val::Px(position.y + 18.0);
    let enemy = owner.is_some_and(|o| o.0 != local.0);
    if let Some(&child) = children.first()
        && let Ok((mut text, mut color)) = texts.get_mut(child)
    {
        if text.0 != name.as_str() {
            text.0 = name.as_str().to_string();
        }
        color.set_if_neq(TextColor(if enemy { ENEMY_TEXT } else { TEXT }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_entity_shows_its_name_and_health() {
        let health = Health { current: 32.4, max: 50.0 };
        let (title, details) = describe(&[("Sentinelle", Some(&health), None)]);
        assert_eq!(title, "Sentinelle");
        assert_eq!(details, "PV 33 / 50");
        let (_, site) = describe(&[("Caserne", Some(&health), Some(0.4))]);
        assert_eq!(site, "PV 33 / 50 · en construction : 40 %");
    }

    #[test]
    fn several_entities_are_counted_by_type() {
        let entries = [("Lancier", None, None), ("Sentinelle", None, None), ("Sentinelle", None, None), ("Bastion", None, None)];
        let (title, details) = describe(&entries);
        assert_eq!(title, "4 sélectionnés");
        assert_eq!(details, "Sentinelle ×2 · Bastion · Lancier");
    }
}
