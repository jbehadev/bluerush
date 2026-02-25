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
            ),
        );
    }
}

const TILE_SIZE: f32 = 16.0;
const WINDOW_WIDTH: f32 = 800.0;
const WINDOW_HEIGHT: f32 = 600.0;
const OFFSET_X: f32 = -(WINDOW_WIDTH / 2.0) + (TILE_SIZE / 2.0);
const OFFSET_Y: f32 = -(WINDOW_HEIGHT / 2.0) + (TILE_SIZE / 2.0);

#[derive(Clone, Debug)]
enum Cell {
    Air,
    Water(f32),
    Object(f32),
}

#[derive(Resource)]
struct Grid {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
}

#[derive(Resource)]
struct GameState {
    water_flow: bool,
}

impl Grid {
    fn init(width: usize, height: usize) -> Grid {
        let mut grid = Grid {
            width: width,
            height: height,
            cells: vec![Cell::Air; width * height],
        };

        for x in 0..width {
            grid.set_cell(x, height - 1, Cell::Object(9999.0));
        }
        for y in 0..height {
            grid.set_cell(0, y, Cell::Object(9999.0));
            grid.set_cell(width - 1, y, Cell::Object(9999.0));
        }

        grid.set_cell((width / 2) - 1, 0, Cell::Water(0.5));

        grid
    }

    fn set_cell(&mut self, x: usize, y: usize, cell: Cell) {
        self.cells[y * self.width + x] = cell;
    }

    fn get_cell(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y * self.width + x]
    }
}

#[derive(Component)]
struct Tile {
    x: usize,
    y: usize,
}

fn setup(mut commands: Commands) {
    let width = (WINDOW_WIDTH / TILE_SIZE) as usize;
    let height = (WINDOW_HEIGHT / TILE_SIZE) as usize;

    commands.spawn(Camera2d);
    commands.insert_resource(GameState { water_flow: false });
    commands.insert_resource(Grid::init(width, height));
    for row in 0..height {
        for col in 0..width {
            commands.spawn((
                Sprite::from_color(
                    BLUE,
                    Vec2 {
                        x: (TILE_SIZE),
                        y: (TILE_SIZE),
                    },
                ),
                Transform::from_xyz(
                    OFFSET_X + (col as f32 * TILE_SIZE),
                    OFFSET_Y + (row as f32 * TILE_SIZE),
                    0.0,
                ),
                Tile { x: col, y: row },
            ));
        }
    }
}

fn render_grid(grid: Res<Grid>, mut query: Query<(&Tile, &mut Sprite)>) {
    for (tile, mut sprite) in &mut query {
        sprite.color = match grid.cells[tile.y * grid.width + tile.x] {
            Cell::Air => WHITE.into(),
            Cell::Water(fill) => Color::srgb(1.0 - fill, 1.0 - fill, 1.0),
            Cell::Object(_) => GRAY.into(),
        }
    }
}

fn handle_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    window: Query<&Window>,
    mut grid: ResMut<Grid>,
    mut state: ResMut<GameState>,
) {
    if mouse.pressed(MouseButton::Left) {
        if let Ok(window) = window.single() {
            if let Some(cursor_pos) = window.cursor_position() {
                let world_x = cursor_pos.x - WINDOW_WIDTH / 2.0;
                let world_y = -(cursor_pos.y - WINDOW_HEIGHT / 2.0);
                let grid_x = ((world_x + WINDOW_WIDTH / 2.0) / TILE_SIZE) as usize;
                let grid_y = ((world_y + WINDOW_HEIGHT / 2.0) / TILE_SIZE) as usize;
                grid.set_cell(grid_x, grid_y, Cell::Object(200.0));
            }
        }
    }
    if mouse.just_pressed(MouseButton::Right) {
        if let Ok(window) = window.single() {
            if let Some(cursor_pos) = window.cursor_position() {
                let world_x = cursor_pos.x - WINDOW_WIDTH / 2.0;
                let world_y = -(cursor_pos.y - WINDOW_HEIGHT / 2.0);
                let grid_x = ((world_x + WINDOW_WIDTH / 2.0) / TILE_SIZE) as usize;
                let grid_y = ((world_y + WINDOW_HEIGHT / 2.0) / TILE_SIZE) as usize;
                println!("{grid_x}, {grid_y}: {:?}", *grid.get_cell(grid_x, grid_y));
            }
        }
    }
    if keyboard.just_pressed(KeyCode::KeyX) {
        state.water_flow = !state.water_flow;
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        *grid = Grid::init(grid.width, grid.height);
        state.water_flow = false;
    }
}

fn flow_water(mut grid: ResMut<Grid>, state: Res<GameState>) {
    if !state.water_flow {
        return;
    }
    let flow_rate: f32 = 1.0;
    // read from bottom of grid
    let width = grid.width;
    for x in 1..width - 1 {
        let new_cell = match grid.cells[x] {
            // read
            Cell::Air => Cell::Water(flow_rate),
            Cell::Water(fill) => Cell::Water((fill + flow_rate).min(1.0)),
            Cell::Object(weight) => Cell::Object(weight),
        };
        grid.set_cell(x, 0, new_cell); // write
    }
}

fn water_fill(cell: &Cell) -> Option<f32> {
    match cell {
        Cell::Water(f) => Some(*f),
        _ => None,
    }
}

fn flow_capacity(cell: &Cell) -> Option<f32> {
    match cell {
        Cell::Water(f) => Some(*f),
        Cell::Air => Some(0.0),
        _ => None,
    }
}

fn step_simulation(grid: &Grid) -> Vec<Cell> {
    let width = grid.width;
    let height = grid.height;

    // delta[i] accumulates the net change in water for each cell this tick.
    // Positive = gaining water, negative = losing water.
    let mut delta = vec![0.0f32; width * height];

    for y in 0..height {
        for x in 1..width - 1 {
            let idx = y * width + x;

            let fill = match water_fill(&grid.cells[idx]) {
                Some(f) => f,
                None => continue,
            };

            // For each passable neighbor, calculate transfer and record it
            // as a loss for this cell and a gain for the neighbor.
            let neighbors: &[(isize, isize)] = &[(0, -1), (0, 1), (-1, 0), (1, 0)];
            for (dx, dy) in neighbors {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                    continue;
                }
                let nidx = ny as usize * width + nx as usize;
                if let Some(neighbor_fill) = flow_capacity(&grid.cells[nidx]) {
                    let transfer = (fill - neighbor_fill).max(0.0) * 0.25;
                    delta[idx] -= transfer; // this cell loses water
                    delta[nidx] += transfer; // neighbor gains water
                }
            }
        }
    }

    // Apply all deltas at once to produce the new cell state
    let mut new_cells = grid.cells.clone();
    for i in 0..new_cells.len() {
        if delta[i] == 0.0 {
            continue;
        }
        let current = flow_capacity(&grid.cells[i]).unwrap_or(0.0);
        let new_fill = (current + delta[i]).clamp(0.0, 1.0);
        new_cells[i] = if new_fill < 0.001 {
            Cell::Air
        } else {
            Cell::Water(new_fill)
        };
    }

    new_cells
}

fn water_pressure(cell: &Cell) -> f32 {
    match cell {
        Cell::Water(f) => *f,
        _ => 0.0,
    }
}

fn object_weight(cell: &Cell) -> Option<f32> {
    match cell {
        Cell::Object(w) => Some(*w),
        _ => None,
    }
}

// Represents an object's intention to move this tick.
#[derive(Debug)]
struct MoveIntent {
    src: usize,    // index of the object's current cell
    dst: usize,    // index of the cell it wants to move into
    weight: f32,   // the object's weight
    pressure: f32, // pressure from the pushing side (fills the vacated cell)
}

fn step_objects(grid: &Grid) -> Vec<Cell> {
    let width = grid.width;
    let height = grid.height;

    // Pass 1: collect all intended moves without writing anything yet.
    let mut intents: Vec<MoveIntent> = Vec::new();
    //println!("Step Object cycle running");
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;

            let weight = match object_weight(&grid.cells[idx]) {
                Some(w) => w,
                None => continue,
            };

            let p_left = if x > 0 {
                water_pressure(&grid.cells[y * width + (x - 1)])
            } else {
                0.0
            };
            let p_right = if x < width - 1 {
                water_pressure(&grid.cells[y * width + (x + 1)])
            } else {
                0.0
            };
            let p_below = if y > 0 {
                water_pressure(&grid.cells[(y - 1) * width + x])
            } else {
                0.0
            };
            let p_above = if y < height - 1 {
                water_pressure(&grid.cells[(y + 1) * width + x])
            } else {
                0.0
            };

            // if the y force below is greater than 10% of the y force above, move up one

            let x_force = p_left - p_right;
            let y_force = p_below - p_above;

            let threshold = 0.1;
            let dx = if x_force.abs() > threshold {
                x_force.signum() as isize
            } else {
                0
            };
            let dy = if y_force.abs() > threshold {
                y_force.signum() as isize
            } else {
                0
            };

            let force_kg = x_force.abs().max(y_force.abs()) * 1000.0;
            let pushing_pressure = x_force.abs().max(y_force.abs());

            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                continue;
            }
            let nidx = ny as usize * width + nx as usize;

            if force_kg <= weight {
                continue;
            }

            let can_move = matches!(&grid.cells[nidx], Cell::Air | Cell::Water(_));

            if !can_move {
                continue;
            }

            intents.push(MoveIntent {
                src: idx,
                dst: nidx,
                weight,
                pressure: pushing_pressure,
            });
        }
    }

    // Pass 2: detect conflicts — if two objects want the same destination, neither moves.
    // Build a list of destination indices that are contested.
    let mut dst_counts = vec![0u32; width * height];
    for intent in &intents {
        dst_counts[intent.dst] += 1;
    }

    // Pass 3: apply only conflict-free moves.
    let mut new_cells = grid.cells.clone();
    for intent in &intents {
        if dst_counts[intent.dst] > 1 || dst_counts[intent.src] > 0 {
            // Contested destination — skip this move entirely.
            continue;
        }
        new_cells[intent.dst] = Cell::Object(intent.weight);
        if dst_counts[intent.src] == 0 {
            new_cells[intent.src] = grid.cells[intent.dst].clone();
        }
        println!("{:?}", intent);
    }

    new_cells
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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build a small grid manually for testing
    fn make_grid(width: usize, height: usize, cells: Vec<Cell>) -> Grid {
        Grid {
            width,
            height,
            cells,
        }
    }

    #[test]
    fn object_pushed_by_water() {
        // 3x3 grid:
        //   y=2: [Air,     Air,      Air]
        //   y=1: [Air,     Object,   Air]   ← object at (1,1), weight 10.0
        //   y=0: [Air,     Water,    Air]   ← water at (1,0), fill 1.0 = 1000kg
        //
        // Water below pushes upward. Force = 1.0 * 1000 = 1000kg > 10kg weight.
        // Object should move from (1,1) to (1,2). Source (1,1) should become water.
        let mut cells = vec![Cell::Air; 9];
        cells[0 * 3 + 1] = Cell::Water(1.0); // (1,0)
        cells[1 * 3 + 1] = Cell::Object(10.0); // (1,1)
        let grid = make_grid(3, 3, cells);

        let result = step_objects(&grid);

        // Object should have moved to (1,2)
        assert!(
            matches!(result[2 * 3 + 1], Cell::Object(_)),
            "Object should have moved to (1,2)"
        );
        // Source (1,1) should now be water
        assert!(
            matches!(result[1 * 3 + 1], Cell::Air),
            "Vacated cell (1,1) should be air"
        );
    }

    #[test]
    fn heavy_object_stays_put() {
        // Same layout but object weighs 2000kg — more than 1000kg of water pressure.
        let mut cells = vec![Cell::Air; 9];
        cells[0 * 3 + 1] = Cell::Water(1.0);
        cells[1 * 3 + 1] = Cell::Object(2000.0);
        let grid = make_grid(3, 3, cells);

        let result = step_objects(&grid);

        // Object should NOT have moved
        assert!(
            matches!(result[1 * 3 + 1], Cell::Object(_)),
            "Heavy object should stay at (1,1)"
        );
        // Cell above should still be air
        assert!(
            matches!(result[2 * 3 + 1], Cell::Air),
            "Cell above should remain air"
        );
    }

    #[test]
    fn water_spreads_to_air_neighbor() {
        // 3x3 grid, water at center (x=1, y=1), everything else Air
        let mut cells = vec![Cell::Air; 9];
        cells[1 * 3 + 1] = Cell::Water(1.0);
        let grid = make_grid(3, 3, cells);

        let result = step_simulation(&grid);

        let above_idx = 2 * 3 + 1;
        let has_water_above = matches!(result[above_idx], Cell::Water(f) if f > 0.0);
        assert!(has_water_above, "Water should have spread upward to (1,2)");
    }

    #[test]
    fn water_is_conserved_over_ticks() {
        // 5x5 grid with walls on edges, water in the middle
        // Walls at x=0, x=4, y=0, y=4 — water at (2,2)
        let mut cells = vec![Cell::Air; 25];
        for x in 0..5 {
            cells[0 * 5 + x] = Cell::Object(9999.0);
        }
        for x in 0..5 {
            cells[4 * 5 + x] = Cell::Object(9999.0);
        }
        for y in 0..5 {
            cells[y * 5 + 0] = Cell::Object(9999.0);
        }
        for y in 0..5 {
            cells[y * 5 + 4] = Cell::Object(9999.0);
        }
        cells[2 * 5 + 2] = Cell::Water(1.0);

        let mut grid = make_grid(5, 5, cells);

        // Count total water before
        let total_before: f32 = grid
            .cells
            .iter()
            .filter_map(|c| {
                if let Cell::Water(f) = c {
                    Some(*f)
                } else {
                    None
                }
            })
            .sum();

        // Run 10 ticks
        for _ in 0..10 {
            grid.cells = step_simulation(&grid);
        }

        // Count total water after
        let total_after: f32 = grid
            .cells
            .iter()
            .filter_map(|c| {
                if let Cell::Water(f) = c {
                    Some(*f)
                } else {
                    None
                }
            })
            .sum();

        println!("Water before: {total_before:.4}, after: {total_after:.4}");
        assert!(
            (total_before - total_after).abs() < 0.1,
            "Water should be conserved, lost {:.4}",
            total_before - total_after
        );
    }

    #[test]
    fn vertical_line_no_merge() {
        // 5x3 grid (width=1 won't work due to wall logic, use width=3):
        // y=4: Air
        // y=3: Object(200.0)
        // y=2: Object(200.0)
        // y=1: Object(200.0)
        // y=0: Water(1.0)   ← pushes upward
        let mut cells = vec![Cell::Air; 5 * 3];
        cells[0 * 3 + 1] = Cell::Water(1.0); // y=0
        cells[1 * 3 + 1] = Cell::Object(200.0); // y=1
        cells[2 * 3 + 1] = Cell::Object(200.0); // y=2
        cells[3 * 3 + 1] = Cell::Object(200.0); // y=3
        let grid = make_grid(3, 5, cells);

        let result = step_objects(&grid);

        // No cell should contain two objects — check count of Object cells
        let object_count = result
            .iter()
            .filter(|c| matches!(c, Cell::Object(_)))
            .count();
        assert_eq!(object_count, 3, "Should still have exactly 3 objects");
    }
}
