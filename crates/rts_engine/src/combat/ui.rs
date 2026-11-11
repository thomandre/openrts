//! Interface du combat : la barre d'ordres et le mode visée.
//!
//! La barre d'ordres (en bas à gauche) apparaît quand le joueur a
//! sélectionné de ses unités. Ses boutons sont des [`ActionButton`] : un
//! clic, un tap ou leur raccourci (A, M, S, H) produisent la même
//! [`GameAction`]. « Attaquer » et « Déplacer » ouvrent le **mode visée** :
//! le prochain clic (ou tap) sur le sol ou sur un ennemi donne l'ordre.
//! C'est ce qui rend l'attaque-déplacement jouable au doigt, sans clavier.
//!
//! Rien ici ne touche à l'état du jeu : les ordres partent dans la file
//! (voir `selection`), ce module ne tient que l'état de l'interface.

use bevy::prelude::*;

use crate::buildings::ui::{key_label, ui_font, BORDER, BUTTON, BUTTON_ACTIVE, KEY_TEXT, PANEL, TEXT};
use crate::cursor::Pointer;
use crate::input::{ActionButton, BlocksPointer, GameAction, Hotkey, InputContext, TargetedOrder};
use crate::players::{LocalPlayer, Owner};
use crate::selection::Selected;
use crate::simulation::MoveSpeed;
use crate::RtsSystems;

/// Marqueur : le panneau de la barre d'ordres.
#[derive(Component, Debug)]
struct OrderBar;

/// Un bouton de la barre d'ordres, et le mode visée qu'il ouvre (s'il en ouvre un).
#[derive(Component, Debug)]
struct OrderButton {
    targeting: Option<TargetedOrder>,
}

/// Le réticule qui suit le pointeur en mode visée (rouge : attaque).
#[derive(Component, Debug)]
struct TargetingReticle;

/// Couleur du mode attaque (réticule, repère au sol).
pub(crate) const ATTACK_COLOR: Color = Color::srgb(1.0, 0.22, 0.16);
/// Couleur du déplacement simple.
pub(crate) const MOVE_COLOR: Color = Color::srgb(0.3, 1.0, 0.45);

pub struct CombatUiPlugin;

impl Plugin for CombatUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_order_bar, spawn_reticle)).add_systems(
            Update,
            (handle_order_actions, update_order_bar, update_reticle).chain().in_set(RtsSystems::Actions),
        );
    }
}

/// Tient à jour le mode visée : ouvert par « Attaquer » ou « Déplacer »,
/// fermé quand l'ordre est donné, annulé, ou qu'il n'y a plus d'unité à commander.
fn handle_order_actions(
    mut actions: MessageReader<GameAction>,
    mut context: ResMut<InputContext>,
    local: Res<LocalPlayer>,
    selected: Query<&Owner, (With<Selected>, With<MoveSpeed>)>,
) {
    for action in actions.read() {
        match *action {
            GameAction::ChooseOrder { order } => context.targeting = Some(order),
            GameAction::OrderAt { keep, .. } if !keep => context.targeting = None,
            GameAction::CancelOrder | GameAction::Stop | GameAction::HoldPosition => context.targeting = None,
            // Le mode pose d'un bâtiment remplace le mode visée.
            GameAction::ChooseBuilding { .. } => context.targeting = None,
            _ => {}
        }
    }
    if context.targeting.is_some() && !selected.iter().any(|o| o.0 == local.0) {
        context.targeting = None;
    }
}

fn spawn_order_bar(mut commands: Commands) {
    let buttons = [
        ("Attaquer", 'a', GameAction::ChooseOrder { order: TargetedOrder::AttackMove }, Some(TargetedOrder::AttackMove)),
        ("Déplacer", 'm', GameAction::ChooseOrder { order: TargetedOrder::Move }, Some(TargetedOrder::Move)),
        ("Stop", 's', GameAction::Stop, None),
        ("Tenir", 'h', GameAction::HoldPosition, None),
    ];
    commands
        .spawn((
            Name::new("Barre d'ordres"),
            OrderBar,
            BlocksPointer,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(14.0),
                left: Val::Px(14.0),
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
            panel.spawn((Text::new("Ordres"), ui_font(14.0), TextColor(KEY_TEXT)));
            panel
                .spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(6.0), ..default() })
                .with_children(|row| {
                    for (label, key, action, targeting) in buttons {
                        row.spawn((
                            OrderButton { targeting },
                            ActionButton { action, enabled: true, hotkey: Some(Hotkey { menu: None, key }) },
                            Node {
                                width: Val::Px(84.0),
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
                            button.spawn((Text::new(key_label(key)), ui_font(16.0), TextColor(KEY_TEXT)));
                            button.spawn((Text::new(label), ui_font(14.0), TextColor(TEXT)));
                        });
                    }
                });
        });
}

/// Montre la barre quand le joueur a sélectionné de ses unités, et
/// éclaire le bouton du mode visée en cours.
fn update_order_bar(
    context: Res<InputContext>,
    local: Res<LocalPlayer>,
    selected: Query<&Owner, (With<Selected>, With<MoveSpeed>)>,
    mut bar: Query<&mut Visibility, With<OrderBar>>,
    mut buttons: Query<(&OrderButton, &mut BackgroundColor, &mut BorderColor)>,
) {
    let Ok(mut visibility) = bar.single_mut() else { return };
    let shown = selected.iter().any(|o| o.0 == local.0);
    visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    for (button, mut background, mut border) in &mut buttons {
        let active = button.targeting.is_some() && button.targeting == context.targeting;
        background.set_if_neq(BackgroundColor(if active { BUTTON_ACTIVE } else { BUTTON }));
        let edge = match (active, button.targeting) {
            (true, Some(TargetedOrder::AttackMove)) => ATTACK_COLOR,
            (true, _) => MOVE_COLOR,
            _ => BORDER,
        };
        border.set_if_neq(BorderColor::all(edge));
    }
}

fn spawn_reticle(mut commands: Commands) {
    // Un réticule : quatre traits autour d'un centre vide.
    commands
        .spawn((
            Name::new("Réticule"),
            TargetingReticle,
            Node { position_type: PositionType::Absolute, width: Val::Px(28.0), height: Val::Px(28.0), ..default() },
            GlobalZIndex(i32::MAX - 1),
            Visibility::Hidden,
        ))
        .with_children(|reticle| {
            for (left, top, width, height) in [(12.0, 0.0, 4.0, 9.0), (12.0, 19.0, 4.0, 9.0), (0.0, 12.0, 9.0, 4.0), (19.0, 12.0, 9.0, 4.0)] {
                reticle.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(left),
                        top: Val::Px(top),
                        width: Val::Px(width),
                        height: Val::Px(height),
                        ..default()
                    },
                    BackgroundColor(ATTACK_COLOR),
                ));
            }
        });
}

/// En mode visée, le réticule suit le pointeur, à la couleur de l'ordre.
fn update_reticle(
    context: Res<InputContext>,
    pointer: Res<Pointer>,
    reticle: Single<(&mut Node, &mut Visibility, &Children), With<TargetingReticle>>,
    mut strokes: Query<&mut BackgroundColor>,
) {
    let (mut node, mut visibility, children) = reticle.into_inner();
    match (context.targeting, pointer.position) {
        (Some(order), Some(pos)) => {
            node.left = Val::Px(pos.x - 14.0);
            node.top = Val::Px(pos.y - 14.0);
            visibility.set_if_neq(Visibility::Inherited);
            let color = if order == TargetedOrder::AttackMove { ATTACK_COLOR } else { MOVE_COLOR };
            for child in children.iter() {
                if let Ok(mut stroke) = strokes.get_mut(child) {
                    stroke.set_if_neq(BackgroundColor(color));
                }
            }
        }
        _ => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}
