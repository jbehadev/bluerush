use rand::Rng;

pub const MAX_WATER_KG: f32 = 1000.0;

#[derive(Clone, Debug)]
pub enum Cell {
    Air,
    Water(f32),
    Object(f32),
    Wall,
}

pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
}

impl Grid {
    pub fn init(width: usize, height: usize) -> Grid {
        let mut grid = Grid {
            width,
            height,
            cells: vec![Cell::Air; width * height],
        };

        for x in 0..width {
            grid.set_cell(x, height - 1, Cell::Wall);
        }
        for y in 0..height {
            grid.set_cell(0, y, Cell::Wall);
            grid.set_cell(width - 1, y, Cell::Wall);
        }

        // Gate row: y=1 interior cells start as Wall, opened by animate_gate
        if height > 1 {
            for x in 1..width - 1 {
                grid.set_cell(x, 1, Cell::Wall);
            }
        }

        // Pre-fill y=0 as the reservoir — the gate at y=1 controls flow into the simulation.
        for x in 1..width - 1 {
            grid.set_cell(x, 0, Cell::Water(MAX_WATER_KG));
        }

        grid
    }

    pub fn set_cell(&mut self, x: usize, y: usize, cell: Cell) {
        self.cells[y * self.width + x] = cell;
    }

    pub fn get_cell(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y * self.width + x]
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

pub fn step_simulation(grid: &Grid) -> Vec<Cell> {
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
        let new_kg = (current + delta[i]).clamp(0.0, MAX_WATER_KG);
        new_cells[i] = if new_kg < 1.0 {
            Cell::Air
        } else {
            Cell::Water(new_kg)
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

pub fn build_depth_pressure(grid: &Grid) -> Vec<f32> {
    let width = grid.width;
    let height = grid.height;
    let mut depth = vec![0.0f32; width * height];
    let decay: f32 = 0.1f32.powf(1.0 / 10.0);
    let inlet_pressure: f32 = MAX_WATER_KG * 3.0;

    for x in 0..width {
        let mut water_below: Vec<(f32, usize)> = vec![(inlet_pressure, 0)]; // inlet as seed
        for y in 0..height {
            let pressure: f32 = water_below
                .iter()
                .map(|&(kg, wy)| kg * decay.powi(y as i32 - wy as i32))
                .sum();

            match grid.cells[y * width + x] {
                Cell::Water(kg) => {
                    depth[y * width + x] = pressure;
                    water_below.push((kg, y));
                }
                Cell::Object(weight) => {
                    depth[y * width + x] = (pressure - weight).max(0.0);
                    // don't push
                }
                Cell::Wall => {
                    depth[y * width + x] = 0.0;
                    // immovable — don't modify water_below
                }
                Cell::Air => {
                    water_below.clear();
                    depth[y * width + x] = 0.0;
                }
            }
        }
    }
    depth
}

pub fn step_objects(grid: &Grid) -> Vec<Cell> {
    let width = grid.width;
    let height = grid.height;

    // Build depth-based pressure table so cells deeper in the water column
    // feel higher pressure regardless of local fill level equalisation.
    let depth = build_depth_pressure(grid);

    // Pass 1: collect all intended moves without writing anything yet.
    let mut intents: Vec<MoveIntent> = Vec::new();
    for y in (0..height).rev() {
        for x in 0..width {
            let idx = y * width + x;

            let weight = match object_weight(&grid.cells[idx]) {
                Some(w) => w,
                None => continue,
            };

            // Horizontal pressure: use depth[] from adjacent water cells,
            // consistent with the vertical buoyancy calculation.
            let p_left = if x > 0 {
                match &grid.cells[y * width + (x - 1)] {
                    Cell::Water(_) => depth[y * width + (x - 1)],
                    _ => 0.0,
                }
            } else {
                0.0
            };
            let p_right = if x < width - 1 {
                match &grid.cells[y * width + (x + 1)] {
                    Cell::Water(_) => depth[y * width + (x + 1)],
                    _ => 0.0,
                }
            } else {
                0.0
            };
            let x_force = p_left - p_right;

            // Vertical pressure: depth[idx] for objects is (raw_upward_pressure - weight).max(0.0).
            // A positive value means buoyancy exceeds the object's weight — no need
            // to compare against weight again.
            let y_force = depth[idx];

            // Both net forces are now comparable: y_force already has weight subtracted
            // (via depth[]), so subtract weight from x_force the same way.
            let net_x = (x_force.abs() - weight).max(0.0);
            let net_y = y_force;

            let threshold = 0.1;
            let (dx, dy) = if net_y >= net_x {
                (0isize, if net_y > threshold { 1isize } else { 0 })
            } else {
                (if net_x > threshold { x_force.signum() as isize } else { 0 }, 0isize)
            };

            if dx == 0 && dy == 0 {
                continue;
            }

            let pushing_pressure = net_x.max(net_y);

            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                continue;
            }
            let nidx = ny as usize * width + nx as usize;

            intents.push(MoveIntent {
                src: idx,
                dst: nidx,
                weight,
                pressure: pushing_pressure,
            });
        }
    }

    // Pass 2: group intents by destination.
    // For each destination, if multiple objects want it, pick one randomly.
    let mut by_dst: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, intent) in intents.iter().enumerate() {
        by_dst.entry(intent.dst).or_default().push(i);
    }

    // Build a set of winning intent indices — one per destination, chosen randomly.
    let mut rng = rand::thread_rng();
    let mut winners: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for candidates in by_dst.values() {
        let winner = candidates[rng.r#gen::<usize>() % candidates.len()];
        winners.insert(winner);
    }

    // Pass 3: apply winning moves, skipping any whose src is another winner's dst.
    // This prevents a winner from overwriting a cell that another winner is vacating into.
    let mut moved_srcs: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Need to sort by y axis with top-down order
    let mut sorted_winners: Vec<usize> = winners.into_iter().collect();
    sorted_winners.sort_by(|&a, &b| intents[b].src.cmp(&intents[a].src));
    let mut new_cells = grid.cells.clone();
    for &i in &sorted_winners {
        let intent = &intents[i];
        if matches!(new_cells[intent.dst], Cell::Wall) {
            continue; // walls are always impassable
        }
        if matches!(new_cells[intent.dst], Cell::Object(_)) && !moved_srcs.contains(&intent.dst) {
            continue; // dst is occupied by an object that hasn't moved away yet
        }
        let vacated = new_cells[intent.dst].clone();
        new_cells[intent.dst] = Cell::Object(intent.weight);
        new_cells[intent.src] = vacated;
        moved_srcs.insert(intent.src);
    }

    new_cells
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
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG); // (1,0)
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
        let mut cells = vec![Cell::Air; 15];
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG);
        cells[1 * 3 + 1] = Cell::Water(MAX_WATER_KG);
        cells[2 * 3 + 1] = Cell::Object(4000.0);
        let grid = make_grid(3, 5, cells);

        let result = step_objects(&grid);

        // Object should NOT have moved
        assert!(
            matches!(result[2 * 3 + 1], Cell::Object(_)),
            "Heavy object should stay at (2,1)"
        );
        // Cell above should still be air
        assert!(
            matches!(result[3 * 3 + 1], Cell::Air),
            "Cell above should remain air"
        );
    }

    #[test]
    fn water_spreads_to_air_neighbor() {
        // 3x3 grid, water at center (x=1, y=1), everything else Air
        let mut cells = vec![Cell::Air; 9];
        cells[1 * 3 + 1] = Cell::Water(MAX_WATER_KG);
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
            cells[0 * 5 + x] = Cell::Wall;
        }
        for x in 0..5 {
            cells[4 * 5 + x] = Cell::Wall;
        }
        for y in 0..5 {
            cells[y * 5 + 0] = Cell::Wall;
        }
        for y in 0..5 {
            cells[y * 5 + 4] = Cell::Wall;
        }
        cells[2 * 5 + 2] = Cell::Water(MAX_WATER_KG);

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
        let mut cells = vec![Cell::Air; 5 * 3];
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG); // y=0
        cells[1 * 3 + 1] = Cell::Object(200.0); // y=1
        cells[2 * 3 + 1] = Cell::Object(200.0); // y=2
        cells[3 * 3 + 1] = Cell::Object(200.0); // y=3
        let grid = make_grid(3, 5, cells);

        let result = step_objects(&grid);

        let object_count = result
            .iter()
            .filter(|c| matches!(c, Cell::Object(_)))
            .count();
        assert_eq!(object_count, 3, "Should still have exactly 3 objects");
    }

    #[test]
    fn hold_the_line() {
        let mut cells = vec![Cell::Air; 5 * 3];
        cells[0 * 3 + 0] = Cell::Water(MAX_WATER_KG); // y=0
        cells[0 * 3 + 1] = Cell::Water(MAX_WATER_KG); // y=0
        cells[0 * 3 + 2] = Cell::Water(MAX_WATER_KG); // y=0
        cells[1 * 3 + 0] = Cell::Water(MAX_WATER_KG); // y=1
        cells[1 * 3 + 1] = Cell::Water(MAX_WATER_KG); // y=1
        cells[1 * 3 + 2] = Cell::Water(MAX_WATER_KG); // y=1
        cells[2 * 3 + 0] = Cell::Object(200.0); // y=2
        cells[2 * 3 + 1] = Cell::Object(200.0); // y=2
        cells[2 * 3 + 2] = Cell::Object(200.0); // y=2
        let grid = make_grid(3, 5, cells);

        let result = step_objects(&grid);

        let object_count = result
            .iter()
            .filter(|c| matches!(c, Cell::Object(_)))
            .count();
        assert_eq!(object_count, 3, "Should still have exactly 3 objects");
        assert!(
            !matches!(result[2 * 3 + 0], Cell::Object(_)),
            "not object at (0,2)"
        );
        assert!(
            !matches!(result[2 * 3 + 1], Cell::Object(_)),
            "not object at (1,2)"
        );
        assert!(
            !matches!(result[2 * 3 + 2], Cell::Object(_)),
            "not object at (2,2)"
        );
        assert!(
            matches!(result[3 * 3 + 0], Cell::Object(_)),
            "object at (0,3)"
        );
        assert!(
            matches!(result[3 * 3 + 1], Cell::Object(_)),
            "object at (1,3)"
        );
        assert!(
            matches!(result[3 * 3 + 2], Cell::Object(_)),
            "object at (2,3)"
        );
    }
}
