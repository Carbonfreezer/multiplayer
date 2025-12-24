#![windows_subsystem = "windows"]

/// Desired window width.
pub const ALL_WIDTH: u32 = 400;
/// Desired window height.
pub const ALL_HEIGHT: u32 = 400;

mod graphics;
mod gui;
mod tic_tac_toe_logic;

use crate::graphics::Graphics;
use crate::gui::{StartupGui, StartupResult, gui_setup};
use crate::tic_tac_toe_logic::backend::TicTacToeLogic;
use crate::tic_tac_toe_logic::traits_implementation::{ViewState, MoveCommand};
use backbone_lib::middle_layer::{ConnectionState, MiddleLayer, ViewStateUpdate};
use macroquad::prelude::{
    BLACK, Camera2D, Conf, MouseButton, Rect, Vec2, clear_background, get_frame_time,
    is_mouse_button_pressed, mouse_position, next_frame, set_camera,
};

/// Configures window title and size.
fn window_conf() -> Conf {
    Conf {
        window_title: "Tic Tac Toe".to_owned(),
        window_width: ALL_WIDTH as i32,
        window_height: ALL_HEIGHT as i32,
        ..Default::default()
    }
}


#[macroquad::main(window_conf)]
async fn main() {
    //! Does the system setup and then runs the core loop, where actions are decided upon the internal connection state.
     
    // Origin is in the lower left corner
    let camera =
        Camera2D::from_display_rect(Rect::new(0.0, 0.0, ALL_WIDTH as f32, ALL_HEIGHT as f32));
    set_camera(&camera);

    let graphics = Graphics::new(&camera);
    let mut net_architecture: MiddleLayer<MoveCommand, MoveCommand, TicTacToeLogic, ViewState> =
        MiddleLayer::generate_middle_layer(
            "ws://127.0.0.1:8080/ws".to_string(),
            "tic-tac-toe".to_string(),
        );

    let mut view_state: Option<ViewState> = None;

    let mut start_up_gui = StartupGui::default();
    gui_setup();
    loop {
        let delta_time = get_frame_time();
        net_architecture.update(delta_time);

        clear_background(BLACK);

        let state = net_architecture.connection_state().clone();
        match state {
            ConnectionState::Disconnected { error_string } => {
                let start_up = start_up_gui.handle_start_up(&error_string);

                match start_up {
                    StartupResult::Pending => {} // Nothing to do here.
                    StartupResult::JoinRoom { room } => net_architecture.start_game_client(room),
                    StartupResult::CreateRoom {
                        room,
                        allow_spectators,
                    } => net_architecture
                        .start_game_server(room, if allow_spectators { 1 } else { 0 }),
                }

                view_state = None;
            }
            ConnectionState::AwaitingHandshake | ConnectionState::ExecutingHandshake => {
                graphics.print_text("Connecting", Vec2 { x: 200.0, y: 350.0 }, 24)
            }
            ConnectionState::Connected {
                is_server: _,
                player_id,
                rule_set: _,
            } => {
                if view_state.is_none() {
                    view_state = Some(ViewState::new(true))
                }

                update_real_game(
                    &graphics,
                    &mut net_architecture,
                    player_id,
                    view_state.as_mut().unwrap(),
                );
            }
        }

        next_frame().await
    }
}

/// The core update for the game. First it drains the network commands, then it renders the board and
/// finally it sends any potential mouse clicks as stone setting commands to the server.
fn update_real_game(
    graphics: &Graphics,
    middle_layer: &mut MiddleLayer<MoveCommand, MoveCommand, TicTacToeLogic, ViewState>,
    local_player: u16,
    view_state: &mut ViewState,
) {
    // We do not have any animations here, so we simply drain the commands.
    while let Some(update) = middle_layer.get_next_update() {
        match update {
            ViewStateUpdate::Full(state) => {
                *view_state = state;
            }
            ViewStateUpdate::Incremental(delta) => {
                view_state.apply_move(&delta);
            }
        }
    }

    let my_turn = ((local_player == 0) && view_state.next_move_host)
        || ((local_player == 1) && (!view_state.next_move_host));

    let text = match view_state.check_winning() {
        1 => "Cross wins",
        2 => "Circle wins",
        3 => "Draw",
        _ => {
            if local_player > 1 {
                "Spectator"
            } else if my_turn {
                "Your turn"
            } else {
                "Waiting"
            }
        }
    };

    graphics.print_text_centered(text, Vec2 { x: 200.0, y: 350.0 }, 24);
    // Now we draw the board.
    graphics.draw_base_board();
    for x in 0..3 {
        for y in 0..3 {
            match view_state.board[y][x] {
                1 => graphics.draw_cross(x as f32, y as f32),
                2 => graphics.draw_circle(x as f32, y as f32),
                _ => {}
            }
        }
    }

    // When it is not our move, we are done here.
    if !my_turn {
        return;
    }

    if is_mouse_button_pressed(MouseButton::Left) {
        let corrected_mouse = graphics.get_adjusted_position(mouse_position());
        let x_pos = ((corrected_mouse.x - 50.0) / 100.0) as i32;
        let y_pos = ((corrected_mouse.y - 20.0) / 100.0) as i32;

        if (x_pos >= 0) && (y_pos >= 0) && (x_pos < 3) && (y_pos < 3) {
            let command = MoveCommand {
                is_host: view_state.next_move_host,
                column: x_pos as u8,
                row: y_pos as u8,
            };
            middle_layer.register_server_rpc(command);
        }
    }
}
