//! Affichage des bâtiments : barre de construction en bas de l'écran, barres
//! de progression des chantiers, bâtiment qui monte du sol.
//!
//! Rien ici ne touche à l'état du jeu : ces systèmes lisent la simulation et
//! l'interface, et les boutons ne font que produire des actions (voir
//! [`ActionButton`]).

use bevy::prelude::*;
use bevy::text::{FontSource, GenericFontFamily};

use super::{completed_types, Building, BuildingBody, BuildingCatalog, PlannedMarker, PlannedSite, SiteFrame, UnderConstruction};
use super::placement::Placement;
use crate::camera::RtsCamera;
use crate::economy::ui::format_cost;
use crate::economy::EconomyRules;
use crate::navigation::NavGrid;
use crate::input::{ActionButton, BlocksPointer, GameAction, Hotkey, InputContext};
use crate::players::{LocalPlayer, Owner, Players};
use crate::selection::Selected;
use crate::units::Unit;

/// Marqueur : le panneau des boutons de construction.
#[derive(Component, Debug)]
pub(crate) struct CommandBar;

/// Le titre du panneau (il change quand le menu « B » est ouvert).
#[derive(Component, Debug)]
pub(crate) struct CommandBarTitle;

/// Un bouton de construction, et le type de bâtiment qu'il pose.
#[derive(Component, Debug)]
pub(crate) struct BuildButton {
    type_index: usize,
}

/// Barre de progression d'un chantier, à l'écran au-dessus de lui.
#[derive(Component, Debug)]
pub(crate) struct ProgressBar {
    building: Entity,
}

/// La partie remplie d'une barre de progression.
#[derive(Component, Debug)]
pub(crate) struct ProgressFill;

pub(crate) const PANEL: Color = Color::srgba(0.03, 0.05, 0.08, 0.86);
pub(crate) const BORDER: Color = Color::srgb(0.18, 0.55, 0.62);
pub(crate) const BUTTON: Color = Color::srgba(0.09, 0.13, 0.18, 0.95);
pub(crate) const BUTTON_ACTIVE: Color = Color::srgba(0.12, 0.3, 0.36, 0.95);
pub(crate) const TEXT: Color = Color::srgb(0.88, 0.92, 0.95);
pub(crate) const KEY_TEXT: Color = Color::srgb(0.31, 0.82, 0.88);
pub(crate) const DISABLED_TEXT: Color = Color::srgb(0.38, 0.4, 0.43);

/// Police de l'interface, à la taille `size`. Police sans empattement du
/// système : contrairement à la police intégrée de Bevy, elle a les lettres
/// accentuées (« Quartier général »).
pub(crate) fn ui_font(size: f32) -> TextFont {
    TextFont { font: FontSource::Generic(GenericFontFamily::SansSerif), ..TextFont::from_font_size(size) }
}

/// Lettre affichée d'un raccourci.
pub(crate) fn key_label(c: char) -> String {
    c.to_uppercase().collect()
}

/// Crée le panneau de construction (une fois), le montre quand un ouvrier
/// est sélectionné, et grise les bâtiments dont il manque un prérequis.
pub(crate) fn update_command_bar(
    mut commands: Commands,
    catalog: Option<Res<BuildingCatalog>>,
    rules: Option<Res<EconomyRules>>,
    context: Res<InputContext>,
    placement: Res<Placement>,
    local: Res<LocalPlayer>,
    players: Res<Players>,
    selected: Query<(&Unit, &Owner), With<Selected>>,
    buildings: Query<(&Building, &Owner, Has<UnderConstruction>)>,
    mut bar: Query<&mut Visibility, With<CommandBar>>,
    mut title: Query<&mut Text, With<CommandBarTitle>>,
    mut buttons: Query<(&BuildButton, &mut ActionButton, &mut BackgroundColor, &mut BorderColor, &Children)>,
    mut texts: Query<&mut TextColor>,
) {
    let Some(catalog) = catalog else { return };
    let Ok(mut visibility) = bar.single_mut() else {
        spawn_command_bar(&mut commands, &catalog, rules.as_deref(), players.get(local.0).faction);
        return;
    };
    let has_builder = selected.iter().any(|(unit, owner)| catalog.builders.contains(&unit.type_index) && owner.0 == local.0);
    visibility.set_if_neq(if has_builder { Visibility::Inherited } else { Visibility::Hidden });
    if !has_builder {
        return;
    }
    let menu_open = context.key_menu == Some(catalog.menu_key);
    if let Ok(mut title) = title.single_mut() {
        let wanted = if menu_open {
            format!("Construire : touche du bâtiment ({} : retour)", "Échap")
        } else {
            format!("Construire ({})", key_label(catalog.menu_key))
        };
        if title.0 != wanted {
            title.0 = wanted;
        }
    }
    let completed = completed_types(&catalog, local.0, buildings.iter());
    for (button, mut action, mut background, mut border, children) in &mut buttons {
        let enabled = catalog.requirements_met(button.type_index, &completed);
        action.enabled = enabled;
        let active = placement.active == Some(button.type_index);
        background.set_if_neq(BackgroundColor(if active { BUTTON_ACTIVE } else { BUTTON }));
        let edge = if active || (menu_open && enabled) { KEY_TEXT } else { BORDER.with_alpha(if enabled { 1.0 } else { 0.35 }) };
        border.set_if_neq(BorderColor::all(edge));
        for (i, child) in children.iter().enumerate() {
            if let Ok(mut color) = texts.get_mut(child) {
                let wanted = match (enabled, i) {
                    (false, _) => DISABLED_TEXT,
                    (true, 0) => KEY_TEXT,
                    (true, _) => TEXT,
                };
                color.set_if_neq(TextColor(wanted));
            }
        }
    }
}

/// La barre ne montre que les bâtiments de la faction `faction` (celle du
/// joueur local) : leurs raccourcis ne se mélangent pas à ceux des autres.
fn spawn_command_bar(commands: &mut Commands, catalog: &BuildingCatalog, rules: Option<&EconomyRules>, faction: usize) {
    let font = ui_font;
    // Conteneur pleine largeur (transparent, il ne bloque pas les clics) qui
    // centre le panneau en bas de l'écran.
    commands
        .spawn((
            Name::new("Barre de construction"),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(14.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                CommandBar,
                BlocksPointer,
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(8.0)),
                    row_gap: Val::Px(6.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(BORDER),
                Visibility::Hidden,
            ))
            .with_children(|panel| {
                panel.spawn((CommandBarTitle, Text::new(""), font(14.0), TextColor(KEY_TEXT)));
                panel
                    .spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), ..default() })
                    .with_children(|row| {
                        let own = catalog.types.iter().enumerate().filter(|(i, _)| catalog.faction_of(*i) == faction);
                        for (type_index, building) in own {
                            let cost = format_cost(building.cost, rules);
                            row.spawn((
                                BuildButton { type_index },
                                ActionButton {
                                    action: GameAction::ChooseBuilding { type_index },
                                    enabled: true,
                                    hotkey: Some(Hotkey { menu: Some(catalog.menu_key), key: building.hotkey }),
                                },
                                Node {
                                    width: Val::Px(128.0),
                                    flex_direction: FlexDirection::Column,
                                    padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    row_gap: Val::Px(2.0),
                                    ..default()
                                },
                                BackgroundColor(BUTTON),
                                BorderColor::all(BORDER),
                            ))
                            .with_children(|button| {
                                button.spawn((Text::new(key_label(building.hotkey)), font(16.0), TextColor(KEY_TEXT)));
                                button.spawn((Text::new(building.name.clone()), font(14.0), TextColor(TEXT)));
                                button.spawn((Text::new(cost), font(12.0), TextColor(TEXT)));
                            });
                        }
                    });
            });
        });
}

/// Pendant la construction, le bâtiment monte du sol : son corps est
/// enfoncé de ce qui reste à construire (le sol cache la partie enterrée).
/// Un bâtiment qui pousse seul, lui, grandit : il part du cinquième de sa
/// taille. À la fin, le cadre du chantier disparaît.
pub(crate) fn raise_buildings(
    mut commands: Commands,
    buildings: Query<(Option<&UnderConstruction>, &Children), With<Building>>,
    mut bodies: Query<(&BuildingBody, &mut Transform)>,
    frames: Query<(), With<SiteFrame>>,
) {
    for (construction, children) in &buildings {
        for child in children.iter() {
            if let Ok((body, mut transform)) = bodies.get_mut(child) {
                let progress = construction.map_or(1.0, |c| c.progress);
                if body.grows {
                    // L'échelle part du sol (l'origine du corps y est) : la
                    // pousse s'élargit et monte en même temps.
                    let scale = Vec3::splat(0.2 + 0.8 * progress);
                    if transform.scale != scale {
                        transform.scale = scale;
                    }
                } else {
                    // On montre au moins 15 % du bâtiment dès la pose (les fondations).
                    let shown = 0.15 + 0.85 * progress;
                    let y = -body.center_height - (1.0 - shown) * body.top;
                    if transform.translation.y != y {
                        transform.translation.y = y;
                    }
                }
            }
            if construction.is_none() && frames.contains(child) {
                commands.entity(child).despawn();
            }
        }
    }
}

/// Repères au sol des emplacements promis aux ouvriers du joueur local en
/// route pour s'y enraciner (style « ouvrier consommé ») : un par promesse,
/// retiré quand l'ouvrier arrive, change d'ordre ou disparaît.
pub(crate) fn show_planned_sites(
    mut commands: Commands,
    catalog: Option<Res<BuildingCatalog>>,
    grid: Option<Res<NavGrid>>,
    local: Res<LocalPlayer>,
    plans: Query<(Entity, &PlannedSite, &Owner)>,
    markers: Query<(Entity, &PlannedMarker)>,
) {
    let (Some(catalog), Some(grid)) = (catalog, grid) else { return };
    for (entity, marker) in &markers {
        if !plans.get(marker.worker).is_ok_and(|(_, plan, _)| *plan == marker.plan) {
            commands.entity(entity).despawn();
        }
    }
    for (worker, plan, owner) in &plans {
        let shown = markers.iter().any(|(_, m)| m.worker == worker && m.plan == *plan);
        if owner.0 == local.0 && !shown {
            catalog.spawn_plan_marker(&mut commands, worker, *plan, grid.frame());
        }
    }
}

/// Barres de progression des chantiers, à l'écran au-dessus de chacun.
pub(crate) fn update_progress_bars(
    mut commands: Commands,
    camera: Single<(&Camera, &GlobalTransform), With<RtsCamera>>,
    sites: Query<(Entity, &GlobalTransform, &UnderConstruction, &Children)>,
    bodies: Query<&BuildingBody>,
    mut bars: Query<(Entity, &ProgressBar, &mut Node, &mut Visibility, &Children), Without<ProgressFill>>,
    mut fills: Query<&mut Node, With<ProgressFill>>,
) {
    const WIDTH: f32 = 120.0;
    let (camera, camera_transform) = camera.into_inner();
    // Barres dont le chantier est terminé ou disparu.
    for (entity, bar, ..) in &bars {
        if !sites.contains(bar.building) {
            commands.entity(entity).despawn();
        }
    }
    for (site, transform, construction, children) in &sites {
        // Au-dessus du toit (la hauteur du modèle terminé).
        let top = children.iter().find_map(|c| bodies.get(c).ok()).map_or(3.0, |b| b.top);
        let anchor = transform.translation().with_y(top + 0.8);
        let screen = camera.world_to_viewport(camera_transform, anchor).ok();
        match bars.iter_mut().find(|(_, bar, ..)| bar.building == site) {
            Some((_, _, mut node, mut visibility, bar_children)) => {
                if let Some(pos) = screen {
                    node.left = Val::Px(pos.x - WIDTH / 2.0);
                    node.top = Val::Px(pos.y);
                }
                visibility.set_if_neq(if screen.is_some() { Visibility::Inherited } else { Visibility::Hidden });
                for child in bar_children.iter() {
                    if let Ok(mut fill) = fills.get_mut(child) {
                        fill.width = Val::Percent(construction.progress * 100.0);
                    }
                }
            }
            None => {
                let pos = screen.unwrap_or(Vec2::splat(-1000.0));
                commands
                    .spawn((
                        ProgressBar { building: site },
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(pos.x - WIDTH / 2.0),
                            top: Val::Px(pos.y),
                            width: Val::Px(WIDTH),
                            height: Val::Px(12.0),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.9)),
                        BorderColor::all(Color::srgb(0.31, 0.82, 0.88)),
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            ProgressFill,
                            Node { width: Val::Percent(construction.progress * 100.0), height: Val::Percent(100.0), ..default() },
                            BackgroundColor(Color::srgb(0.31, 0.82, 0.88)),
                        ));
                    });
            }
        }
    }
}
