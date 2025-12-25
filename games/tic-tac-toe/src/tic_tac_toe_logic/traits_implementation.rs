//! Contains the view state implementation and the command that is used as a delta information and
//! Server RPC command at the same time here.

use serde::{Deserialize, Serialize};

/// The rpc payload and the delta information is the same in our case.
#[derive(Clone, Serialize, Deserialize)]
pub struct MoveCommand {
    /// Flags if we have a cross or circle.
    pub is_host: bool,
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

    /// Applies a move to the game board.
    pub fn apply_move(&mut self, move_data: &MoveCommand) {
        self.board[move_data.row as usize][move_data.column as usize] =
            if move_data.is_host { 2 } else { 1 };
        self.next_move_host = !self.next_move_host;
        self.game_state = self.check_winning();
    }

    /// Checks if the move is legal. This is if it is the correct players turn and the field is still free.
    pub fn check_legality(&self, move_data: &MoveCommand) -> bool {
        if move_data.is_host != self.next_move_host {
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

    /// 0 : pending 1 : cross wins 2 : circle wins 3 : draw
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
