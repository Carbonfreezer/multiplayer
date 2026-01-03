//! Contains the real game logic except for the main function.

use crate::board_logic::board_and_transition::{
    BufferedBoardForRendering, PresentationState, TransitionBoard,
};
use crate::board_logic::board_representation::{FieldPosition, StonePlacement};
use crate::network_logic::back_end::TernioLogic;
use crate::network_logic::basic_commands::{DeltaInformation, GameState, RpcPayload};
use crate::network_logic::view_state::ViewState;
use crate::render_system::gui::{AssignmentResult, PlayerAssignmentGui, StartupGui, StartupResult};
use crate::render_system::media::{CELL_SIZE, Media};
use backbone_lib::transport_layer::TransportLayer;
use backbone_lib::transport_layer::ViewStateUpdate::{Full, Incremental};
use macroquad::camera::Camera2D;
use macroquad::input::{MouseButton, is_mouse_button_pressed, mouse_position};
use macroquad::math::Vec2;

/// The point where we draw status information.
pub const TEXT_POINT_STATUS_INFO: Vec2 = Vec2 { x: 250.0, y: 620.0 };

/// Shortcut for the complete type of the transport layer.
pub type TernioSystem = TransportLayer<RpcPayload, DeltaInformation, TernioLogic, ViewState>;

/// Contains the complete data for the game.
pub struct GlobalData {
    /// The summary of the media data.
    pub media: Media,
    /// The view state as used on the client side, this is the synchronized game status.
    pub view_state: ViewState,
    /// The presentation state for drawing the board. Basically doing nothing, showing the static situation or performing animation.
    pub presentation_state: PresentationState,
    /// The complete transport layer.
    pub net_architecture: TernioSystem,
    /// A buffer for the player name from the start_up_gui, that still has to be sent to the server.
    pub pending_player_name: Option<String>,
    /// The GUI shown on startup.
    start_up_gui: StartupGui,
    /// The assignment GUI for the players, only gets instantiated on host side.
    player_assignment_gui: Option<PlayerAssignmentGui>,
    /// Macroquad camera for rendering.
    camera: Camera2D,
}

impl GlobalData {
    /// Construction is async because of the sound loading.
    pub async fn new(architecture: TernioSystem, camera: Camera2D) -> GlobalData {
        GlobalData {
            media: Media::new().await,
            view_state: ViewState::new(),
            presentation_state: PresentationState::None,
            start_up_gui: StartupGui::default(),
            player_assignment_gui: None,
            net_architecture: architecture,
            camera,
            pending_player_name: None,
        }
    }

    /// This gets called as soon as the request to join or create a room is sent.
    /// During the lifetime of the program this function may be called multiple times.
    pub fn reset(&mut self, pending_name: String) {
        self.view_state.reset();
        self.presentation_state = PresentationState::None;
        self.player_assignment_gui = None;
        self.pending_player_name = Some(pending_name);
    }

    /// Takes care of the login screen, where player input their data.
    pub fn handle_login_screen(&mut self, error_string: &Option<String>) {
        let start_up = self.start_up_gui.handle_start_up(error_string);

        match start_up {
            StartupResult::Pending => {} // Nothing to do here.
            StartupResult::JoinRoom {
                room_name,
                player_name,
            } => {
                self.net_architecture.start_game_client(room_name);
                self.reset(player_name);
            }
            StartupResult::CreateRoom {
                room_name,
                player_name,
            } => {
                self.net_architecture.start_game_server(room_name, 0);
                self.reset(player_name);
            }
        }
    }

    /// Does the setup phase which is the logon process and the assignment of players. In the logon phase,
    /// we are waiting for other players to join.
    ///
    /// # Panic
    /// May happen, if we receive a move as an incremental update. This should not happen.
    pub fn handle_setup_phase(&mut self, is_server: bool, player_id: u16) {
        // First analyze the incoming messages.
        while let Some(result) = self.net_architecture.get_next_update() {
            match result {
                Full(state) => {
                    self.view_state = state;
                    return;
                }
                Incremental(
                    delta @ (DeltaInformation::SetPlayerNames(_)
                    | DeltaInformation::SetPlayerColors(_)),
                ) => {
                    self.view_state.apply_delta(&delta);
                }
                Incremental(delta @ DeltaInformation::SetGameState(_)) => {
                    self.view_state.apply_delta(&delta);
                    if matches!(delta, DeltaInformation::SetGameState(GameState::Move(_))) {
                        self.presentation_state = PresentationState::WaitingForInput(
                            BufferedBoardForRendering::new(&self.view_state, player_id),
                        );
                    }
                    break;
                }
                Incremental(DeltaInformation::MakeMove(_)) => {
                    panic!("We should not get a make move update in the setup phase.")
                }
            }
        }

        if self.view_state.game_state == GameState::AwaitingPlayers {
            self.media
                .print_text("Awaiting players...", TEXT_POINT_STATUS_INFO);
        } else if self.view_state.game_state == GameState::AssigningPlayers {
            if is_server {
                if self.player_assignment_gui.is_none() {
                    let player_names = self.view_state.player_names.clone();
                    self.player_assignment_gui = Some(PlayerAssignmentGui::new(player_names));
                }
                let assign_result = self
                    .player_assignment_gui
                    .as_mut()
                    .unwrap()
                    .handle_assignment();
                match assign_result {
                    AssignmentResult::Pending => {} // Nothing to do here.
                    AssignmentResult::ColorSetting(color) => self
                        .net_architecture
                        .register_server_rpc(RpcPayload::SetPlayerColors(color)),
                }
            } else {
                self.media
                    .print_text("Awaiting assignment...", TEXT_POINT_STATUS_INFO);
            }
        }
    }

    /// Handles the static view state, where we are not animating and when we are the correct player also process the
    /// input commands.
    pub fn handle_static_view_state(&mut self, player_id: u16) {
        // This may happen, if we are not synced yet.
        let PresentationState::WaitingForInput(ref buffer) = self.presentation_state else {
            self.media.print_text("Syncing...", TEXT_POINT_STATUS_INFO);
            return;
        };

        match self.view_state.game_state {
            GameState::AssigningPlayers | GameState::AwaitingPlayers => {
                // This happens when the game has ended and we want to get to the assignment phase.
            }
            GameState::GameOver => {
                buffer.render(&self.media);
            }

            GameState::Move(color) => {
                buffer.render(&self.media);

                // It is not our turn.
                if color != self.view_state.player_colors[player_id as usize] {
                    return;
                }

                let turn = Self::process_mouse_input(buffer.possible_moves(), &self.camera);
                if let Some(action) = turn {
                    self.net_architecture
                        .register_server_rpc(RpcPayload::MakeMove(StonePlacement {
                            field_position: action,
                            stone_color: color,
                        }));
                }
            }
        }
    }

    /// Checks, if we need to perform a transition animation, if so we execute it and
    /// return true - otherwise we return false.
    pub fn performing_animation(&mut self, delta_time: f32) -> bool {
        let mut finished_animation = false;
        let mut performed_animation = false;
        // For the case, that we are animating, we simply do so.
        if let PresentationState::Animating(ref mut animation) = self.presentation_state {
            animation.render(&self.media);
            finished_animation = animation.update(delta_time, &mut self.view_state);
            performed_animation = true;
        }

        if finished_animation {
            self.presentation_state = PresentationState::None;
        }

        performed_animation
    }

    /// Reads though all the incoming messages and returns a true, if we should display some animation transition.
    /// For the case we enter the information of an animation transition, we stop reading data from the message pump
    /// and return.
    pub fn process_message_pump_and_return_if_animated(&mut self, player_id: u16) -> bool {
        let mut update_presentation_state = false;
        while let Some(result) = self.net_architecture.get_next_update() {
            match result {
                Full(state) => {
                    self.view_state = state;
                    update_presentation_state = true;
                }
                Incremental(command @ DeltaInformation::SetGameState(_)) => {
                    self.view_state.apply_delta(&command); // We switch to the new state.
                    update_presentation_state = true;

                    if matches!(command, DeltaInformation::SetGameState(GameState::GameOver)) {
                        self.media.play_game_over_sound();
                    }
                }
                Incremental(command @ DeltaInformation::MakeMove(_)) => {
                    // Here we have to store the move and prepare the animation.
                    self.presentation_state = PresentationState::Animating(TransitionBoard::new(
                        command,
                        &self.view_state.game_board,
                    ));

                    self.media.play_stone_placement_sound();
                    return true;
                }
                // Theoretically this can happen after a full reset.
                Incremental(
                    command @ (DeltaInformation::SetPlayerNames(_)
                    | DeltaInformation::SetPlayerColors(_)),
                ) => {
                    debug_assert!(
                        update_presentation_state,
                        "A setting of player colors should happen after a a full reset."
                    );
                    self.view_state.apply_delta(&command);
                }
            }
        }

        if update_presentation_state
            && matches!(
                self.view_state.game_state,
                GameState::GameOver | GameState::Move(_)
            )
        {
            self.presentation_state = PresentationState::WaitingForInput(
                BufferedBoardForRendering::new(&self.view_state, player_id),
            )
        }

        false
    }

    /// Checks if the mouse got pressed and if we got an eligible field.
    fn process_mouse_input(
        list_of_eligible_positions: &[FieldPosition],
        camera: &Camera2D,
    ) -> Option<FieldPosition> {
        if !is_mouse_button_pressed(MouseButton::Left) {
            return None;
        }
        let click_pos = camera.screen_to_world(Vec2::from(mouse_position()));
        let selected_pos = FieldPosition {
            x_coord: (click_pos.x / CELL_SIZE) as i8,
            y_coord: (click_pos.y / CELL_SIZE) as i8,
        };

        if !list_of_eligible_positions.contains(&selected_pos) {
            return None;
        }
        Some(selected_pos)
    }
}
