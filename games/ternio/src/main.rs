//! This is the core module of Ternio.

#![windows_subsystem = "windows"]

mod board_logic;
mod global_game;
mod network_logic;
mod render_system;

use crate::global_game::{GlobalData, TEXT_POINT_STATUS_INFO, TernioSystem};
use crate::network_logic::basic_commands::{GameState, RpcPayload};
use crate::render_system::gui::gui_setup;
use backbone_lib::transport_layer::{ConnectionState, TransportLayer};
use board_logic::board_and_transition::PresentationState;
use macroquad::prelude::{
    BLACK, Camera2D, Conf, Rect, clear_background, get_frame_time, next_frame, set_camera,
};

/// Width of the window in stand-alone mode.
const WINDOW_WIDTH: i32 = 900;
/// Height of the window in stand-alone mode.
const WINDOW_HEIGHT: i32 = 1100;

/// Sets the windows name and the required size.
fn window_conf() -> Conf {
    Conf {
        window_title: "Ternio".to_owned(),
        window_width: WINDOW_WIDTH,
        window_height: WINDOW_HEIGHT,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    //! Initializes the system and runs the core loop.
    //!
    //! # Panic
    //! Can panic if we are after starting an animation not in animation state.
    // Origin is in the lower left corner
    let camera = Camera2D::from_display_rect(Rect::new(
        0.0,
        0.0,
        WINDOW_WIDTH as f32,
        WINDOW_HEIGHT as f32,
    ));
    set_camera(&camera);

    let net_architecture: TernioSystem = TransportLayer::generate_transport_layer(
        "ws://127.0.0.1:8080/ws".to_string(),
        // "wss://board-game-hub.de/api/ws".to_string(),
        "Ternio".to_string(),
    );

    let mut global_data = GlobalData::new(net_architecture, camera).await;

    gui_setup();
    loop {
        let delta_time = get_frame_time();
        global_data.net_architecture.update(delta_time);

        clear_background(BLACK);

        let state = global_data.net_architecture.connection_state().clone();
        match state {
            ConnectionState::Disconnected { error_string } => {
                global_data.handle_login_screen(&error_string);
            }
            ConnectionState::AwaitingHandshake | ConnectionState::ExecutingHandshake => {
                global_data
                    .media
                    .print_text("Connecting...", TEXT_POINT_STATUS_INFO);
            }
            ConnectionState::Connected {
                is_server,
                player_id,
                rule_set: _,
            } => {
                if let Some(name) = global_data.pending_player_name.take() {
                    global_data
                        .net_architecture
                        .register_server_rpc(RpcPayload::SetPlayerName(name));
                }

                if matches!(
                    global_data.view_state.game_state,
                    GameState::AssigningPlayers | GameState::AwaitingPlayers
                ) {
                    global_data.handle_setup_phase(is_server, player_id);
                } else {
                    let performed_animation = global_data.performing_animation(delta_time);

                    if !performed_animation {
                        let started_animation =
                            global_data.process_message_pump_and_return_if_animated(player_id);
                        if !started_animation {
                            global_data.handle_static_view_state(player_id);
                        } else {
                            let PresentationState::Animating(ref mut animation) =
                                global_data.presentation_state
                            else {
                                panic!("Unexpected state.")
                            };
                            animation.render(&global_data.media);
                        }
                    }
                }
            }
        }
        next_frame().await;
    }
}
