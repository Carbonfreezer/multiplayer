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
pub struct GameBoard {
    /// Contains the raw board 3,3 0: empty 1: cross, 2: circle
    pub board: Vec<Vec<u8>>,
    /// Flags if the next mode is host or not.
    pub next_move_host: bool,
    /// The game state. 0 : pending 1 : cross wins 2 : circle wins 3 : draw
    pub game_state: u8,
}

impl GameBoard {
    pub fn new(is_host_starting: bool) -> GameBoard {
        let mut board = Vec::with_capacity(3);
        for _ in 0..3 {
            let column = vec![0_u8, 0_u8, 0_u8];
            board.push(column);
        }

        // Circle starts.
        GameBoard {
            board,
            game_state: 0,
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

    /// Checks if the move is legal.
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
        // Check all rows.
        for row in 0..3 {
            let mut all_probe = true;
            for col in 0..3 {
                if self.board[row][col] != probe {
                    all_probe = false;
                }
            }
            if all_probe {
                return true;
            }
        }
        // Check all columns
        for col in 0..3 {
            let mut all_probe = true;
            for row in 0..3 {
                if self.board[row][col] != probe {
                    all_probe = false;
                }
            }
            if all_probe {
                return true;
            }
        }
        // Diag 1
        let mut all_probe = true;
        for i in 0..3 {
            if self.board[i][i] != probe {
                all_probe = false;
            }
        }
        if all_probe {
            return true;
        }
        all_probe = true;
        for i in 0..3 {
            if self.board[i][2 - i] != probe {
                all_probe = false;
            }
        }
        all_probe
    }

    /// 0 : pending 1 : cross wins 2 : circle wins 3 : draw
    pub fn check_winning(&self) -> u8 {
        if self.check_for(1) {
            return 1;
        }
        if self.check_for(2) {
            return 2;
        }
        if self.board.iter().flatten().all(|x| *x != 0) {
            return 3;
        }
        0
    }
}
