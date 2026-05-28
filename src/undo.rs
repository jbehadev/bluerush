use bevy::prelude::*;

use crate::simulation::Cell;
use crate::simulation3d::Grid3D;

/// A before/after snapshot of a single cell.
#[derive(Clone)]
pub struct CellChange {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub old: Cell,
    pub new: Cell,
}

/// A single undoable user action.
pub struct Action {
    pub changes: Vec<CellChange>,
}

/// Undo/redo history for user edits. Capped at `MAX_UNDO` actions.
#[derive(Resource, Default)]
pub struct UndoStack {
    undo: Vec<Action>,
    redo: Vec<Action>,
    pending: Vec<CellChange>,
}

const MAX_UNDO: usize = 100;

impl UndoStack {
    pub fn record(&mut self, x: usize, y: usize, z: usize, old: Cell, new: Cell) {
        if old == new { return; }
        self.pending.push(CellChange { x, y, z, old, new });
    }

    pub fn commit(&mut self) {
        if self.pending.is_empty() { return; }
        let changes = std::mem::take(&mut self.pending);
        self.undo.push(Action { changes });
        self.redo.clear();
        if self.undo.len() > MAX_UNDO { self.undo.remove(0); }
    }

    pub fn undo(&mut self, grid: &mut Grid3D) {
        if let Some(action) = self.undo.pop() {
            for change in &action.changes {
                grid.set_cell(change.x, change.y, change.z, change.old.clone());
            }
            self.redo.push(action);
        }
    }

    pub fn redo(&mut self, grid: &mut Grid3D) {
        if let Some(action) = self.redo.pop() {
            for change in &action.changes {
                grid.set_cell(change.x, change.y, change.z, change.new.clone());
            }
            self.undo.push(action);
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.pending.clear();
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}
