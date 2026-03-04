use crate::simulation::{
    Cell, Grid, MAX_WATER_KG, build_depth_pressure, step_objects, step_simulation,
};
use crate::textures::TextureAssets;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::{color::palettes::css::*, prelude::*};

pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    simulate_objects,
                    flow_water,
                    simulate_flow,
                    render_grid,
                    handle_input,
                    handle_weight_buttons,
                    handle_eraser_button,
                    update_tool_buttons,
                    handle_inlet_toggle,
                    update_inlet_button,
                    handle_heatmap_toggle,
                    update_heatmap_button,
                    handle_reset,
                    animate_gate,
                    handle_speed_buttons,
                    update_speed_label,
                    handle_brush_buttons,
                    update_brush_label,
                    update_status,
                ),
            );
    }
}

// Allow Grid (defined in simulation) to be used as a Bevy resource.
impl Resource for Grid {}

const PANEL_WIDTH: f32 = 120.0;

#[derive(Resource, Clone)]
pub struct GridConfig {
    pub window_width: f32,
    pub window_height: f32,
    pub tile_size: f32,
}

impl GridConfig {
    fn offset_x(&self) -> f32 {
        -(self.window_width / 2.0) + PANEL_WIDTH + (self.tile_size / 2.0)
    }
    fn offset_y(&self) -> f32 {
        -(self.window_height / 2.0) + (self.tile_size / 2.0)
    }
}

#[derive(Resource)]
struct GameState {
    water_flow: bool,
    show_pressure: bool,
    gate_progress: usize, // how many cells are open on each side from center (0 = fully closed)
    sim_speed: u32,       // simulation steps per frame: 1–16
    brush_radius: u32,    // 0 = 1×1, 1 = 3×3, 2 = 5×5, …
}

#[derive(Resource, PartialEq, Clone)]
enum SelectedTool {
    Block(f32),
    Eraser,
}

#[derive(Component)]
struct WeightButton(f32);

#[derive(Component)]
struct EraserButton;

#[derive(Component)]
struct InletButton;

#[derive(Component)]
struct ResetButton;

#[derive(Component)]
struct HeatmapButton;

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

#[derive(Component)]
struct Tile {
    x: usize,
    y: usize,
}

#[derive(Component)]
struct TileBorder {
    x: usize,
    y: usize,
}

fn setup(mut commands: Commands, config: Res<GridConfig>) {
    let tile_size = config.tile_size;
    let width = ((config.window_width - PANEL_WIDTH) / tile_size) as usize;
    let height = (config.window_height / tile_size) as usize;
    let offset_x = config.offset_x();
    let offset_y = config.offset_y();

    commands.spawn(Camera2d);
    commands.insert_resource(GameState {
        water_flow: false,
        show_pressure: false,
        gate_progress: 0,
        sim_speed: 1,
        brush_radius: 0,
    });
    commands.insert_resource(SelectedTool::Block(200.0));
    commands.insert_resource(Grid::init(width, height));

    let border_inner = (tile_size - 2.0).max(1.0);
    for row in 0..height {
        for col in 0..width {
            let x = offset_x + (col as f32 * tile_size);
            let y = offset_y + (row as f32 * tile_size);
            commands.spawn((
                Sprite::from_color(BLUE, Vec2::splat(tile_size)),
                Transform::from_xyz(x, y, 0.0),
                Tile { x: col, y: row },
            ));
            // Inner sprite sits 1px inside the outer tile; used for heatmap object borders
            commands.spawn((
                Sprite::from_color(Color::NONE, Vec2::splat(border_inner)),
                Transform::from_xyz(x, y, 0.1),
                TileBorder { x: col, y: row },
            ));
        }
    }

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
                TextFont { font_size: 9.0, ..default() },
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
                        (200.0,  14.0, 0.92),
                        (500.0,  20.0, 0.74),
                        (1000.0, 26.0, 0.52),
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
                                TextFont { font_size: 9.0, ..default() },
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
                            TextFont { font_size: 9.0, ..default() },
                            TextColor(Color::WHITE),
                        ));
                    });
                });

            // BRUSH section
            parent.spawn((
                Text::new("BRUSH"),
                TextFont { font_size: 9.0, ..default() },
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
                            TextFont { font_size: 16.0, ..default() },
                            TextColor(Color::WHITE),
                        ));
                    });

                    row.spawn((
                        Text::new("1"),
                        TextFont { font_size: 14.0, ..default() },
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
                            TextFont { font_size: 16.0, ..default() },
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
                TextFont { font_size: 9.0, ..default() },
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
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(Color::WHITE),
                    ));
                });

            // Heatmap toggle button
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
                    BackgroundColor(Color::srgb(0.30, 0.30, 0.34)),
                    HeatmapButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Pressure"),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(Color::WHITE),
                    ));
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
                        TextFont { font_size: 12.0, ..default() },
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
                            TextFont { font_size: 16.0, ..default() },
                            TextColor(Color::WHITE),
                        ));
                    });

                    // Speed label (center)
                    row.spawn((
                        Text::new("x1"),
                        TextFont { font_size: 14.0, ..default() },
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
                            TextFont { font_size: 16.0, ..default() },
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

fn render_grid(
    grid: Res<Grid>,
    mut tile_query: Query<(&Tile, &mut Sprite), Without<TileBorder>>,
    mut border_query: Query<(&TileBorder, &mut Sprite), Without<Tile>>,
    textures: Res<TextureAssets>,
    mut state: ResMut<GameState>,
    time: Res<Time>,
) {
    if state.show_pressure {
        render_heat_grid(&grid, &mut tile_query, &mut border_query);
        return;
    }
    // Only clear border sprites when switching back from heatmap mode
    if state.is_changed() {
        for (_, mut sprite) in &mut border_query {
            sprite.color = Color::NONE;
        }
    }
    // Switch froth frames every 0.4s — gives the illusion of churning bubbles.
    let froth = if (time.elapsed_secs() % 0.8) < 0.4 {
        &textures.froth_frame1
    } else {
        &textures.froth_frame2
    };
    for (tile, mut sprite) in &mut tile_query {
        match grid.cells[tile.y * grid.width + tile.x] {
            Cell::Air => {
                sprite.image = Handle::default();
                sprite.color = WHITE.into();
            }
            Cell::Water(kg) if kg < MAX_WATER_KG * 0.03 => {
                sprite.image = froth.clone();
                sprite.color = WHITE.into();
            }
            Cell::Water(kg) => {
                sprite.image = Handle::default();
                let fill = kg / MAX_WATER_KG;
                sprite.color = Color::srgb(1.0 - fill, 1.0 - fill, 1.0);
            }
            Cell::Wall => {
                sprite.image = Handle::default();
                sprite.color = Color::srgb(0.1, 0.1, 0.1);
            }
            Cell::Object(w) => {
                sprite.image = Handle::default();
                // Match toolbar icon shades: 200kg = lightest, 5000kg = darkest
                let brightness = if w <= 200.0 { 0.92 }
                    else if w <= 500.0 { 0.74 }
                    else if w <= 1000.0 { 0.52 }
                    else if w <= 2000.0 { 0.30 }
                    else { 0.10 };
                sprite.color = Color::srgb(brightness, brightness, brightness);
            }
        }
    }
}

/// Maps t ∈ [0,1] through a five-stop rainbow: blue → cyan → green → yellow → red.
fn pressure_color(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.25 {
        let s = t / 0.25;
        (0.0, s, 1.0) // blue → cyan
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25;
        (0.0, 1.0, 1.0 - s) // cyan → green
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25;
        (s, 1.0, 0.0) // green → yellow
    } else {
        let s = (t - 0.75) / 0.25;
        (1.0, 1.0 - s, 0.0) // yellow → red
    };
    Color::srgb(r, g, b)
}

fn render_heat_grid(
    grid: &Res<Grid>,
    tile_query: &mut Query<(&Tile, &mut Sprite), Without<TileBorder>>,
    border_query: &mut Query<(&TileBorder, &mut Sprite), Without<Tile>>,
) {
    let depth = build_depth_pressure(grid);
    let max = depth.iter().cloned().fold(1.0f32, f32::max);

    for (tile, mut sprite) in tile_query.iter_mut() {
        sprite.image = Handle::default();
        let idx = tile.y * grid.width + tile.x;
        if matches!(grid.cells[idx], Cell::Object(_)) {
            // Outer sprite = black border ring
            sprite.color = Color::BLACK;
        } else {
            let val = depth[idx];
            sprite.color = if val > 0.0 { pressure_color(val / max) } else { WHITE.into() };
        }
    }

    for (border, mut sprite) in border_query.iter_mut() {
        sprite.image = Handle::default();
        let idx = border.y * grid.width + border.x;
        if matches!(grid.cells[idx], Cell::Object(_)) {
            // Inner sprite = pressure color showing through the object
            let val = depth[idx];
            sprite.color = if val > 0.0 { pressure_color(val / max) } else { WHITE.into() };
        } else {
            sprite.color = Color::NONE;
        }
    }
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

fn update_tool_buttons(
    mut weight_query: Query<(&WeightButton, &mut BackgroundColor), Without<EraserButton>>,
    mut eraser_query: Query<&mut BackgroundColor, With<EraserButton>>,
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
            BackgroundColor(Color::srgb(0.1, 0.6, 0.2)) // green = ON
        } else {
            BackgroundColor(Color::srgb(0.4, 0.15, 0.15)) // dark red = OFF
        };
    }
}

fn handle_heatmap_toggle(
    interaction_query: Query<&Interaction, (Changed<Interaction>, With<HeatmapButton>)>,
    mut state: ResMut<GameState>,
) {
    for interaction in &interaction_query {
        if *interaction == Interaction::Pressed {
            state.show_pressure = !state.show_pressure;
        }
    }
}

fn update_heatmap_button(
    mut query: Query<&mut BackgroundColor, With<HeatmapButton>>,
    state: Res<GameState>,
) {
    if !state.is_changed() {
        return;
    }
    for mut color in &mut query {
        *color = if state.show_pressure {
            BackgroundColor(Color::srgb(0.2, 0.5, 0.8)) // blue = ON
        } else {
            BackgroundColor(Color::srgb(0.3, 0.3, 0.3)) // gray = OFF
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

/// Opens the gate at y=1 one cell per side per frame when the inlet is ON,
/// closes it one cell per side per frame when OFF (overwrites any water).
fn animate_gate(mut grid: ResMut<Grid>, mut state: ResMut<GameState>) {
    let center = grid.width / 2;
    // Left and right interiors are not always equal (odd-width grids round center down),
    // so track each side's limit independently.
    let left_max = center - 1; // leftmost valid cell is x=1
    let right_max = grid.width - 1 - center; // rightmost valid cell is x=width-2
    let max_progress = left_max.max(right_max);

    if state.water_flow && state.gate_progress < max_progress {
        let p = state.gate_progress;
        if p < left_max {
            grid.set_cell(center - p - 1, 1, Cell::Air);
        }
        if p < right_max {
            grid.set_cell(center + p, 1, Cell::Air);
        }
        state.gate_progress += 1;
    } else if !state.water_flow && state.gate_progress > 0 {
        let p = state.gate_progress;
        if p <= left_max {
            grid.set_cell(center - p, 1, Cell::Wall);
        }
        if p <= right_max {
            grid.set_cell(center + p - 1, 1, Cell::Wall);
        }
        state.gate_progress -= 1;
    }
}

/// Converts a cursor window position to a grid cell, or None if it's in the toolbar/OOB.
/// Uses the camera's actual viewport to get correct world coordinates at any resolution/DPI.
fn cursor_to_grid(
    cursor_pos: Vec2,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    config: &GridConfig,
) -> Option<(usize, usize)> {
    // Reject anything inside the left toolbar panel (window pixels, no math needed)
    if cursor_pos.x < PANEL_WIDTH {
        return None;
    }
    let world = camera.viewport_to_world_2d(camera_transform, cursor_pos).ok()?;
    // offset_x/y is the world position of tile (0, 0) center
    let gx = (world.x - config.offset_x()) / config.tile_size + 0.5;
    let gy = (world.y - config.offset_y()) / config.tile_size + 0.5;
    if gx < 0.0 || gy < 0.0 {
        return None;
    }
    Some((gx as usize, gy as usize))
}

fn handle_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
    mut selected: ResMut<SelectedTool>,
    config: Res<GridConfig>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera_q.single() else { return };

    if mouse.pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some((cx, cy)) = cursor_to_grid(cursor_pos, camera, camera_transform, &config) {
                let r = state.brush_radius as usize;
                for dy in 0..=(r * 2) {
                    for dx in 0..=(r * 2) {
                        let bx = (cx + dx).saturating_sub(r);
                        let by = (cy + dy).saturating_sub(r);
                        if bx < grid.width && by < grid.height
                            && !matches!(grid.get_cell(bx, by), Cell::Wall)
                        {
                            match *selected {
                                SelectedTool::Block(w) => grid.set_cell(bx, by, Cell::Object(w)),
                                SelectedTool::Eraser  => grid.set_cell(bx, by, Cell::Air),
                            }
                        }
                    }
                }
            }
        }
    }
    if mouse.just_pressed(MouseButton::Right) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some((grid_x, grid_y)) = cursor_to_grid(cursor_pos, camera, camera_transform, &config) {
                if grid_x < grid.width && grid_y < grid.height {
                    println!(
                        "{grid_x}, {grid_y}: {:?} pressure: {}",
                        *grid.get_cell(grid_x, grid_y),
                        build_depth_pressure(&grid)[grid_y * grid.width + grid_x]
                    );
                }
            }
        }
    }
    if keyboard.just_pressed(KeyCode::Digit1) {
        *selected = SelectedTool::Block(200.0);
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        *selected = SelectedTool::Block(500.0);
    }
    if keyboard.just_pressed(KeyCode::Digit3) {
        *selected = SelectedTool::Block(1000.0);
    }
    if keyboard.just_pressed(KeyCode::Digit4) {
        *selected = SelectedTool::Block(2000.0);
    }
    if keyboard.just_pressed(KeyCode::Digit5) {
        *selected = SelectedTool::Block(5000.0);
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        *selected = SelectedTool::Eraser;
    }
    if keyboard.just_pressed(KeyCode::KeyX) {
        state.water_flow = !state.water_flow;
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        *grid = Grid::init(grid.width, grid.height);
        state.water_flow = false;
        state.gate_progress = 0;
    }
    if keyboard.just_pressed(KeyCode::KeyM) {
        state.show_pressure = !state.show_pressure;
    }
}

fn flow_water(mut grid: ResMut<Grid>, _state: Res<GameState>) {
    // y=0 is always the reservoir — keep it full regardless of gate state.
    let flow_rate: f32 = MAX_WATER_KG;
    let width = grid.width;
    for x in 1..width - 1 {
        let new_cell = match grid.cells[x] {
            Cell::Air => Cell::Water(flow_rate),
            Cell::Water(kg) => Cell::Water((kg + flow_rate).min(MAX_WATER_KG)),
            Cell::Object(weight) => Cell::Object(weight),
            Cell::Wall => Cell::Wall,
        };
        grid.set_cell(x, 0, new_cell);
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

fn update_speed_label(
    mut label_query: Query<&mut Text, With<SpeedLabel>>,
    state: Res<GameState>,
) {
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

fn update_brush_label(
    mut label_query: Query<&mut Text, With<BrushLabel>>,
    state: Res<GameState>,
) {
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
        .filter_map(|c| if let Cell::Water(kg) = c { Some(*kg) } else { None })
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

fn simulate_objects(mut grid: ResMut<Grid>, state: Res<GameState>) {
    for _ in 0..state.sim_speed {
        grid.cells = step_objects(&grid);
    }
}

fn simulate_flow(mut grid: ResMut<Grid>, state: Res<GameState>) {
    if !state.water_flow {
        return;
    }
    for _ in 0..state.sim_speed {
        grid.cells = step_simulation(&grid);
    }
}
