//! The view state, delta and rpc logic.

use crate::board_logic::board_representation::{NUM_OF_COLORS, StoneColor, StonePlacement};
use crate::network_logic::basic_commands::GameState::Move;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
/// The different game states we may be in during the lifecycle of the game-
pub enum GameState {
    /// We are waiting for three players to be completely arrived.
    AwaitingPlayers,
    /// One player assigns the colors to the different names.
    AssigningPlayers,
    /// We are waiting for the player with the indicated color to make a move.
    Move(StoneColor),
    /// The game is over.
    GameOver,
}

impl GameState {
    /// Gets the current color that is moving, if we are in a moving phase (None otherwise).
    pub fn current_move_color(&self) -> Option<StoneColor> {
        match self {
            Move(color) => Some(*color),
            _ => None,
        }
    }
}

/// The different RPC we can do.
#[derive(Serialize, Deserialize, Clone)]
pub enum RpcPayload {
    /// Sets the player name.
    SetPlayerName(String),
    /// Sets the player colors for three players (0,1,2).
    SetPlayerColors([StoneColor; NUM_OF_COLORS]),
    /// The command to make a move.
    MakeMove(StonePlacement),
}

/// The delta information that can get transmitted for view state changes.
#[derive(Serialize, Deserialize, Clone)]
pub enum DeltaInformation {
    /// Sets the choice state of the next state (eg. whose move is next, game over, ...).
    SetGameState(GameState),
    /// Sets the names of the players
    SetPlayerNames([String; NUM_OF_COLORS]),
    /// Sets the colors of the players.
    SetPlayerColors([StoneColor; NUM_OF_COLORS]),
    /// Makes a move command.
    MakeMove(StonePlacement),
}
