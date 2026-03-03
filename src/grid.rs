use crate::simulation::{build_depth_pressure, step_objects, step_simulation, Cell, Grid, MAX_WATER_KG};
use crate::textures::TextureAssets;
use bevy::{color::palettes::css::*, prelude::*};

pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            (
                simulate_objects,
                flow_water,
                simulate_flow,
                render_grid,
                handle_input,
                handle_weight_buttons,
                update_button_colors,
                handle_inlet_toggle,
                update_inlet_button,
                handle_heatmap_toggle,
                update_heatmap_button,
                handle_reset,
                animate_gate,
            ),
        );
    }
}

// Allow Grid (defined in simulation) to be used as a Bevy resource.
impl Resource for Grid {}

const PANEL_WIDTH: f32 = 110.0;

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
}

#[derive(Resource)]
struct SelectedWeight(f32);

#[derive(Component)]
struct WeightButton(f32);

#[derive(Component)]
struct InletButton;

#[derive(Component)]
struct ResetButton;

#[derive(Component)]
struct HeatmapButton;

#[derive(Component)]
struct Tile {
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
    });
    commands.insert_resource(SelectedWeight(200.0));
    commands.insert_resource(Grid::init(width, height));

    for row in 0..height {
        for col in 0..width {
            commands.spawn((
                Sprite::from_color(
                    BLUE,
                    Vec2 {
                        x: tile_size,
                        y: tile_size,
                    },
                ),
                Transform::from_xyz(
                    offset_x + (col as f32 * tile_size),
                    offset_y + (row as f32 * tile_size),
                    0.0,
                ),
                Tile { x: col, y: row },
            ));
        }
    }

    // Left toolbar with weight selection buttons
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
                padding: UiRect::top(Val::Px(16.0)),
                row_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.1, 0.18, 0.38)),
        ))
        .with_children(|parent| {
            for &weight in &[200.0f32, 500.0, 1000.0, 2000.0, 5000.0] {
                let is_selected = weight == 200.0;
                parent
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(90.0),
                            height: Val::Px(40.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(if is_selected {
                            Color::srgb(0.2, 0.5, 0.8)
                        } else {
                            Color::srgb(0.3, 0.3, 0.3)
                        }),
                        WeightButton(weight),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new(format!("{} kg", weight as u32)),
                            TextFont {
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
            }

            // Inlet toggle button, separated from weight buttons
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(90.0),
                        height: Val::Px(40.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        margin: UiRect::top(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.4, 0.15, 0.15)), // starts OFF (dark red)
                    InletButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Inlet"),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

            // Heatmap toggle button
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(90.0),
                        height: Val::Px(40.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.3, 0.3, 0.3)),
                    HeatmapButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Heat"),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });

            // Reset button, separated from toggles
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(90.0),
                        height: Val::Px(40.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        margin: UiRect::top(Val::Px(20.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.5, 0.15, 0.15)),
                    ResetButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Reset"),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn render_grid(
    grid: Res<Grid>,
    mut query: Query<(&Tile, &mut Sprite)>,
    textures: Res<TextureAssets>,
    mut state: ResMut<GameState>,
    time: Res<Time>,
) {
    if state.show_pressure {
        render_heat_grid(grid, query);
        return;
    }
    // Switch froth frames every 0.4s — gives the illusion of churning bubbles.
    let froth = if (time.elapsed_secs() % 0.8) < 0.4 {
        &textures.froth_frame1
    } else {
        &textures.froth_frame2
    };
    for (tile, mut sprite) in &mut query {
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
                // Heavier = darker: 200kg → ~0.86, 500kg → ~0.65, 1000kg → ~0.30
                let brightness = 1.0 - (w / 1000.0).clamp(0.0, 1.0) * 0.7;
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

fn render_heat_grid(grid: Res<Grid>, mut query: Query<(&Tile, &mut Sprite)>) {
    let depth = build_depth_pressure(&grid);
    let max = depth.iter().cloned().fold(1.0f32, f32::max);

    for (tile, mut sprite) in &mut query {
        let val = depth[tile.y * grid.width + tile.x];
        sprite.image = Handle::default();
        if val > 0.0 {
            sprite.color = pressure_color(val / max);
        } else {
            sprite.color = WHITE.into();
        }
    }
}

fn handle_weight_buttons(
    interaction_query: Query<(&Interaction, &WeightButton), Changed<Interaction>>,
    mut selected: ResMut<SelectedWeight>,
) {
    for (interaction, weight_btn) in &interaction_query {
        if *interaction == Interaction::Pressed {
            selected.0 = weight_btn.0;
        }
    }
}

fn update_button_colors(
    mut query: Query<(&WeightButton, &mut BackgroundColor)>,
    selected: Res<SelectedWeight>,
) {
    if !selected.is_changed() {
        return;
    }
    for (btn, mut color) in &mut query {
        *color = if btn.0 == selected.0 {
            BackgroundColor(Color::srgb(0.2, 0.5, 0.8))
        } else {
            BackgroundColor(Color::srgb(0.3, 0.3, 0.3))
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
    let left_max = center - 1;            // leftmost valid cell is x=1
    let right_max = grid.width - 2 - center; // rightmost valid cell is x=width-2
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

fn handle_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    window: Query<&Window>,
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
    mut selected: ResMut<SelectedWeight>,
    config: Res<GridConfig>,
) {
    if mouse.pressed(MouseButton::Left) {
        if let Ok(window) = window.single() {
            if let Some(cursor_pos) = window.cursor_position() {
                let world_x = cursor_pos.x - config.window_width / 2.0;
                let world_y = -(cursor_pos.y - config.window_height / 2.0);
                // Ignore clicks inside the left toolbar
                if world_x < -(config.window_width / 2.0) + PANEL_WIDTH {
                    return;
                }
                let grid_x =
                    ((world_x + config.window_width / 2.0 - PANEL_WIDTH) / config.tile_size) as usize;
                let grid_y = ((world_y + config.window_height / 2.0) / config.tile_size) as usize;
                if grid_x < grid.width
                    && grid_y < grid.height
                    && !matches!(grid.get_cell(grid_x, grid_y), Cell::Wall)
                {
                    grid.set_cell(grid_x, grid_y, Cell::Object(selected.0));
                }
            }
        }
    }
    if mouse.just_pressed(MouseButton::Right) {
        if let Ok(window) = window.single() {
            if let Some(cursor_pos) = window.cursor_position() {
                let world_x = cursor_pos.x - config.window_width / 2.0;
                let world_y = -(cursor_pos.y - config.window_height / 2.0);
                if world_x < -(config.window_width / 2.0) + PANEL_WIDTH {
                    return;
                }
                let grid_x =
                    ((world_x + config.window_width / 2.0 - PANEL_WIDTH) / config.tile_size) as usize;
                let grid_y = ((world_y + config.window_height / 2.0) / config.tile_size) as usize;
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
        selected.0 = 200.0;
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        selected.0 = 500.0;
    }
    if keyboard.just_pressed(KeyCode::Digit3) {
        selected.0 = 1000.0;
    }
    if keyboard.just_pressed(KeyCode::Digit4) {
        selected.0 = 2000.0;
    }
    if keyboard.just_pressed(KeyCode::Digit5) {
        selected.0 = 5000.0;
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

fn flow_water(mut grid: ResMut<Grid>, state: Res<GameState>) {
    if !state.water_flow {
        return;
    }
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

fn simulate_objects(mut grid: ResMut<Grid>, state: Res<GameState>) {
    if !state.water_flow {
        return;
    }
    grid.cells = step_objects(&grid);
}

fn simulate_flow(mut grid: ResMut<Grid>, state: Res<GameState>) {
    if !state.water_flow {
        return;
    }
    grid.cells = step_simulation(&grid);
}
