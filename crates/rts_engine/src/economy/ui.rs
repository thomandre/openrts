//! Interface de l'économie : traduction des boutons de production en
//! ordres, compteurs de ressources, panneau de production, messages.
//!
//! Comme pour la construction, rien ici ne change l'état du jeu : les
//! actions ([`GameAction`]) deviennent des ordres ([`PlayerCommand`]) dans
//! la file, que la simulation vérifie et applique au pas suivant.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::production::{ProductionQueue, RallyPoint, QUEUE_LIMIT};
use super::{Cost, EconomyRules, Notice, ResourceKind};
use crate::buildings::ui::{key_label, ui_font, BORDER, BUTTON, DISABLED_TEXT, KEY_TEXT, PANEL, TEXT};
use crate::buildings::{completed_types, Building, BuildingCatalog, UnderConstruction};
use crate::input::{ActionButton, BlocksPointer, GameAction, Hotkey};
use crate::orders::{CommandQueue, PlayerCommand};
use crate::players::{LocalPlayer, Owner, Players};
use crate::selection::Selected;
use crate::terrain::Terrain;
use crate::units::{Unit, UnitCatalog};

/// Les bâtiments producteurs terminés du joueur local dans la sélection,
/// triés par identifiant. Le premier est celui dont le panneau est affiché.
pub(crate) fn selected_producers<'a>(
    local: LocalPlayer,
    producers: impl Iterator<Item = (Entity, &'a Owner, &'a ProductionQueue)>,
) -> Vec<Entity> {
    let mut list: Vec<Entity> = producers.filter(|(_, owner, _)| owner.0 == local.0).map(|(e, ..)| e).collect();
    list.sort_unstable();
    list
}

/// Consomme les actions de production et d'annulation.
pub(crate) fn handle_production_actions(
    mut actions: MessageReader<GameAction>,
    local: Res<LocalPlayer>,
    catalog: Option<Res<BuildingCatalog>>,
    producers: Query<(Entity, &Owner, &ProductionQueue, &Building), (With<Selected>, Without<UnderConstruction>)>,
    sites: Query<(Entity, &Owner), (With<Selected>, With<UnderConstruction>)>,
    mut orders: ResMut<CommandQueue>,
) {
    let Some(catalog) = catalog else {
        actions.clear();
        return;
    };
    for action in actions.read() {
        match *action {
            GameAction::Train { unit_type } => {
                // Parmi les bâtiments sélectionnés qui produisent cette unité :
                // la file la plus courte, puis le plus petit identifiant.
                let best = producers
                    .iter()
                    .filter(|(_, owner, _, building)| {
                        owner.0 == local.0 && catalog.types[building.type_index].production.iter().any(|o| o.unit_type == unit_type)
                    })
                    .min_by_key(|(entity, _, queue, _)| (queue.items.len(), *entity));
                if let Some((building, ..)) = best {
                    orders.push(local.0, PlayerCommand::Train { building, unit_type });
                }
            }
            GameAction::CancelQueued { slot } => {
                let shown = selected_producers(*local, producers.iter().map(|(e, o, q, _)| (e, o, q)));
                if let Some(&building) = shown.first() {
                    orders.push(local.0, PlayerCommand::CancelTrain { building, slot });
                }
            }
            GameAction::CancelConstruction => {
                let mut list: Vec<Entity> = sites.iter().filter(|(_, o)| o.0 == local.0).map(|(e, _)| e).collect();
                list.sort_unstable();
                for building in list {
                    orders.push(local.0, PlayerCommand::CancelConstruction { building });
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Affichage.
// ---------------------------------------------------------------------------

/// Texte d'un coût : « 100 Cristal + 25 Éther » (noms des ressources du jeu).
pub fn format_cost(cost: Cost, rules: Option<&EconomyRules>) -> String {
    let name = |kind: ResourceKind, fallback: &str| rules.map_or_else(|| fallback.to_string(), |r| r.resource(kind).name.clone());
    let mut text = format!("{} {}", cost.primary, name(ResourceKind::Primary, "P"));
    if cost.rare > 0 {
        text += &format!(" + {} {}", cost.rare, name(ResourceKind::Rare, "R"));
    }
    text
}

/// Un compteur en haut à droite : 0 et 1 pour les ressources, 2 pour la population.
#[derive(Component, Debug)]
pub(crate) struct CounterText(usize);

/// Le panneau de production (en bas, au centre) et le type de bâtiment
/// dont il montre les boutons.
#[derive(Component, Debug, Default)]
pub(crate) struct ProductionPanel {
    shown_type: Option<usize>,
}

#[derive(Component, Debug)]
pub(crate) struct ProductionTitle;

/// La rangée des boutons de production (reconstruite quand le type change).
#[derive(Component, Debug)]
pub(crate) struct OptionsRow;

/// Un bouton de production, et l'unité qu'il produit.
#[derive(Component, Debug)]
pub(crate) struct TrainButton {
    type_index: usize,
    option: usize,
}

/// Une case de la file de production (clic : annuler).
#[derive(Component, Debug)]
pub(crate) struct QueueSlot(usize);

#[derive(Component, Debug)]
pub(crate) struct SlotText;

/// La barre de progression d'une case de la file.
#[derive(Component, Debug)]
pub(crate) struct SlotFill;

/// Le panneau d'un chantier sélectionné (avancement, annulation).
#[derive(Component, Debug)]
pub(crate) struct SitePanel;

#[derive(Component, Debug)]
pub(crate) struct SiteText;

/// Le message court affiché en haut de l'écran.
#[derive(Component, Debug)]
pub(crate) struct NoticeText;

/// Le repère du point de ralliement, au sol.
#[derive(Component, Debug)]
pub(crate) struct RallyMarker;

/// Couleur des coûts qu'on ne peut pas payer.
const UNAFFORDABLE: Color = Color::srgb(0.95, 0.42, 0.35);
/// Couleur des messages.
const NOTICE: Color = Color::srgb(0.98, 0.76, 0.35);
/// Durée d'affichage d'un message, en secondes.
const NOTICE_SECONDS: f32 = 2.5;

/// Crée une fois les éléments fixes : compteurs, panneaux, message, repère.
pub(crate) fn spawn_economy_ui(
    mut commands: Commands,
    rules: Option<Res<EconomyRules>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut done: Local<bool>,
) {
    // Le jeu fournit ses règles au démarrage ; sans elles, pas d'économie.
    let Some(rules) = rules else { return };
    if *done {
        return;
    }
    *done = true;

    // --- Compteurs, en haut à droite ---
    commands
        .spawn((
            Name::new("Compteurs"),
            BlocksPointer,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                right: Val::Px(14.0),
                column_gap: Val::Px(18.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(BORDER),
        ))
        .with_children(|bar| {
            let colors = [rules.resources[0].color, rules.resources[1].color, TEXT];
            for (i, color) in colors.into_iter().enumerate() {
                bar.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), ..default() }).with_children(|item| {
                    // Une pastille de la couleur de la ressource (la population : un losange clair).
                    item.spawn((
                        Node { width: Val::Px(10.0), height: Val::Px(10.0), ..default() },
                        BackgroundColor(color),
                        UiTransform::from_rotation(Rot2::degrees(if i == 2 { 45.0 } else { 0.0 })),
                    ));
                    item.spawn((CounterText(i), Text::new(""), ui_font(16.0), TextColor(TEXT)));
                });
            }
        });

    // --- Message court, en haut au centre ---
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(64.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|root| {
            root.spawn((NoticeText, Text::new(""), ui_font(20.0), TextColor(NOTICE), Visibility::Hidden));
        });

    // --- Panneau de production, en bas au centre ---
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(14.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|root| {
            root.spawn((
                ProductionPanel::default(),
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
                panel.spawn((ProductionTitle, Text::new(""), ui_font(14.0), TextColor(KEY_TEXT)));
                panel.spawn((OptionsRow, Node { column_gap: Val::Px(6.0), ..default() }));
                panel
                    .spawn(Node { column_gap: Val::Px(4.0), align_items: AlignItems::Center, ..default() })
                    .with_children(|row| {
                        row.spawn((Text::new("File"), ui_font(12.0), TextColor(DISABLED_TEXT), Node { width: Val::Px(34.0), ..default() }));
                        for slot in 0..QUEUE_LIMIT {
                            row.spawn((
                                QueueSlot(slot),
                                ActionButton { action: GameAction::CancelQueued { slot }, enabled: true, hotkey: None },
                                Node {
                                    width: Val::Px(92.0),
                                    height: Val::Px(34.0),
                                    flex_direction: FlexDirection::Column,
                                    justify_content: JustifyContent::SpaceBetween,
                                    padding: UiRect::all(Val::Px(4.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BackgroundColor(BUTTON),
                                BorderColor::all(BORDER.with_alpha(0.35)),
                            ))
                            .with_children(|slot_node| {
                                slot_node.spawn((SlotText, Text::new(""), ui_font(12.0), TextColor(TEXT)));
                                slot_node
                                    .spawn((Node { height: Val::Px(4.0), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08))))
                                    .with_children(|bar| {
                                        bar.spawn((
                                            SlotFill,
                                            Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() },
                                            BackgroundColor(KEY_TEXT),
                                        ));
                                    });
                            });
                        }
                    });
            });
        });

    // --- Panneau de chantier ---
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(14.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|root| {
            root.spawn((
                SitePanel,
                BlocksPointer,
                Node {
                    padding: UiRect::all(Val::Px(8.0)),
                    column_gap: Val::Px(12.0),
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(BORDER),
                Visibility::Hidden,
            ))
            .with_children(|panel| {
                panel.spawn((SiteText, Text::new(""), ui_font(14.0), TextColor(TEXT)));
                panel
                    .spawn((
                        ActionButton { action: GameAction::CancelConstruction, enabled: true, hotkey: Some(Hotkey { menu: None, key: 'x' }) },
                        Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)), column_gap: Val::Px(6.0), border: UiRect::all(Val::Px(1.0)), ..default() },
                        BackgroundColor(BUTTON),
                        BorderColor::all(BORDER),
                    ))
                    .with_children(|button| {
                        button.spawn((Text::new("X"), ui_font(14.0), TextColor(KEY_TEXT)));
                        button.spawn((Text::new("Annuler (remboursé)"), ui_font(14.0), TextColor(TEXT)));
                    });
            });
        });

    // --- Repère du point de ralliement : mât, fanion et anneau au sol ---
    let glow = materials.add(StandardMaterial { base_color: Color::BLACK, emissive: KEY_TEXT.to_linear() * 3.0, ..default() });
    commands
        .spawn((Name::new("Point de ralliement"), RallyMarker, Transform::default(), Visibility::Hidden))
        .with_children(|marker| {
            marker.spawn((Mesh3d(meshes.add(Cylinder::new(0.06, 2.4))), MeshMaterial3d(glow.clone()), Transform::from_xyz(0.0, 1.2, 0.0), NotShadowCaster));
            marker.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.8, 0.45, 0.04))),
                MeshMaterial3d(glow.clone()),
                Transform::from_xyz(0.43, 2.15, 0.0),
                NotShadowCaster,
            ));
            marker.spawn((
                Mesh3d(meshes.add(Annulus::new(0.6, 0.8))),
                MeshMaterial3d(glow),
                Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                NotShadowCaster,
            ));
        });
}

/// Compteurs : stocks et population du joueur local.
pub(crate) fn update_counters(
    local: Res<LocalPlayer>,
    players: Res<Players>,
    rules: Option<Res<EconomyRules>>,
    mut texts: Query<(&CounterText, &mut Text, &mut TextColor)>,
) {
    let Some(rules) = rules else { return };
    let state = players.get(local.0);
    for (counter, mut text, mut color) in &mut texts {
        let wanted = match counter.0 {
            0 => format!("{} {}", rules.resources[0].name, state.stock.primary),
            1 => format!("{} {}", rules.resources[1].name, state.stock.rare),
            _ => format!("Population {} / {}", state.population, state.population_cap),
        };
        if text.0 != wanted {
            text.0 = wanted;
        }
        // La population vire au rouge quand elle est pleine.
        let full = counter.0 == 2 && state.population >= state.population_cap;
        color.set_if_neq(TextColor(if full { UNAFFORDABLE } else { TEXT }));
    }
}

/// Panneau de production du bâtiment producteur sélectionné : boutons
/// (grisés s'il manque un prérequis), file et progression, point de
/// ralliement. Caché quand un ouvrier est sélectionné (la barre de
/// construction prend la place).
pub(crate) fn update_production_panel(
    mut commands: Commands,
    local: Res<LocalPlayer>,
    players: Res<Players>,
    rules: Option<Res<EconomyRules>>,
    catalog: Option<Res<BuildingCatalog>>,
    units: Option<Res<UnitCatalog>>,
    selected_units: Query<(&Unit, &Owner), With<Selected>>,
    producers: Query<
        (Entity, &Owner, &ProductionQueue, &Building),
        (With<Selected>, Without<UnderConstruction>),
    >,
    all_buildings: Query<(&Building, &Owner, Has<UnderConstruction>)>,
    mut panel: Query<(&mut ProductionPanel, &mut Visibility)>,
    mut title: Query<&mut Text, (With<ProductionTitle>, Without<SlotText>)>,
    rows: Query<Entity, With<OptionsRow>>,
    mut train_buttons: Query<(&TrainButton, &mut ActionButton, &mut BorderColor, &Children), Without<QueueSlot>>,
    // Un n-uplet de paramètres compte pour un seul : Bevy limite le nombre
    // de paramètres d'un système (16).
    (mut slots, mut slot_texts, mut fills, bar_children): (
        Query<(&QueueSlot, &mut BorderColor, &Children), Without<TrainButton>>,
        Query<&mut Text, (With<SlotText>, Without<ProductionTitle>)>,
        Query<&mut Node, With<SlotFill>>,
        Query<&Children>,
    ),
    mut colors: Query<&mut TextColor>,
) {
    let (Some(rules), Some(catalog), Some(units)) = (rules, catalog, units) else { return };
    let Ok((mut panel, mut visibility)) = panel.single_mut() else { return };
    let shown = selected_producers(*local, producers.iter().map(|(e, o, q, ..)| (e, o, q))).first().copied();
    let has_builder = selected_units.iter().any(|(u, o)| o.0 == local.0 && catalog.builders.contains(&u.type_index));
    let shown = shown.filter(|_| !has_builder).and_then(|e| producers.get(e).ok());

    let Some((_, _, queue, building)) = shown else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    let building_type = &catalog.types[building.type_index];

    // Nouveau type de bâtiment : on reconstruit la rangée de boutons.
    if panel.shown_type != Some(building.type_index) {
        panel.shown_type = Some(building.type_index);
        if let Ok(mut title) = title.single_mut() {
            title.0 = format!("{} : production", building_type.name);
        }
        if let Ok(row) = rows.single() {
            commands.entity(row).despawn_children().with_children(|row| {
                for (i, option) in building_type.production.iter().enumerate() {
                    let unit_name = units.types.get(option.unit_type).map_or_else(String::new, |t| t.name.clone());
                    let requires: Vec<String> = option.requires.iter().map(|r| r.label(&catalog)).collect();
                    row.spawn((
                        TrainButton { type_index: building.type_index, option: i },
                        ActionButton {
                            action: GameAction::Train { unit_type: option.unit_type },
                            enabled: true,
                            hotkey: Some(Hotkey { menu: None, key: option.hotkey }),
                        },
                        Node {
                            width: Val::Px(138.0),
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
                        let economy = rules.unit(option.unit_type);
                        button.spawn((Text::new(key_label(option.hotkey)), ui_font(16.0), TextColor(KEY_TEXT)));
                        button.spawn((Text::new(unit_name), ui_font(14.0), TextColor(TEXT)));
                        button.spawn((
                            Text::new(format!("{} · {} pop. · {} s", format_cost(economy.cost, Some(&rules)), economy.population, economy.train_time)),
                            ui_font(11.0),
                            TextColor(TEXT),
                        ));
                        if !requires.is_empty() {
                            button.spawn((Text::new(format!("Demande : {}", requires.join(", "))), ui_font(11.0), TextColor(DISABLED_TEXT)));
                        }
                    });
                }
            });
        }
    }

    // État des boutons : prérequis (grisé) et coût (rouge si trop cher).
    let completed = completed_types(&catalog, local.0, all_buildings.iter());
    let state = players.get(local.0);
    for (button, mut action, mut border, children) in &mut train_buttons {
        let Some(option) = catalog.types.get(button.type_index).and_then(|t| t.production.get(button.option)) else { continue };
        let enabled = option.requires.iter().all(|r| r.met(&completed));
        action.enabled = enabled;
        border.set_if_neq(BorderColor::all(BORDER.with_alpha(if enabled { 1.0 } else { 0.35 })));
        let affordable = state.stock.covers(rules.unit(option.unit_type).cost);
        for (i, child) in children.iter().enumerate() {
            if let Ok(mut color) = colors.get_mut(child) {
                let wanted = match (enabled, i) {
                    (false, _) => DISABLED_TEXT,
                    (true, 0) => KEY_TEXT,
                    (true, 2) if !affordable => UNAFFORDABLE,
                    (true, _) => TEXT,
                };
                color.set_if_neq(TextColor(wanted));
            }
        }
    }

    // La file : nom de chaque unité, progression de la première.
    let progress = queue.progress(&rules);
    for (slot, mut border, children) in &mut slots {
        let item = queue.items.get(slot.0);
        border.set_if_neq(BorderColor::all(BORDER.with_alpha(if item.is_some() { 1.0 } else { 0.3 })));
        for child in children.iter() {
            if let Ok(mut text) = slot_texts.get_mut(child) {
                let wanted = item.and_then(|i| units.types.get(i.unit_type)).map_or_else(String::new, |t| t.name.clone());
                if text.0 != wanted {
                    text.0 = wanted;
                }
            }
            for grandchild in bar_children.get(child).into_iter().flatten() {
                if let Ok(mut fill) = fills.get_mut(*grandchild) {
                    let wanted = if slot.0 == 0 && item.is_some() { Val::Percent(progress * 100.0) } else { Val::Percent(0.0) };
                    if fill.width != wanted {
                        fill.width = wanted;
                    }
                }
            }
        }
    }
}

/// Repère du point de ralliement du bâtiment producteur affiché, et une
/// ligne du bâtiment jusqu'à lui (`Gizmos` : dessin immédiat de lignes,
/// refait à chaque image).
pub(crate) fn update_rally_marker(
    local: Res<LocalPlayer>,
    producers: Query<(Entity, &Owner, &ProductionQueue, &GlobalTransform, Option<&RallyPoint>), With<Selected>>,
    mut marker: Query<(&mut Transform, &mut Visibility), With<RallyMarker>>,
    mut gizmos: Gizmos,
    terrain: Res<Terrain>,
) {
    let Ok((mut transform, mut visibility)) = marker.single_mut() else { return };
    let shown = selected_producers(*local, producers.iter().map(|(e, o, q, ..)| (e, o, q))).first().copied();
    match shown.and_then(|e| producers.get(e).ok()).and_then(|(.., t, rally)| rally.map(|r| (t, r))) {
        Some((building, rally)) => {
            transform.translation = Vec3::new(rally.point.x, terrain.height_at(rally.point), rally.point.y);
            visibility.set_if_neq(Visibility::Inherited);
            let from = building.translation();
            let from = from.with_y(terrain.height_at(from.xz()) + 0.15);
            gizmos.line(from, transform.translation + Vec3::Y * 0.15, KEY_TEXT.with_alpha(0.7));
        }
        None => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Panneau d'un chantier sélectionné : avancement et bouton d'annulation.
pub(crate) fn update_site_panel(
    local: Res<LocalPlayer>,
    catalog: Option<Res<BuildingCatalog>>,
    selected_units: Query<(), (With<Selected>, With<Unit>)>,
    sites: Query<(Entity, &Owner, &Building, &UnderConstruction), With<Selected>>,
    mut panel: Query<&mut Visibility, With<SitePanel>>,
    mut text: Query<&mut Text, With<SiteText>>,
) {
    let Some(catalog) = catalog else { return };
    let Ok(mut visibility) = panel.single_mut() else { return };
    let mut mine: Vec<_> = sites.iter().filter(|(_, o, ..)| o.0 == local.0).collect();
    mine.sort_unstable_by_key(|s| s.0);
    let Some((_, _, building, construction)) = mine.first().copied().filter(|_| selected_units.is_empty()) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    if let Ok(mut text) = text.single_mut() {
        let wanted = format!("Chantier : {} — {:.0} %", catalog.types[building.type_index].name, construction.progress * 100.0);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

/// Affiche le dernier message du joueur local pendant quelques secondes.
pub(crate) fn show_notices(
    time: Res<Time<Real>>,
    local: Res<LocalPlayer>,
    mut notices: MessageReader<Notice>,
    mut text: Query<(&mut Text, &mut TextColor, &mut Visibility), With<NoticeText>>,
    mut shown_at: Local<Option<f32>>,
) {
    let Ok((mut text, mut color, mut visibility)) = text.single_mut() else { return };
    let now = time.elapsed_secs();
    if let Some(notice) = notices.read().filter(|n| n.player == local.0).last() {
        text.0 = notice.kind.text().to_string();
        *shown_at = Some(now);
    }
    match *shown_at {
        Some(start) if now - start < NOTICE_SECONDS => {
            // Fondu sur la dernière demi-seconde.
            let alpha = ((NOTICE_SECONDS - (now - start)) / 0.5).min(1.0);
            color.set_if_neq(TextColor(NOTICE.with_alpha(alpha)));
            visibility.set_if_neq(Visibility::Inherited);
        }
        _ => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}
