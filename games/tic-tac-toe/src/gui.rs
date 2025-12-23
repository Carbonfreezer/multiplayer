//! Contains helper functionality for GUI used for setup.
//! The first part is essentially about evoking the keyboard for the single text line edit
//! on mobile devices in WASM mode.

use egui_macroquad::egui;

/// The purpose of this module is to force the activation of the keyboard on mobile by querying a hidden text field.
/// Consequently, it distinguishes between mobile input and mouse input.
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
/// hidden text HTML element to make the keyboard appear on mobile.
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
mod mobile_input {
    pub fn focus_input(_: &str) {}
    pub fn get_value() -> String {
        String::new()
    }
    pub fn blur_input() {}
}

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
}

/// The internal state of the gui contains the room name and the info, if spectators are allowed.
#[derive(Default)]
pub struct StartupGui {
    room_name: String,
    allow_spectators: bool,
}

/// The result that returns of the start-up process.
pub enum StartupResult {
    /// The player has not decided yet.
    Pending,
    /// The player wants to create a new room.
    CreateRoom {
        room: String,
        allow_spectators: bool,
    },
    /// The player want to join a room.
    JoinRoom { room: String },
}

impl StartupGui {
    /// Run handler for the immediate mode egui. The error is an optional string that can be set,
    /// if we return to this screen from a network error.
    pub fn handle_start_up(&mut self, error: &Option<String>) -> StartupResult {
        let mut result = StartupResult::Pending;

        egui_macroquad::ui(|egui_ctx| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.vertical(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("Log on to Tic-Tac-Toe");
                    });
                    ui.label("Enter or create a room. The room creator starts and selects if spectators are allowed.");
                    ui.add_space(30.0);

                    ui.horizontal(|ui| {
                        ui.label("Room:");
                        ui.add_space(10.0);

                        focus_text_line!(ui, self.room_name);

                    });
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        ui.label("Allow spectators: ");
                        ui.add_space(10.0);
                        ui.checkbox(&mut self.allow_spectators, "allow");
                    });
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        ui.add_space(75.0);
                        if ui.button("Create Room").clicked() && !self.room_name.is_empty() {
                            result = StartupResult::CreateRoom {
                                room: self.room_name.clone(),
                                allow_spectators: self.allow_spectators,
                            };
                        }
                        ui.add_space(20.0);
                        if ui.button("Join Room").clicked() && !self.room_name.is_empty() {
                            result = StartupResult::JoinRoom {
                                room: self.room_name.clone(),
                            };
                        }
                    });

                    ui.add_space(50.0);
                    if let Some(error_str) = error {
                        ui.label(egui::RichText::new(error_str).color(egui::Color32::RED));
                    }
                });
            });
        });
        egui_macroquad::draw();
        result
    }
}
