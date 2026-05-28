use crate::render::find_cursor_cell_3d;
use crate::persistence;
use crate::simulation::Cell;
use crate::simulation3d::{Grid3D, step_objects_3d, step_simulation_3d};
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

/// Fired by simulation systems after any grid state change.
/// The render system listens for this to rebuild voxel meshes.
#[derive(Message)]
pub struct GridDirty;

/// Root plugin that wires together the camera, UI, rendering, and simulation.
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((CameraPlugin, UiPlugin, RenderPlugin))
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_message::<SaveRequested>()
            .add_message::<LoadRequested>()
            .add_message::<GridDirty>()
            .init_resource::<PendingFileOp>()
            .init_resource::<UndoStack>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    simulate_objects,
                    simulate_flow,
                    handle_input,
                    handle_save,
                    handle_load,
                    poll_file_op,
                ),
            );
    }
}

// Allow Grid3D (defined in simulation3d) to be used as a Bevy resource.
impl Resource for Grid3D {}

/// Pixel width reserved for the left-side UI panel.
pub const PANEL_WIDTH: f32 = 120.0;

/// Startup configuration loaded from `config.yaml`.
#[derive(Resource, Clone)]
pub struct GridConfig {
    pub cols: usize,
    pub rows: usize,
    pub depth: usize,
    pub tile_size: f32,
    pub collision_destruction: bool,
}

/// Controls how water enters (kept for UI compatibility; Springs now handle inlet in 3D).
#[derive(Resource, PartialEq, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum InletMode {
    #[default]
    Flood,
    Sine,
    Random,
}

/// Controls which overlay is rendered.
#[derive(Resource, PartialEq, Clone, Default)]
pub enum ViewMode {
    #[default]
    Normal,
    Pressure,
    FlowArrows,
}

#[derive(Resource)]
pub struct WaveState {
    pub cycle: u32,
    pub peak: f32,
}

impl Default for WaveState {
    fn default() -> Self { Self { cycle: 0, peak: 1.0 } }
}

/// Per-frame mutable simulation state.
#[derive(Resource)]
pub struct GameState {
    pub water_flow: bool,
    pub gate_progress: usize,
    pub sim_speed: u32,
    pub brush_radius: u32,
    pub drag_start: Option<(usize, usize)>,
}

/// The Y layer currently selected for editing (0 = ground floor).
#[derive(Resource)]
pub struct ActiveLayer {
    pub y: usize,
}

/// The currently active placement tool.
#[derive(Resource, PartialEq, Clone)]
pub enum SelectedTool {
    Block(f32),
    Eraser,
    Spring,
    Drain,
    Building { weight: f32, threshold: f32 },
}

#[derive(Resource, Default)]
struct PendingFileOp {
    op: Option<persistence::PendingIo>,
}

pub fn setup(mut commands: Commands, config: Res<GridConfig>) {
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
    commands.insert_resource(ActiveLayer { y: 1 });
    // Grid is populated by levels.rs setup_level; blank is a placeholder
    commands.insert_resource(Grid3D::blank(config.cols, config.rows, config.depth));
}

fn simulate_flow(
    mut grid: ResMut<Grid3D>,
    state: Res<GameState>,
    mut dirty: MessageWriter<GridDirty>,
) {
    if !state.water_flow { return; }
    for _ in 0..state.sim_speed {
        grid.cells = step_simulation_3d(&grid);
    }
    dirty.write(GridDirty);
}

fn simulate_objects(
    mut grid: ResMut<Grid3D>,
    state: Res<GameState>,
    config: Res<GridConfig>,
    mut dirty: MessageWriter<GridDirty>,
) {
    if !state.water_flow { return; }
    let mut rng = thread_rng();
    for _ in 0..state.sim_speed {
        step_objects_3d(&mut grid, &mut rng, config.collision_destruction);
    }
    dirty.write(GridDirty);
}

fn handle_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mut grid: ResMut<Grid3D>,
    mut state: ResMut<GameState>,
    mut selected: ResMut<SelectedTool>,
    mut view_mode: ResMut<ViewMode>,
    mut inlet_mode: ResMut<InletMode>,
    mut save_events: MessageWriter<SaveRequested>,
    mut load_events: MessageWriter<LoadRequested>,
    mut undo_stack: ResMut<UndoStack>,
    current_level: Res<crate::levels::CurrentLevel>,
    config: Res<GridConfig>,
    active_layer: Res<ActiveLayer>,
    mut dirty: MessageWriter<GridDirty>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera_q.single() else { return };

    let ctrl = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    if mouse.just_pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some((cx, _cy, cz)) = find_cursor_cell_3d(
                cursor_pos, camera, camera_transform, &grid, active_layer.y
            ) {
                state.drag_start = Some((cx, cz));
            }
        }
    }

    if mouse.pressed(MouseButton::Left) {
        if let Some(cursor_pos) = window.cursor_position() {
            if let Some((mut cx, _cy, mut cz)) = find_cursor_cell_3d(
                cursor_pos, camera, camera_transform, &grid, active_layer.y
            ) {
                if shift {
                    if let Some((sx, sz)) = state.drag_start {
                        let dx = (cx as isize - sx as isize).unsigned_abs();
                        let dz = (cz as isize - sz as isize).unsigned_abs();
                        if dx >= dz { cz = sz; } else { cx = sx; }
                    }
                }

                let ay = active_layer.y;
                let r = state.brush_radius as usize;
                let mut placed = false;
                for dz in 0..=(r * 2) {
                    for dx in 0..=(r * 2) {
                        let bx = (cx + dx).saturating_sub(r);
                        let bz = (cz + dz).saturating_sub(r);
                        if bx < grid.width
                            && ay < grid.height
                            && bz < grid.depth
                            && !matches!(grid.get_cell(bx, ay, bz), Cell::Wall | Cell::Rock | Cell::Sand)
                        {
                            let new_cell = match *selected {
                                SelectedTool::Block(w)
                                    if !matches!(grid.get_cell(bx, ay, bz), Cell::Object(_)) =>
                                    Some(Cell::Object(w)),
                                SelectedTool::Eraser => Some(Cell::Air),
                                SelectedTool::Spring
                                    if !matches!(grid.get_cell(bx, ay, bz), Cell::Spring) =>
                                    Some(Cell::Spring),
                                SelectedTool::Drain
                                    if !matches!(grid.get_cell(bx, ay, bz), Cell::Drain) =>
                                    Some(Cell::Drain),
                                SelectedTool::Building { weight, threshold }
                                    if !matches!(grid.get_cell(bx, ay, bz), Cell::Building { .. }) =>
                                    Some(Cell::Building { weight, threshold }),
                                _ => None,
                            };
                            if let Some(new) = new_cell {
                                let old = grid.get_cell(bx, ay, bz).clone();
                                undo_stack.record(bx, ay, bz, old, new.clone());
                                grid.set_cell(bx, ay, bz, new);
                                placed = true;
                            }
                        }
                    }
                }
                if placed { dirty.write(GridDirty); }
            }
        }
    }

    if mouse.just_released(MouseButton::Left) {
        state.drag_start = None;
        if undo_stack.has_pending() { undo_stack.commit(); }
    }

    if ctrl && keyboard.just_pressed(KeyCode::KeyZ) {
        if shift {
            undo_stack.redo(&mut grid);
        } else {
            undo_stack.undo(&mut grid);
        }
        dirty.write(GridDirty);
    }

    if keyboard.just_pressed(KeyCode::Digit1) { *selected = SelectedTool::Block(200.0); }
    if keyboard.just_pressed(KeyCode::Digit2) { *selected = SelectedTool::Block(500.0); }
    if keyboard.just_pressed(KeyCode::Digit3) { *selected = SelectedTool::Block(1000.0); }
    if keyboard.just_pressed(KeyCode::Digit4) { *selected = SelectedTool::Block(2000.0); }
    if keyboard.just_pressed(KeyCode::Digit5) { *selected = SelectedTool::Block(5000.0); }
    if keyboard.just_pressed(KeyCode::KeyE) { *selected = SelectedTool::Eraser; }
    if keyboard.just_pressed(KeyCode::KeyD) && !ctrl { *selected = SelectedTool::Drain; }
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
            InletMode::Sine  => InletMode::Random,
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
        dirty.write(GridDirty);
    }
    if keyboard.just_pressed(KeyCode::KeyM) {
        *view_mode = if *view_mode == ViewMode::Pressure {
            ViewMode::Normal
        } else {
            ViewMode::Pressure
        };
    }
}

fn handle_save(
    mut events: MessageReader<SaveRequested>,
    grid: Res<Grid3D>,
    config: Res<GridConfig>,
    mut pending: ResMut<PendingFileOp>,
) {
    for _ in events.read() {
        if pending.op.is_some() { println!("File dialog already open"); return; }
        pending.op = Some(persistence::save_grid_async(&grid, config.tile_size));
    }
}

fn handle_load(
    mut events: MessageReader<LoadRequested>,
    config: Res<GridConfig>,
    grid: Res<Grid3D>,
    mut pending: ResMut<PendingFileOp>,
) {
    for _ in events.read() {
        if pending.op.is_some() { println!("File dialog already open"); return; }
        pending.op = Some(persistence::load_grid_async(
            config.tile_size,
            grid.width,
            grid.height,
            grid.depth,
        ));
    }
}

fn poll_file_op(
    mut pending: ResMut<PendingFileOp>,
    mut grid: ResMut<Grid3D>,
    mut state: ResMut<GameState>,
    mut dirty: MessageWriter<GridDirty>,
) {
    let Some(ref op) = pending.op else { return };
    let done = match op {
        persistence::PendingIo::Save(rx) => {
            let rx = rx.lock().unwrap();
            match rx.try_recv() {
                Ok(Ok(())) => true,
                Ok(Err(e)) => { println!("Save failed: {e}"); true }
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
                    dirty.write(GridDirty);
                    true
                }
                Ok(Err(e)) => {
                    if e != "Cancelled" { println!("Load failed: {e}"); }
                    true
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => false,
                Err(_) => true,
            }
        }
    };
    if done { pending.op = None; }
}
