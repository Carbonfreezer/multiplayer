//! The backend logic for ternio. All relevant game logic is concentrated here.

use crate::board_logic::board_representation::NUM_OF_COLORS;
use crate::board_logic::board_representation::StoneColor::Red;
use crate::network_logic::basic_commands::GameState;
use crate::network_logic::basic_commands::{DeltaInformation, RpcPayload};
use crate::network_logic::view_state::ViewState;
use backbone_lib::traits::BackendCommand::{Delta, SetTimer};
use backbone_lib::traits::{BackEndArchitecture, BackendCommand};

/// The backend module for the transport layer.
pub struct TernioLogic {
    /// The list with the commands we sent to the transport layer.
    command_list: Vec<BackendCommand<DeltaInformation>>,
    /// The view state we have on the server side, that contains all relevant information.
    view_state: ViewState,
    /// The names of the three players if set. This is only done once, even if the game restarts.
    player_names: [Option<String>; NUM_OF_COLORS],
}

impl BackEndArchitecture<RpcPayload, DeltaInformation, ViewState> for TernioLogic {
    /// We do not have any rule variations here.
    fn new(_: u16) -> Self {
        TernioLogic {
            command_list: Vec::new(),
            view_state: ViewState::new(),
            player_names: [None, None, None],
        }
    }

    /// No required action on player arrival. The name setting comes with a separate RPC.
    /// For safety reasons we only check here, if we have too many players.
    fn player_arrival(&mut self, player_id: u16) {
        if player_id >= NUM_OF_COLORS as u16 {
            self.command_list
                .push(BackendCommand::KickPlayer { player: player_id });
        }
    }

    /// As soon as a player leaves, we terminate the room as we can not continue the game.
    fn player_departure(&mut self, player_id: u16) {
        // If our partner leaves, we cancel the room.
        if player_id < NUM_OF_COLORS as u16 {
            self.command_list.push(BackendCommand::TerminateRoom);
        }
    }

    /// The different RPCs from he players with the indicated id get processed here.
    /// These are **SetPlayerName** for the name of a single player, **SetPlayerColors** to set
    /// all colors of all players, **MakeMove** to place a stone. The legality of actions is checked upfront.
    fn inform_rpc(&mut self, player: u16, payload: RpcPayload) {
        // Here we need to do a validity check.
        if !self.view_state.check_legal_execution(player, &payload) {
            return;
        }
        match payload {
            RpcPayload::SetPlayerName(player_name) => {
                self.player_names[player as usize] = Some(player_name);
                if let [Some(first), Some(second), Some(third)] = &self.player_names {
                    let delta = DeltaInformation::SetPlayerNames([
                        first.clone(),
                        second.clone(),
                        third.clone(),
                    ]);
                    self.view_state.apply_delta(&delta);
                    self.command_list.push(Delta(delta));
                    let delta = DeltaInformation::SetGameState(GameState::AssigningPlayers);
                    self.view_state.apply_delta(&delta);
                    self.command_list.push(Delta(delta));
                }
            }
            RpcPayload::SetPlayerColors(player_colors) => {
                let delta = DeltaInformation::SetPlayerColors(player_colors);
                self.view_state.apply_delta(&delta);
                self.command_list.push(Delta(delta));
                // Now the red player starts.
                let delta = DeltaInformation::SetGameState(GameState::Move(Red));
                self.view_state.apply_delta(&delta);
                self.command_list.push(Delta(delta));
            }
            RpcPayload::MakeMove(move_command) => {
                let delta = DeltaInformation::MakeMove(move_command);
                self.view_state.apply_delta(&delta);
                self.command_list.push(Delta(delta));
                // Now we have to see how to continue.
                let current_color = self
                    .view_state
                    .game_state
                    .current_move_color()
                    .expect("Should have been checked before.");
                let next_phase = current_color
                    .cycle_from_next()
                    .into_iter()
                    .find(|color| {
                        !self
                            .view_state
                            .game_board
                            .get_all_legal_moves(*color)
                            .is_empty()
                    })
                    .map(GameState::Move)
                    .unwrap_or(GameState::GameOver);

                let delta = DeltaInformation::SetGameState(next_phase);
                self.view_state.apply_delta(&delta);
                self.command_list.push(Delta(delta));
                // Set the timer for restart.
                if next_phase == GameState::GameOver {
                    self.command_list.push(SetTimer {
                        timer_id: 0,
                        duration: 15.0,
                    })
                }
            }
        }
    }

    /// There is only one timer, and that is the one that restarts the game after a game ending.
    fn timer_triggered(&mut self, _: u16) {
        // Simply reset the game.
        self.view_state.reset();
        self.command_list.push(BackendCommand::ResetViewState);
    }

    fn get_view_state(&self) -> &ViewState {
        &self.view_state
    }

    fn drain_commands(&mut self) -> Vec<BackendCommand<DeltaInformation>> {
        std::mem::take(&mut self.command_list)
    }
}
