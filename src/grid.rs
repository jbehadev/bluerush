use crate::render::find_cursor_cell;
use crate::persistence;
use crate::simulation::{
    Cell, Grid, MAX_WATER_KG, build_depth_pressure, step_buildings, step_objects, step_simulation,
};
use rand::thread_rng;
use crate::undo::UndoStack;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;

use crate::camera::CameraPlugin;
use crate::render::RenderPlugin;
use crate::ui::UiPlugin;

#[derive(Message)]
struct SaveRequested;

#[derive(Message)]
struct LoadRequested;

/// Root plugin that wires together the camera, UI, rendering, and all simulation systems.
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((CameraPlugin, UiPlugin, RenderPlugin))
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_message::<SaveRequested>()
            .add_message::<LoadRequested>()
            .init_resource::<PendingFileOp>()
            .init_resource::<UndoStack>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    simulate_objects,
                    simulate_buildings_system,
                    flow_water,
                    simulate_flow,
                    handle_input,
                    animate_gate,
                    handle_save,
                    handle_load,
                    poll_file_op,
                ),
            );
    }
}

// Allow Grid (defined in simulation) to be used as a Bevy resource.
impl Resource for Grid {}

/// Pixel width reserved for the left-side UI panel. Clicks within this region are
/// not forwarded to the grid.
pub const PANEL_WIDTH: f32 = 120.0;

/// Startup configuration loaded from `config.yaml` and shared as a Bevy resource.
#[derive(Resource, Clone)]
pub struct GridConfig {
    /// Number of grid columns.
    pub cols: usize,
    /// Number of grid rows.
    pub rows: usize,
    /// Tile edge length in pixels (informational; used for save/load validation).
    pub tile_size: f32,
    /// When true, objects that collide at speed destroy each other.
    pub collision_destruction: bool,
}

/// Controls how water enters from the inlet row (y=0).
#[derive(Resource, PartialEq, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum InletMode {
    /// Constant fill at MAX_WATER_KG every tick.
    #[default]
    Flood,
    /// Smooth oscillation between 100 and MAX_WATER_KG using a sine wave.
    Sine,
    /// Random water level each frame between 100 and MAX_WATER_KG.
    Random,
}

/// Controls which overlay is rendered on top of the grid.
#[derive(Resource, PartialEq, Clone, Default)]
pub enum ViewMode {
    /// Standard cell colours.
    #[default]
    Normal,
    /// Rainbow heatmap showing depth-based pressure values.
    Pressure,
    /// Arrows showing predicted flow direction for the currently selected weight.
    FlowArrows,
}

/// Tracks per-cycle random peak for the Random inlet mode.
#[derive(Resource)]
pub struct WaveState {
    /// Which sine cycle we're currently in (increments each zero-crossing).
    pub cycle: u32,
    /// Random peak for the current cycle (0.0–1.0 mapped to 100–MAX_WATER_KG).
    pub peak: f32,
}

impl Default for WaveState {
    fn default() -> Self {
        Self { cycle: 0, peak: 1.0 }
    }
}

/// Per-frame mutable simulation state, separate from `GridConfig` which is read-only.
#[derive(Resource)]
pub struct GameState {
    /// Whether the inlet gate is open and water is flowing.
    pub water_flow: bool,
    /// How many cells the gate has opened (used by `animate_gate`).
    pub gate_progress: usize,
    /// Number of simulation ticks to run per rendered frame.
    pub sim_speed: u32,
    /// Radius of the paint brush in cells (0 = single cell).
    pub brush_radius: u32,
    pub drag_start: Option<(usize, usize)>,
}

/// The currently active placement tool.
#[derive(Resource, PartialEq, Clone)]
pub enum SelectedTool {
    /// Place an immovable block with the given weight in kg.
    Block(f32),
    /// Remove any cell, replacing it with `Air`.
    Eraser,
    /// Place a `Spring` (permanent water source).
    Spring,
    /// Place a `Drain` (permanent water sink).
    Drain,
    /// Place a destructible `Building` that collapses under water pressure.
    Building { weight: f32, threshold: f32 },
}

#[derive(Resource, Default)]
struct PendingFileOp {
    op: Option<persistence::PendingIo>,
}

pub fn setup(mut commands: Commands, config: Res<GridConfig>) {
    let width = config.cols;
    let height = config.rows;

    commands.insert_resource(GameState {
        water_flow: false,
        gate_progress: 0,
        sim_speed: 1,
        brush_radius: 0,
        drag_start: None,
    });
    commands.init_resource::<ViewMode>();
    commands.init_resource::<InletMode>();
    commands.init_resource::<WaveState>();
    commands.insert_resource(SelectedTool::Block(200.0));
    commands.insert_resource(Grid::init(width, height));
}

fn handle_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
    mut selected: ResMut<SelectedTool>,
    mut view_mode: ResMut<ViewMode>,
    mut inlet_mode: ResMut<InletMode>,
    mut save_events: MessageWriter<SaveRequested>,
    mut load_events: MessageWriter<LoadRequested>,
    mut undo_stack: ResMut<UndoStack>,
    current_level: Res<crate::levels::CurrentLevel>,
    config: Res<GridConfig>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera_q.single() else {
        return;
    };

    let ctrl = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    if mouse.just_pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some((cx, cy)) = find_cursor_cell(cursor_pos, camera, camera_transform, &grid) {
                state.drag_start = Some((cx, cy));
            }
        }
    }

    if mouse.pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some((mut cx, mut cy)) =
                find_cursor_cell(cursor_pos, camera, camera_transform, &grid)
            {
                // When shift is held, constrain to the dominant axis from drag start
                if shift {
                    if let Some((sx, sy)) = state.drag_start {
                        let dx = (cx as isize - sx as isize).unsigned_abs();
                        let dy = (cy as isize - sy as isize).unsigned_abs();
                        if dx >= dy {
                            cy = sy; // horizontal line
                        } else {
                            cx = sx; // vertical line
                        }
                    }
                }

                let r = state.brush_radius as usize;
                for dy in 0..=(r * 2) {
                    for dx in 0..=(r * 2) {
                        let bx = (cx + dx).saturating_sub(r);
                        let by = (cy + dy).saturating_sub(r);
                        if bx < grid.width
                            && by < grid.height
                            && !matches!(grid.get_cell(bx, by), Cell::Wall)
                        {
                            let new_cell = match *selected {
                                SelectedTool::Block(w)
                                    if !matches!(grid.get_cell(bx, by), Cell::Object(_)) =>
                                {
                                    Some(Cell::Object(w))
                                }
                                SelectedTool::Eraser => Some(Cell::Air),
                                SelectedTool::Spring
                                    if !matches!(grid.get_cell(bx, by), Cell::Spring) =>
                                {
                                    Some(Cell::Spring)
                                }
                                SelectedTool::Drain
                                    if !matches!(grid.get_cell(bx, by), Cell::Drain) =>
                                {
                                    Some(Cell::Drain)
                                }
                                SelectedTool::Building { weight, threshold }
                                    if !matches!(
                                        grid.get_cell(bx, by),
                                        Cell::Building { .. }
                                    ) =>
                                {
                                    Some(Cell::Building { weight, threshold })
                                }
                                _ => None,
                            };
                            if let Some(new) = new_cell {
                                let old = grid.get_cell(bx, by).clone();
                                undo_stack.record(bx, by, old, new.clone());
                                grid.set_cell(bx, by, new);
                            }
                        }
                    }
                }
            }
        }
    }
    // Commit pending undo changes when mouse is released
    if mouse.just_released(MouseButton::Left) {
        state.drag_start = None;
        if undo_stack.has_pending() {
            undo_stack.commit();
        }
    }

    // Undo/Redo shortcuts: Cmd+Z / Cmd+Shift+Z
    if ctrl && keyboard.just_pressed(KeyCode::KeyZ) {
        if shift {
            undo_stack.redo(&mut grid);
        } else {
            undo_stack.undo(&mut grid);
        }
    }
    if mouse.just_pressed(MouseButton::Right) && shift {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some((grid_x, grid_y)) = find_cursor_cell(cursor_pos, camera, camera_transform, &grid) {
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
    if keyboard.just_pressed(KeyCode::KeyD) && !ctrl {
        *selected = SelectedTool::Drain;
    }
    if keyboard.just_pressed(KeyCode::KeyB) {
        *selected = SelectedTool::Building { weight: 3000.0, threshold: 2500.0 };
    }
    if keyboard.just_pressed(KeyCode::KeyS) && ctrl {
        save_events.write(SaveRequested);
    } else if keyboard.just_pressed(KeyCode::KeyS) {
        *selected = SelectedTool::Spring;
    }
    if keyboard.just_pressed(KeyCode::KeyO) && ctrl {
        load_events.write(LoadRequested);
    }
    if keyboard.just_pressed(KeyCode::KeyX) {
        state.water_flow = !state.water_flow;
    }
    if keyboard.just_pressed(KeyCode::KeyW) {
        *inlet_mode = match *inlet_mode {
            InletMode::Flood => InletMode::Sine,
            InletMode::Sine => InletMode::Random,
            InletMode::Random => InletMode::Flood,
        };
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        crate::levels::load_level(
            &current_level.path,
            &mut grid,
            &mut state,
            &mut inlet_mode,
            &config,
        );
        undo_stack.clear();
    }
    if keyboard.just_pressed(KeyCode::KeyM) {
        *view_mode = if *view_mode == ViewMode::Pressure {
            ViewMode::Normal
        } else {
            ViewMode::Pressure
        };
    }
    if keyboard.just_pressed(KeyCode::KeyF) {
        *view_mode = if *view_mode == ViewMode::FlowArrows {
            ViewMode::Normal
        } else {
            ViewMode::FlowArrows
        };
    }
}


/// Opens the gate at y=1 one cell per side per frame when the inlet is ON,
/// closes it one cell per side per frame when OFF (overwrites any water).
fn animate_gate(mut grid: ResMut<Grid>, mut state: ResMut<GameState>) {
    let center = grid.width / 2;
    let left_max = center - 1;
    let right_max = grid.width - 1 - center;
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

fn flow_water(
    mut grid: ResMut<Grid>,
    state: Res<GameState>,
    inlet_mode: Res<InletMode>,
    time: Res<Time>,
    mut wave_state: ResMut<WaveState>,
) {
    if !state.water_flow {
        return;
    }
    let period = 4.0_f32;
    let t = time.elapsed_secs();
    let flow_rate: f32 = match *inlet_mode {
        InletMode::Flood => MAX_WATER_KG,
        InletMode::Sine => {
            100.0 + (MAX_WATER_KG - 100.0) * ((t * std::f32::consts::TAU / period).sin() * 0.5 + 0.5)
        }
        InletMode::Random => {
            // Same sine shape, but pick a new random peak at each trough
            use rand::Rng;
            let phase = t * std::f32::consts::TAU / period;
            let wave = phase.sin() * 0.5 + 0.5;
            // Shift phase so the cycle counter increments at the trough (sin minimum)
            let cycle_at_trough = ((phase + std::f32::consts::FRAC_PI_2) / std::f32::consts::TAU).floor() as u32;
            if cycle_at_trough != wave_state.cycle {
                wave_state.cycle = cycle_at_trough;
                wave_state.peak = 0.1 + 0.9 * thread_rng().r#gen::<f32>();
            }
            100.0 + (MAX_WATER_KG - 100.0) * wave * wave_state.peak
        }
    };
    let width = grid.width;
    let is_wave = *inlet_mode != InletMode::Flood;
    for x in 1..width - 1 {
        let new_cell = match grid.cells[x] {
            Cell::Air => Cell::Water(flow_rate),
            Cell::Water(kg) => {
                if is_wave {
                    // In wave mode, force the inlet to the oscillating level
                    Cell::Water(flow_rate)
                } else {
                    Cell::Water((kg + flow_rate).min(MAX_WATER_KG))
                }
            }
            Cell::Object(weight) => Cell::Object(weight),
            Cell::Wall => Cell::Wall,
            Cell::Spring => Cell::Spring,
            Cell::Drain => Cell::Drain,
            Cell::Building { weight, threshold } => Cell::Building { weight, threshold },
            Cell::Rock => Cell::Rock,
            Cell::Sand => Cell::Sand,
        };
        grid.set_cell(x, 0, new_cell);
    }
}

fn simulate_objects(mut grid: ResMut<Grid>, state: Res<GameState>, config: Res<GridConfig>) {
    if !state.water_flow {
        return;
    }
    let mut rng = thread_rng();
    for _ in 0..state.sim_speed {
        step_objects(&mut grid, &mut rng, config.collision_destruction);
    }
}

fn simulate_buildings_system(mut grid: ResMut<Grid>, state: Res<GameState>) {
    if !state.water_flow {
        return;
    }
    for _ in 0..state.sim_speed {
        step_buildings(&mut grid);
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

fn handle_save(
    mut events: MessageReader<SaveRequested>,
    grid: Res<Grid>,
    config: Res<GridConfig>,
    mut pending: ResMut<PendingFileOp>,
) {
    for _ in events.read() {
        if pending.op.is_some() {
            println!("File dialog already open");
            return;
        }
        pending.op = Some(persistence::save_grid_async(&grid, config.tile_size));
    }
}

fn handle_load(
    mut events: MessageReader<LoadRequested>,
    config: Res<GridConfig>,
    grid: Res<Grid>,
    mut pending: ResMut<PendingFileOp>,
) {
    for _ in events.read() {
        if pending.op.is_some() {
            println!("File dialog already open");
            return;
        }
        pending.op = Some(persistence::load_grid_async(
            config.tile_size,
            grid.width,
            grid.height,
        ));
    }
}

fn poll_file_op(
    mut pending: ResMut<PendingFileOp>,
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
) {
    let Some(ref op) = pending.op else { return };
    let done = match op {
        persistence::PendingIo::Save(rx) => {
            let rx = rx.lock().unwrap();
            match rx.try_recv() {
                Ok(Ok(())) => true,
                Ok(Err(e)) => {
                    println!("Save failed: {e}");
                    true
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => false,
                Err(_) => true,
            }
        }
        persistence::PendingIo::Load(rx) => {
            let rx = rx.lock().unwrap();
            match rx.try_recv() {
                Ok(Ok(cells)) => {
                    grid.cells = cells;
                    state.water_flow = false;
                    state.gate_progress = 0;
                    true
                }
                Ok(Err(e)) => {
                    if e != "Cancelled" {
                        println!("Load failed: {e}");
                    }
                    true
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => false,
                Err(_) => true,
            }
        }
    };
    if done {
        pending.op = None;
    }
}
