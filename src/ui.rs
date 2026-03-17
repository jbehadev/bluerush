use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::grid::{GameState, PANEL_WIDTH, SelectedTool, ViewMode};
use crate::simulation::{Cell, Grid};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_ui).add_systems(
            Update,
            (
                handle_weight_buttons,
                handle_eraser_button,
                handle_spring_button,
                handle_drain_button,
                handle_building_button,
                update_tool_buttons,
                handle_inlet_toggle,
                update_inlet_button,
                handle_view_toggle,
                update_view_buttons,
                handle_reset,
                handle_speed_buttons,
                update_speed_label,
                handle_brush_buttons,
                update_brush_label,
                update_status,
            ),
        );
    }
}

#[derive(Component)]
struct WeightButton(f32);

#[derive(Component)]
struct EraserButton;

#[derive(Component)]
struct SpringButton;

#[derive(Component)]
struct DrainButton;

#[derive(Component)]
struct BuildingButton;

#[derive(Component)]
pub struct InletButton;

#[derive(Component)]
struct ResetButton;

#[derive(Component)]
struct ViewButton(ViewMode);

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct SpeedDownButton;

#[derive(Component)]
struct SpeedUpButton;

#[derive(Component)]
struct SpeedLabel;

#[derive(Component)]
struct BrushDownButton;

#[derive(Component)]
struct BrushUpButton;

#[derive(Component)]
struct BrushLabel;

fn setup_ui(mut commands: Commands) {
    // Left toolbar — SimCity-style icon panel
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Px(PANEL_WIDTH),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::new(Val::Px(0.0), Val::Px(0.0), Val::Px(12.0), Val::Px(0.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.12, 0.15)),
        ))
        .with_children(|parent| {
            // Section label
            parent.spawn((
                Text::new("OBJECTS"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.55, 0.60)),
            ));

            // 2-column icon grid for weight buttons + eraser
            parent
                .spawn((Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    column_gap: Val::Px(4.0),
                    row_gap: Val::Px(4.0),
                    padding: UiRect::horizontal(Val::Px(8.0)),
                    ..default()
                },))
                .with_children(|grid| {
                    // Weight icon buttons
                    let weights: &[(f32, f32, f32)] = &[
                        (200.0, 14.0, 0.80),
                        (500.0, 20.0, 0.65),
                        (1000.0, 26.0, 0.42),
                        (2000.0, 32.0, 0.30),
                        (5000.0, 38.0, 0.10),
                    ];
                    for &(weight, icon_size, gray) in weights {
                        let is_selected = weight == 200.0;
                        grid.spawn((
                            Button,
                            Node {
                                width: Val::Px(50.0),
                                height: Val::Px(56.0),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::FlexEnd,
                                padding: UiRect::bottom(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(if is_selected {
                                Color::srgb(0.18, 0.36, 0.65)
                            } else {
                                Color::srgb(0.55, 0.55, 0.58)
                            }),
                            WeightButton(weight),
                        ))
                        .with_children(|btn| {
                            // Icon area
                            btn.spawn((Node {
                                width: Val::Px(50.0),
                                height: Val::Px(38.0),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },))
                                .with_children(|icon| {
                                    icon.spawn((
                                        Node {
                                            width: Val::Px(icon_size),
                                            height: Val::Px(icon_size),
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgb(gray, gray, gray)),
                                    ));
                                });
                            // Label
                            btn.spawn((
                                Text::new(format!("{}kg", weight as u32)),
                                TextFont {
                                    font_size: 9.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                    }

                    // Eraser button
                    grid.spawn((
                        Button,
                        Node {
                            width: Val::Px(50.0),
                            height: Val::Px(56.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::FlexEnd,
                            padding: UiRect::bottom(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.55, 0.55, 0.58)),
                        EraserButton,
                    ))
                    .with_children(|btn| {
                        btn.spawn((Node {
                            width: Val::Px(50.0),
                            height: Val::Px(38.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },))
                            .with_children(|icon| {
                                icon.spawn((
                                    Node {
                                        width: Val::Px(30.0),
                                        height: Val::Px(14.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.95, 0.70, 0.70)),
                                ));
                            });
                        btn.spawn((
                            Text::new("Erase"),
                            TextFont {
                                font_size: 9.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });

                    // Spring tool button
                    grid.spawn((
                        Button,
                        Node {
                            width: Val::Px(50.0),
                            height: Val::Px(56.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::FlexEnd,
                            padding: UiRect::bottom(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.55, 0.55, 0.58)),
                        SpringButton,
                    ))
                    .with_children(|btn| {
                        btn.spawn((Node {
                            width: Val::Px(50.0),
                            height: Val::Px(38.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },))
                            .with_children(|icon| {
                                icon.spawn((
                                    Node {
                                        width: Val::Px(14.0),
                                        height: Val::Px(14.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.0, 0.8, 0.7)),
                                ));
                            });
                        btn.spawn((
                            Text::new("Spring"),
                            TextFont {
                                font_size: 9.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });

                    // Drain tool button
                    grid.spawn((
                        Button,
                        Node {
                            width: Val::Px(50.0),
                            height: Val::Px(56.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::FlexEnd,
                            padding: UiRect::bottom(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.55, 0.55, 0.58)),
                        DrainButton,
                    ))
                    .with_children(|btn| {
                        btn.spawn((Node {
                            width: Val::Px(50.0),
                            height: Val::Px(38.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },))
                            .with_children(|icon| {
                                icon.spawn((
                                    Node {
                                        width: Val::Px(14.0),
                                        height: Val::Px(14.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.8, 0.4, 0.0)),
                                ));
                            });
                        btn.spawn((
                            Text::new("Drain"),
                            TextFont {
                                font_size: 9.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });

                    // Building tool button — warm tan house icon
                    grid.spawn((
                        Button,
                        Node {
                            width: Val::Px(50.0),
                            height: Val::Px(56.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::FlexEnd,
                            padding: UiRect::bottom(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.55, 0.55, 0.58)),
                        BuildingButton,
                    ))
                    .with_children(|btn| {
                        // House-shaped icon: narrow roof strip + wider body
                        btn.spawn((Node {
                            width: Val::Px(50.0),
                            height: Val::Px(38.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            row_gap: Val::Px(1.0),
                            ..default()
                        },))
                            .with_children(|icon| {
                                // Roof: narrower, darker strip on top
                                icon.spawn((
                                    Node {
                                        width: Val::Px(18.0),
                                        height: Val::Px(5.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.55, 0.38, 0.22)),
                                ));
                                // Body: wider, warm tan rectangle
                                icon.spawn((
                                    Node {
                                        width: Val::Px(14.0),
                                        height: Val::Px(10.0),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.76, 0.60, 0.42)),
                                ));
                            });
                        btn.spawn((
                            Text::new("Build"),
                            TextFont {
                                font_size: 9.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
                });

            // BRUSH section
            parent.spawn((
                Text::new("BRUSH"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.55, 0.60)),
            ));

            parent
                .spawn((Node {
                    width: Val::Px(104.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },))
                .with_children(|row| {
                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(30.0),
                            height: Val::Px(28.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.30, 0.30, 0.34)),
                        BrushDownButton,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("-"),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });

                    row.spawn((
                        Text::new("1"),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        BrushLabel,
                    ));

                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(30.0),
                            height: Val::Px(28.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.30, 0.30, 0.34)),
                        BrushUpButton,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("+"),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
                });

            // Divider
            parent.spawn((
                Node {
                    width: Val::Percent(80.0),
                    height: Val::Px(1.0),
                    margin: UiRect::vertical(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.35, 0.35, 0.40)),
            ));

            // FLOW section label
            parent.spawn((
                Text::new("FLOW"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.55, 0.60)),
            ));

            // Inlet toggle button
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(104.0),
                        height: Val::Px(32.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.4, 0.15, 0.15)),
                    InletButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Inlet"),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

            // Divider
            parent.spawn((
                Node {
                    width: Val::Percent(80.0),
                    height: Val::Px(1.0),
                    margin: UiRect::vertical(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.35, 0.35, 0.40)),
            ));

            // VIEW section label
            parent.spawn((
                Text::new("VIEW"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.55, 0.60)),
            ));

            for (label, mode) in [
                ("Normal", ViewMode::Normal),
                ("Pressure", ViewMode::Pressure),
                ("Flow", ViewMode::FlowArrows),
            ] {
                parent
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(104.0),
                            height: Val::Px(28.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.30, 0.30, 0.34)),
                        ViewButton(mode),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new(label),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
            }

            // Divider
            parent.spawn((
                Node {
                    width: Val::Percent(80.0),
                    height: Val::Px(1.0),
                    margin: UiRect::vertical(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.35, 0.35, 0.40)),
            ));

            // Reset button
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(104.0),
                        height: Val::Px(32.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.50, 0.12, 0.12)),
                    ResetButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Reset"),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

            // Speed controls: [-] x1 [+]
            parent
                .spawn((Node {
                    width: Val::Px(104.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    margin: UiRect::top(Val::Px(12.0)),
                    ..default()
                },))
                .with_children(|row| {
                    // Decrement button
                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(30.0),
                            height: Val::Px(28.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.30, 0.30, 0.34)),
                        SpeedDownButton,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("-"),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });

                    // Speed label (center)
                    row.spawn((
                        Text::new("x1"),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        SpeedLabel,
                    ));

                    // Increment button
                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(30.0),
                            height: Val::Px(28.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.30, 0.30, 0.34)),
                        SpeedUpButton,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("+"),
                            TextFont {
                                font_size: 16.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
                });
        });

    // Status bar — pinned to the bottom of the grid area
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Px(PANEL_WIDTH),
                right: Val::Px(0.0),
                height: Val::Px(22.0),
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
        ))
        .with_children(|bar| {
            bar.spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                StatusText,
            ));
        });
}

fn handle_weight_buttons(
    interaction_query: Query<(&Interaction, &WeightButton), Changed<Interaction>>,
    mut selected: ResMut<SelectedTool>,
) {
    for (interaction, weight_btn) in &interaction_query {
        if *interaction == Interaction::Pressed {
            *selected = SelectedTool::Block(weight_btn.0);
        }
    }
}

fn handle_eraser_button(
    q: Query<&Interaction, (Changed<Interaction>, With<EraserButton>)>,
    mut selected: ResMut<SelectedTool>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            *selected = SelectedTool::Eraser;
        }
    }
}

fn handle_spring_button(
    q: Query<&Interaction, (Changed<Interaction>, With<SpringButton>)>,
    mut selected: ResMut<SelectedTool>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            *selected = SelectedTool::Spring;
        }
    }
}

fn handle_drain_button(
    q: Query<&Interaction, (Changed<Interaction>, With<DrainButton>)>,
    mut selected: ResMut<SelectedTool>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            *selected = SelectedTool::Drain;
        }
    }
}

fn handle_building_button(
    q: Query<&Interaction, (Changed<Interaction>, With<BuildingButton>)>,
    mut selected: ResMut<SelectedTool>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            *selected = SelectedTool::Building { weight: 3000.0, threshold: 2500.0 };
        }
    }
}

fn update_tool_buttons(
    mut weight_query: Query<
        (&WeightButton, &mut BackgroundColor),
        (
            Without<EraserButton>,
            Without<SpringButton>,
            Without<DrainButton>,
            Without<BuildingButton>,
        ),
    >,
    mut eraser_query: Query<
        &mut BackgroundColor,
        (
            With<EraserButton>,
            Without<SpringButton>,
            Without<DrainButton>,
            Without<BuildingButton>,
        ),
    >,
    mut spring_query: Query<
        &mut BackgroundColor,
        (With<SpringButton>, Without<DrainButton>, Without<BuildingButton>),
    >,
    mut drain_query: Query<
        &mut BackgroundColor,
        (With<DrainButton>, Without<BuildingButton>),
    >,
    mut building_query: Query<&mut BackgroundColor, With<BuildingButton>>,
    selected: Res<SelectedTool>,
) {
    if !selected.is_changed() {
        return;
    }
    let selected_color = Color::srgb(0.18, 0.36, 0.65);
    let unselected_color = Color::srgb(0.55, 0.55, 0.58);

    for (btn, mut color) in &mut weight_query {
        *color = if *selected == SelectedTool::Block(btn.0) {
            BackgroundColor(selected_color)
        } else {
            BackgroundColor(unselected_color)
        };
    }
    for mut color in &mut eraser_query {
        *color = if *selected == SelectedTool::Eraser {
            BackgroundColor(selected_color)
        } else {
            BackgroundColor(unselected_color)
        };
    }
    for mut color in &mut spring_query {
        *color = if *selected == SelectedTool::Spring {
            BackgroundColor(selected_color)
        } else {
            BackgroundColor(unselected_color)
        };
    }
    for mut color in &mut drain_query {
        *color = if *selected == SelectedTool::Drain {
            BackgroundColor(selected_color)
        } else {
            BackgroundColor(unselected_color)
        };
    }
    for mut color in &mut building_query {
        *color = if matches!(*selected, SelectedTool::Building { .. }) {
            BackgroundColor(selected_color)
        } else {
            BackgroundColor(unselected_color)
        };
    }
}

fn handle_inlet_toggle(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<InletButton>)>,
    mut state: ResMut<GameState>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            state.water_flow = !state.water_flow;
        }
    }
}

fn update_inlet_button(
    mut query: Query<&mut BackgroundColor, With<InletButton>>,
    state: Res<GameState>,
) {
    if !state.is_changed() {
        return;
    }
    for mut color in &mut query {
        *color = if state.water_flow {
            BackgroundColor(Color::srgb(0.1, 0.6, 0.2))
        } else {
            BackgroundColor(Color::srgb(0.4, 0.15, 0.15))
        };
    }
}

fn handle_view_toggle(
    interaction_query: Query<(&Interaction, &ViewButton), Changed<Interaction>>,
    mut view_mode: ResMut<ViewMode>,
) {
    for (interaction, btn) in &interaction_query {
        if *interaction == Interaction::Pressed {
            *view_mode = btn.0.clone();
        }
    }
}

fn update_view_buttons(
    mut query: Query<(&ViewButton, &mut BackgroundColor)>,
    view_mode: Res<ViewMode>,
) {
    if !view_mode.is_changed() {
        return;
    }
    for (btn, mut color) in &mut query {
        *color = if btn.0 == *view_mode {
            BackgroundColor(Color::srgb(0.2, 0.5, 0.8))
        } else {
            BackgroundColor(Color::srgb(0.30, 0.30, 0.34))
        };
    }
}

fn handle_reset(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<ResetButton>)>,
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            *grid = Grid::init(grid.width, grid.height);
            state.water_flow = false;
            state.gate_progress = 0;
        }
    }
}

fn handle_speed_buttons(
    down_q: Query<&Interaction, (Changed<Interaction>, With<SpeedDownButton>)>,
    up_q: Query<&Interaction, (Changed<Interaction>, With<SpeedUpButton>)>,
    mut state: ResMut<GameState>,
) {
    for interaction in &down_q {
        if *interaction == Interaction::Pressed {
            state.sim_speed = (state.sim_speed - 1).max(1);
        }
    }
    for interaction in &up_q {
        if *interaction == Interaction::Pressed {
            state.sim_speed = (state.sim_speed + 1).min(16);
        }
    }
}

fn update_speed_label(mut label_query: Query<&mut Text, With<SpeedLabel>>, state: Res<GameState>) {
    if !state.is_changed() {
        return;
    }
    for mut text in &mut label_query {
        **text = format!("x{}", state.sim_speed);
    }
}

fn handle_brush_buttons(
    down_q: Query<&Interaction, (Changed<Interaction>, With<BrushDownButton>)>,
    up_q: Query<&Interaction, (Changed<Interaction>, With<BrushUpButton>)>,
    mut state: ResMut<GameState>,
) {
    for interaction in &down_q {
        if *interaction == Interaction::Pressed {
            state.brush_radius = state.brush_radius.saturating_sub(1);
        }
    }
    for interaction in &up_q {
        if *interaction == Interaction::Pressed {
            state.brush_radius = (state.brush_radius + 1).min(5);
        }
    }
}

fn update_brush_label(mut label_query: Query<&mut Text, With<BrushLabel>>, state: Res<GameState>) {
    if !state.is_changed() {
        return;
    }
    let diameter = state.brush_radius * 2 + 1;
    for mut text in &mut label_query {
        **text = format!("{}", diameter);
    }
}

fn update_status(
    mut query: Query<&mut Text, With<StatusText>>,
    diagnostics: Res<DiagnosticsStore>,
    grid: Res<Grid>,
    state: Res<GameState>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let water_kg: f32 = grid
        .cells
        .iter()
        .filter_map(|c| {
            if let Cell::Water(kg) = c {
                Some(*kg)
            } else {
                None
            }
        })
        .sum();

    let water_str = if water_kg >= 1_000_000.0 {
        format!("{:.1}kt", water_kg / 1_000_000.0)
    } else if water_kg >= 1_000.0 {
        format!("{:.1}t", water_kg / 1_000.0)
    } else {
        format!("{:.0}kg", water_kg)
    };

    let object_count = grid
        .cells
        .iter()
        .filter(|c| matches!(c, Cell::Object(_)))
        .count();

    let speed_str = format!("x{}", state.sim_speed);

    for mut text in &mut query {
        **text = format!(
            "FPS: {fps:.0}  |  Water: {water_str}  |  Objects: {object_count}  |  Speed: {speed_str}"
        );
    }
}
