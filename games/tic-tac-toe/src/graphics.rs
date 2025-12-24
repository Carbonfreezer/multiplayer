//! All relevant drawing functions for tic tact toe are accumulated here.

use macroquad::prelude::{
    BLACK, Camera2D, Font, GRAY, TextParams, Vec2, WHITE, draw_circle, draw_line, draw_text_ex,
    load_ttf_font_from_bytes, measure_text,
};

/// The font we draw with gets embedded as binary.
const HELVETICA: &[u8] = include_bytes!("../Helvetica.ttf");

/// The size of the cross and the circle in the game.
const ICON_SIZE: f32 = 35.0;

/// The graphics module can not live longer than the camera, that gets borrowed.
pub struct Graphics<'a> {
    font: Font,
    camera: &'a Camera2D,
}

impl<'a> Graphics<'a> {
    pub fn new(camera: &'a Camera2D) -> Self {
        Graphics {
            camera,
            font: load_ttf_font_from_bytes(HELVETICA).unwrap(),
        }
    }

    /// Transfer function to process mouse positions.
    pub fn get_adjusted_position(&self, pos: (f32, f32)) -> Vec2 {
        self.camera.screen_to_world(Vec2::new(pos.0, pos.1))
    }

    /// Draws a text at the indicated position.
    pub fn print_text(&self, text: &str, position: Vec2, font_size: u16) {
        draw_text_ex(
            text,
            position.x,
            position.y,
            TextParams {
                font: Some(&self.font),
                font_size,
                font_scale: -1.0,
                font_scale_aspect: -1.0,
                rotation: 0.0,
                color: WHITE,
            },
        );
    }

    /// Draws the base lines of the tic tac toe board.
    pub fn draw_base_board(&self) {
        for i in 0..=3 {
            draw_line(
                50.0,
                100.0 * i as f32 + 20.0,
                350.0,
                100.0 * i as f32 + 20.0,
                3.0,
                GRAY,
            );
            draw_line(
                100.0 * i as f32 + 50.0,
                20.0,
                100.0 * i as f32 + 50.0,
                320.0,
                3.0,
                GRAY,
            );
        }
    }

    /// The cross symbol for tic tac toe.
    pub fn draw_cross(&self, x: f32, y: f32) {
        let x_center = x * 100.0 + 100.0;
        let y_center = y * 100.0 + 50.0 + 20.0;
        draw_line(
            x_center - ICON_SIZE,
            y_center - ICON_SIZE,
            x_center + ICON_SIZE,
            y_center + ICON_SIZE,
            2.0,
            WHITE,
        );
        draw_line(
            x_center - ICON_SIZE,
            y_center + ICON_SIZE,
            x_center + ICON_SIZE,
            y_center - ICON_SIZE,
            2.0,
            WHITE,
        );
    }

    /// The circle symbol for tic tac toe.
    pub fn draw_circle(&self, x: f32, y: f32) {
        let x_center = x * 100.0 + 100.0;
        let y_center = y * 100.0 + 50.0 + 20.0;
        draw_circle(x_center, y_center, ICON_SIZE, WHITE);
        draw_circle(x_center, y_center, ICON_SIZE - 2.0, BLACK);
    }

    /// Same as print text, only in this case the center point is handed over.
    pub fn print_text_centered(&self, text: &str, position: Vec2, font_size: u16) {
        let size = measure_text(text, Some(&self.font), font_size, 1.0);
        self.print_text(
            text,
            position
                - Vec2 {
                    x: size.width / 2.0,
                    y: size.height / 2.0,
                },
            font_size,
        );
    }
}
