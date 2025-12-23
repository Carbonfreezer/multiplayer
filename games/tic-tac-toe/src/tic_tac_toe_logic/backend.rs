//! The back end of tic tac toe. This is the part that only gets executed ont the server side.

use crate::tic_tac_toe_logic::traits_implementation::{ViewState, MoveCommand};
use backbone_lib::traits::{BackEndArchitecture, BackendCommand};

/// The backend logic of tic tac toe is contained here,
pub struct TicTacToeLogic {
    is_host_starting: bool,
    command_list: Vec<BackendCommand<MoveCommand>>,
    view_state: ViewState,
    allow_spectators: bool,
}

impl TicTacToeLogic {
    /// Restarts the game.
    fn reset_game(&mut self) {
        // This happens when we want to restart the game.
        self.command_list.push(BackendCommand::ResetViewState);
        self.view_state = ViewState::new(self.is_host_starting);
    }
}

/// In this case the server RPC struct and the delta information is the same.
impl BackEndArchitecture<MoveCommand, MoveCommand, ViewState> for TicTacToeLogic {
    /// Starts the game rule variation only contains the information, if spectators are allowed.
    fn new(rule_variation: u16) -> Self {
        TicTacToeLogic {
            is_host_starting: true,
            command_list: Vec::new(),
            view_state: ViewState::new(true),
            allow_spectators: rule_variation == 1,
        }
    }

    /// If we do not allow spectators all players beyond index 1 will get rejected.
    fn player_arrival(&mut self, player: u16) {
        if !self.allow_spectators && (player > 1) {
            self.command_list
                .push(BackendCommand::KickPlayer { player });
        }
    }

    /// If player 1, the main playing partner left, the game ends,
    fn player_departure(&mut self, player: u16) {
        if player == 1 {
            self.command_list.push(BackendCommand::TerminateRoom);
        }
    }

    /// Check move for legality and if the game finished set the timer for restart.
    fn inform_rpc(&mut self, _: u16, payload: MoveCommand) {
        if self.view_state.game_state != 0 {
            return;
        }
        // Returns illegal commands.
        if !self.view_state.check_legality(&payload) {
            return;
        }
        self.view_state.apply_move(&payload);
        self.command_list.push(BackendCommand::Delta(payload));
        if self.view_state.game_state != 0 {
            self.command_list.push(BackendCommand::SetTimer {
                timer_id: 0,
                duration: 5.0,
            })
        };
    }

    /// The timers gets triggered when the game should restart.
    fn timer_triggered(&mut self, _: u16) {
        self.is_host_starting = !self.is_host_starting;
        self.reset_game();
    }

    fn get_view_state(&self) -> &ViewState {
        &self.view_state
    }

    fn drain_commands(&mut self) -> Vec<BackendCommand<MoveCommand>> {
        std::mem::take(&mut self.command_list)
    }
}
