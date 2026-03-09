use crate::render::find_cursor_cell;
use crate::persistence;
use crate::simulation::{
    Cell, Grid, MAX_WATER_KG, build_depth_pressure, step_objects, step_simulation,
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

pub const PANEL_WIDTH: f32 = 120.0;

#[derive(Resource, Clone)]
pub struct GridConfig {
    pub cols: usize,
    pub rows: usize,
    pub tile_size: f32,
    pub collision_destruction: bool,
}

#[derive(Resource, PartialEq, Clone, Default)]
pub enum ViewMode {
    #[default]
    Normal,
    Pressure,
    FlowArrows,
}

#[derive(Resource)]
pub struct GameState {
    pub water_flow: bool,
    pub gate_progress: usize,
    pub sim_speed: u32,
    pub brush_radius: u32,
}

#[derive(Resource, PartialEq, Clone)]
pub enum SelectedTool {
    Block(f32),
    Eraser,
    Spring,
    Drain,
}

#[derive(Resource, Default)]
struct PendingFileOp {
    op: Option<persistence::PendingIo>,
}

fn setup(mut commands: Commands, config: Res<GridConfig>) {
    let width = config.cols;
    let height = config.rows;

    commands.insert_resource(GameState {
        water_flow: false,
        gate_progress: 0,
        sim_speed: 1,
        brush_radius: 0,
    });
    commands.init_resource::<ViewMode>();
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
    mut save_events: MessageWriter<SaveRequested>,
    mut load_events: MessageWriter<LoadRequested>,
    mut undo_stack: ResMut<UndoStack>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera_q.single() else {
        return;
    };

    let ctrl = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);

    if mouse.pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some((cx, cy)) = find_cursor_cell(cursor_pos, camera, camera_transform, &grid) {
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
    if mouse.just_released(MouseButton::Left) && undo_stack.has_pending() {
        undo_stack.commit();
    }

    // Undo/Redo shortcuts: Cmd+Z / Cmd+Shift+Z
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
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
    if keyboard.just_pressed(KeyCode::KeyR) {
        *grid = Grid::init(grid.width, grid.height);
        state.water_flow = false;
        state.gate_progress = 0;
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
            Cell::Spring => Cell::Spring,
            Cell::Drain => Cell::Drain,
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
