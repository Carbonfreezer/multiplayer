//! The view state as needed by the system. This is the central data structure that gets synchronized.

use crate::board_logic::board_representation::{GameBoard, NUM_OF_COLORS, StoneColor};
use crate::network_logic::basic_commands::GameState::{AssigningPlayers, AwaitingPlayers, Move};
use crate::network_logic::basic_commands::{DeltaInformation, GameState, RpcPayload};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ViewState {
    /// The central game board.
    pub game_board: GameBoard,
    /// The name of the three players.
    pub player_names: [String; NUM_OF_COLORS],
    /// The colors the players have.
    pub player_colors: [StoneColor; NUM_OF_COLORS],
    /// The overall state we are currently in.
    pub game_state: GameState,
}

impl ViewState {
    /// We start by awaiting players.
    pub fn new() -> Self {
        let mut game_board = GameBoard::new();
        game_board.reset_board();
        ViewState {
            game_board,
            player_names: [String::from(""), String::from(""), String::from("")],
            player_colors: [StoneColor::Red, StoneColor::Green, StoneColor::Blue],
            game_state: AwaitingPlayers,
        }
    }

    /// Asks for the player names in the sequence of the player colors rgb.
    pub fn get_player_names_in_rgb_sequence(&self) -> [String; NUM_OF_COLORS] {
        use StoneColor::*;
        [Red, Green, Blue].map(|color| {
            let idx = self.player_colors.iter().position(|c| *c == color).unwrap();
            self.player_names[idx].clone()
        })
    }

    /// The reset recreates the game board but leaves nicknames intact. We start again by reassigning players.
    pub fn reset(&mut self) {
        self.game_board.reset_board();
        self.game_state = AssigningPlayers;
    }

    /// Checks if the current move is legal to execute. Legality depends on the current game state and for the
    /// move command, if it is coming from the right player with the right color for a legal placement.
    pub fn check_legal_execution(&self, player_id: u16, rpc_payload: &RpcPayload) -> bool {
        // We should only have three players, but we do a safety check here.
        if player_id >= NUM_OF_COLORS as u16 {
            return false;
        }
        match rpc_payload {
            RpcPayload::SetPlayerName(_) => self.game_state == AwaitingPlayers,
            RpcPayload::SetPlayerColors(_) => {
                player_id == 0 && (self.game_state == AssigningPlayers)
            }
            RpcPayload::MakeMove(move_command) => {
                (self.player_colors[player_id as usize] == move_command.stone_color)
                    && (self.game_board.is_legal_move(
                        move_command.field_position.clone(),
                        move_command.stone_color,
                    ))
                    && matches!(self.game_state, Move(stone_color) if move_command.stone_color == stone_color)
            }
        }
    }

    /// Applies a known information coming from the server. This is game state changing, player names or
    /// color changing or making a move.
    pub fn apply_delta(&mut self, delta: &DeltaInformation) {
        match delta {
            DeltaInformation::SetGameState(game_state) => {
                self.game_state = *game_state;
            }
            DeltaInformation::SetPlayerNames(names) => {
                self.player_names = names.clone();
            }
            DeltaInformation::SetPlayerColors(colors) => {
                self.player_colors = *colors;
            }
            DeltaInformation::MakeMove(move_command) => self
                .game_board
                .set_stone(&move_command.field_position, move_command.stone_color),
        }
    }
}
