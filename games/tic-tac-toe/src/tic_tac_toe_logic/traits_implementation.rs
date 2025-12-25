//! Contains all relevant trait implementations for the system:
//!
//! - [`ViewState`]: The complete representation of the game board.
//! - [`ViewStateDelta`]: The delta information to update the game board.
//! - [`StonePlacement`]: The information of where a tone gets placed. The type of stone is extracted from the player id.

use serde::{Deserialize, Serialize};

/// The delta information for the view state.
#[derive(Clone, Serialize, Deserialize)]
pub struct ViewStateDelta {
    /// Flags if we have a cross or circle.
    pub is_circle: bool,
    /// Flags the column we move.
    pub column: u8,
    /// Flags the row we move.
    pub row: u8,
}

/// This is the rpc payload for stone placement.
#[derive(Clone, Serialize, Deserialize)]
pub struct StonePlacement {
    /// Flags the column we move.
    pub column: u8,
    /// Flags the row we move.
    pub row: u8,
}

/// The game board used as a view state.
#[derive(Clone, Serialize, Deserialize)]
pub struct ViewState {
    /// Contains the raw board 3,3 0: empty 1: cross, 2: circle
    pub board: Vec<Vec<u8>>,
    /// Flags if the next mode is host or not.
    pub next_move_host: bool,
    /// The game state.
    pub game_state: GameState,
}

/// The situation we have, when we are in the game.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub enum GameState {
    Pending,
    CrossWins,
    CircleWins,
    Draw,
}

impl ViewState {
    /// Creates a fresh view state with the indication if the host is the starting player or not.
    pub fn new(is_host_starting: bool) -> ViewState {
        let mut board = Vec::with_capacity(3);
        for _ in 0..3 {
            let column = vec![0_u8, 0_u8, 0_u8];
            board.push(column);
        }

        // Circle starts.
        ViewState {
            board,
            game_state: GameState::Pending,
            next_move_host: is_host_starting,
        }
    }

    /// Applies a change to the game board.
    pub fn apply_delta(&mut self, delta: &ViewStateDelta) {
        self.board[delta.row as usize][delta.column as usize] =
            if delta.is_circle { 2 } else { 1 };
        self.next_move_host = !self.next_move_host;
        self.game_state = self.check_winning();
    }

    /// Checks if the move is legal. This is if it is the correct players turn and the field is still free.
    pub fn check_legality(&self, move_data: &StonePlacement, player_id : u16) -> bool {
        if player_id > 1 {return false}
        if (player_id == 0) != self.next_move_host {
            return false;
        }
        if self.board[move_data.row as usize][move_data.column as usize] != 0 {
            return false;
        }
        true
    }

    /// Does a winning check with the player stone in probe handed over.
    fn check_for(&self, probe: u8) -> bool {
        // Rows
        (0..3).any(|row| (0..3).all(|col| self.board[row][col] == probe))
            // Columns
            || (0..3).any(|col| (0..3).all(|row| self.board[row][col] == probe))
            // Diagonals
            || (0..3).all(|i| self.board[i][i] == probe)
            || (0..3).all(|i| self.board[i][2 - i] == probe)
    }

    /// Checks if we have a game over situation and if so which one.
    pub fn check_winning(&self) -> GameState {
        if self.check_for(1) {
            return GameState::CrossWins;
        }
        if self.check_for(2) {
            return GameState::CircleWins;
        }
        if self.board.iter().flatten().all(|x| *x != 0) {
            return GameState::Draw;
        }
        GameState::Pending
    }
}
