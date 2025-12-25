//! The back end of tic-tac-toe. This is the part that only gets executed ont the server side.
//! and implements [`BackEndArchitecture`].

use crate::tic_tac_toe_logic::traits_implementation::{GameState, ViewStateDelta, ViewState, StonePlacement};
use backbone_lib::traits::{BackEndArchitecture, BackendCommand};

/// The backend logic of tic-tac-toe is contained here,
pub struct TicTacToeLogic {
    /// The command list which gets drained by the middle layer.
    command_list: Vec<BackendCommand<ViewStateDelta>>,
    /// The view state, that contains the complete game representation.
    view_state: ViewState,
    /// Indicates if the host is starting the game.
    is_host_starting: bool,
    /// Do we allow spectators in the game?
    allow_spectators: bool,
}

impl TicTacToeLogic {
    /// Restarts the game and requests a reset of the view state on all clients.
    fn reset_game(&mut self) {
        // This happens when we want to restart the game.
        self.command_list.push(BackendCommand::ResetViewState);
        self.view_state = ViewState::new(self.is_host_starting);
    }
}

/// The implementation of [`BackEndArchitecture`]. 
/// The implementations of the diverse components are as follows:
/// 
/// - [`StonePlacement`] contains the command to place a stone at a certain position.
/// - [`ViewStateDelta`] contains the change of the view state to a new game situation.
/// - [`ViewState`] contains the board representation, that is used for visualization and game state checking.
impl BackEndArchitecture<StonePlacement, ViewStateDelta, ViewState> for TicTacToeLogic {
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
    fn inform_rpc(&mut self, player_id : u16, payload: StonePlacement) {
        if self.view_state.game_state != GameState::Pending {
            return;
        }
        // Returns illegal commands.
        if !self.view_state.check_legality(&payload, player_id) {
            return;
        }
        let delta = ViewStateDelta{ is_circle: (player_id == 0), column: payload.column, row: payload.row};
        self.view_state.apply_delta(&delta);
        self.command_list.push(BackendCommand::Delta(delta));
        if self.view_state.game_state != GameState::Pending {
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

    fn drain_commands(&mut self) -> Vec<BackendCommand<ViewStateDelta>> {
        std::mem::take(&mut self.command_list)
    }
}
