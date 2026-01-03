//! Contains helper functionality for GUI implemented with egui. This is the logon gui for the game and
//! the player color assignment gui.

use crate::board_logic::board_representation::{NUM_OF_COLORS, StoneColor};
use egui_macroquad::egui;

// === Mobile Input Modul ===
#[cfg(target_arch = "wasm32")]
pub mod mobile_input {
    use sapp_jsutils::JsObject;

    unsafe extern "C" {
        fn focus_mobile_input(current_value: JsObject);
        fn get_mobile_input_value() -> JsObject;
        fn blur_mobile_input();
        fn is_touch_device() -> bool;
    }

    pub fn is_mobile() -> bool {
        unsafe { is_touch_device() }
    }

    pub fn focus_input(current: &str) {
        unsafe {
            let js_str = JsObject::string(current);
            focus_mobile_input(js_str);
        }
    }

    pub fn get_value() -> String {
        unsafe {
            let js_obj = get_mobile_input_value();
            let mut result = String::new();
            js_obj.to_string(&mut result);
            result
        }
    }

    pub fn blur_input() {
        unsafe {
            blur_mobile_input();
        }
    }
}

/// This is a helper macro to combine a single line text editing field with a
/// hidden text HTML element to make the keyboard appear on mobile. In native mode this
/// gets ignored.
///
/// In order for this to work the HTML file of the WASM plugin has to contain an entry of the form
/// ```html
/// <input type="text" id="mobile-keyboard-input"
///    style="position: absolute; left: 0; opacity: 0; pointer-events: none;"
///    autocomplete="off" />
/// ```
#[macro_export]
macro_rules! focus_text_line {
    ($ui:ident, $var_name:expr) => {
        let _response = $ui.text_edit_singleline(&mut $var_name);

        #[cfg(target_arch = "wasm32")]
        if mobile_input::is_mobile() {
            if _response.gained_focus() {
                mobile_input::focus_input(&$var_name);
            }
            if _response.has_focus() {
                $var_name = mobile_input::get_value();
            }
            if _response.lost_focus() {
                mobile_input::blur_input();
            }
        }
    };
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
/// The purpose of this module is to force the activation of the keyboard on mobile by querying a hidden text field.
/// Consequently, it distinguishes between mobile input and mouse input.
mod mobile_input {
    /// Sets on mobile the focus on the hidden text field.
    pub fn focus_input(_: &str) {}
    /// Asks the value of the hidden text field on mobile.
    pub fn get_value() -> String {
        String::new()
    }
    /// Makes the hidden text field loose focus on mobile.
    pub fn blur_input() {}
}

// ================== GUI Code ===================

/// Defines the global style for the GUI, mostly sets font sizes.
pub fn gui_setup() {
    egui_macroquad::ui(|egui_ctx| {
        let mut style = (*egui_ctx.style()).clone();

        style.text_styles = [
            (egui::TextStyle::Body, egui::FontId::proportional(18.0)),
            (egui::TextStyle::Button, egui::FontId::proportional(18.0)),
            (egui::TextStyle::Heading, egui::FontId::proportional(24.0)),
            (egui::TextStyle::Monospace, egui::FontId::monospace(16.0)),
            (egui::TextStyle::Small, egui::FontId::proportional(14.0)),
        ]
        .into();

        style.visuals.override_text_color = Some(egui::Color32::WHITE);
        egui_ctx.set_style(style);
    });
    egui_macroquad::draw();
}

#[derive(Default)]
/// The internal state of the gui contains a room name and a player name.
pub struct StartupGui {
    room_name: String,
    player_name: String,
}

/// The current state of the startup gui.
pub enum StartupResult {
    /// There is no result yet.
    Pending,
    /// We want to create a room with the indicated player name and room name.
    CreateRoom {
        room_name: String,
        player_name: String,
    },
    /// We want to join a room with the indicated player and room name.
    JoinRoom {
        room_name: String,
        player_name: String,
    },
}
impl StartupGui {
    /// This is the egui implementation to show and handle the gui. An error string that should be
    /// displayed is handed over if necessary.
    pub fn handle_start_up(&mut self, error: &Option<String>) -> StartupResult {
        let mut result = StartupResult::Pending;

        egui_macroquad::ui(|egui_ctx| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.vertical(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("Log on to Ternio.");
                    });
                    ui.label("Set a nickname and create or join a room. ");
                    ui.add_space(40.0);
                    ui.label("Set your nickname.");
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.add_space(20.0);
                        focus_text_line!(ui, self.player_name);
                    });
                    ui.add_space(40.0);
                    ui.label("Create or join a room.");
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        ui.label("Room:");
                        ui.add_space(20.0);
                        focus_text_line!(ui, self.room_name);
                    });
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        if ui.button("Create Room").clicked() && !self.room_name.is_empty() {
                            result = StartupResult::CreateRoom {
                                room_name: self.room_name.clone(),
                                player_name: self.player_name.clone(),
                            };
                        }
                        ui.add_space(100.0);
                        if ui.button("Join Room").clicked() && !self.room_name.is_empty() {
                            result = StartupResult::JoinRoom {
                                room_name: self.room_name.clone(),
                                player_name: self.player_name.clone(),
                            };
                        }
                    });

                    ui.add_space(50.0);
                    if let Some(error_str) = error.clone() {
                        ui.label(egui::RichText::new(error_str).color(egui::Color32::RED));
                    }
                });
            });
        });
        egui_macroquad::draw();
        result
    }
}

// -----------------------------------
// The GUI for player color assignment
// -----------------------------------

/// The player assignment gui is only shown on host side to be able to assign the players to different colors.
pub struct PlayerAssignmentGui {
    player_name: [String; NUM_OF_COLORS],
    player_color: [StoneColor; NUM_OF_COLORS],
}

/// The result is pending, or the assignments of color to the different player.
pub enum AssignmentResult {
    Pending,
    ColorSetting([StoneColor; NUM_OF_COLORS]),
}

impl PlayerAssignmentGui {
    /// Creates a new gui from the player names.
    pub fn new(player_name: [String; NUM_OF_COLORS]) -> Self {
        use StoneColor::*;
        PlayerAssignmentGui {
            player_name,
            player_color: [Red, Green, Blue],
        }
    }

    /// Shows the assignment GUI with the radio buttons for all three players.
    pub fn handle_assignment(&mut self) -> AssignmentResult {
        let mut result = AssignmentResult::Pending;

        egui_macroquad::ui(|egui_ctx| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.vertical(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("Assign Players");
                    });
                    ui.add_space(20.0);
                    use StoneColor::*;
                    for player in 0..NUM_OF_COLORS {
                        ui.label(format!("{}:", self.player_name[player]));
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.player_color[player], Red, "red");
                            ui.radio_value(&mut self.player_color[player], Green, "green");
                            ui.radio_value(&mut self.player_color[player], Blue, "blue");
                        });
                        ui.add_space(20.0);
                    }

                    if (self.player_color[0] != self.player_color[2])
                        && (self.player_color[1] != self.player_color[0])
                        && (self.player_color[1] != self.player_color[2])
                        && ui.button("Assign").clicked()
                    {
                        result = AssignmentResult::ColorSetting(self.player_color);
                    }
                });
            });
        });
        egui_macroquad::draw();
        result
    }
}
