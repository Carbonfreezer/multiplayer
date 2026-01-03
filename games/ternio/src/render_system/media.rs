//! Has a collection of different functions to draw the board and play sound.

use crate::board_logic::board_representation::{
    BOARD_DIM, FieldPosition, NUM_OF_COLORS, StoneColor, StonePlacement,
};
use macroquad::audio::{Sound, load_sound_from_bytes, play_sound_once};
use macroquad::prelude::*;

/// Embedding of helvetica.ttf.
const HELVETICA: &[u8] = include_bytes!("../../Helvetica.ttf");

/// Embedding of stone placement sound.
const PLACEMENT: &[u8] = include_bytes!("../../Drop.wav");

/// Embedding of final gong sound.
const GONG: &[u8] = include_bytes!("../../Gong.ogg");

/// The media contains the loaded data from the embedded binary.
pub struct Media {
    /// The font we draw text with.
    font: Option<Font>,
    /// The sound for placing a stone.
    placement_sound: Sound,
    /// The sound for playing the game over gong.
    game_over_sound: Sound,
}

impl Media {
    /// Loads all the embedded data.
    pub async fn new() -> Self {
        Media {
            font: load_ttf_font_from_bytes(HELVETICA).ok(),
            placement_sound: load_sound_from_bytes(PLACEMENT).await.unwrap(),
            game_over_sound: load_sound_from_bytes(GONG).await.unwrap(),
        }
    }

    /// Prints the text at the indicated position, which is the lower left point.
    pub fn print_text(&self, text: &str, position: Vec2) {
        draw_text_ex(
            text,
            position.x,
            position.y,
            TextParams {
                font: self.font.as_ref(),
                font_size: 40,
                font_scale: -1.0,
                font_scale_aspect: -1.0,
                rotation: 0.0,
                color: WHITE,
            },
        );
    }

    /// Prints the text centered. The position handed over will be the center position of the text.
    pub fn print_text_centered(&self, text: &str, position: Vec2) {
        let size = measure_text(text, self.font.as_ref(), 40, 1.0);
        draw_text_ex(
            text,
            position.x - size.width / 2.0,
            position.y - size.height / 2.0,
            TextParams {
                font: self.font.as_ref(),
                font_size: 40,
                font_scale: -1.0,
                font_scale_aspect: -1.0,
                rotation: 0.0,
                color: WHITE,
            },
        );
    }

    /// Draws the current score on the top line.
    pub fn draw_score(&self, red_green_blue: [i8; NUM_OF_COLORS]) {
        self.print_text_centered(
            format!(
                "Red: {:2}        Green: {:2}        Blue:{:2}",
                red_green_blue[0], red_green_blue[1], red_green_blue[2]
            )
            .as_str(),
            Vec2::new(450.0, 935.0),
        );
    }

    /// Paints a header line.
    pub fn draw_header(&self, text: &str) {
        self.print_text_centered(text, Vec2::new(450.0, 1035.0));
    }

    /// Plays the placement sound of the stone.
    pub fn play_stone_placement_sound(&self) {
        play_sound_once(&self.placement_sound);
    }

    /// Plays the game over sound of the game.
    pub fn play_game_over_sound(&self) {
        play_sound_once(&self.game_over_sound);
    }
}

/// Gets the macroquad color for a certain internal stone color.
pub fn get_stone_color(stone: StoneColor) -> Color {
    match stone {
        StoneColor::Green => GREEN,
        StoneColor::Blue => BLUE,
        StoneColor::Red => RED,
    }
}

/// The size we have reserved for a cell.
pub const CELL_SIZE: f32 = 100.0;

/// The radius with which we want to draw a stone.
pub const STONE_RADIUS: f32 = 40.0;

/// The background color of the playing field.
const BACKGROUND_COLOR: Color = DARKGRAY;

/// The color of the lines to mark the playing field.
const LINE_COLOR: Color = GRAY;

/// Draws the game board in its base state. Gets a vector of the static stone positions to draw.
pub fn draw_game_board(pattern: &Vec<StonePlacement>) {
    // Draw the base game board.
    draw_rectangle(0.0, 0.0, 900.0, 900.0, BACKGROUND_COLOR);
    for line_ind in 0..=BOARD_DIM {
        draw_line(
            CELL_SIZE * line_ind as f32,
            0.0,
            CELL_SIZE * line_ind as f32,
            BOARD_DIM as f32 * CELL_SIZE,
            3.0,
            LINE_COLOR,
        );
        draw_line(
            0.0,
            CELL_SIZE * line_ind as f32,
            BOARD_DIM as f32 * CELL_SIZE,
            CELL_SIZE * line_ind as f32,
            3.0,
            LINE_COLOR,
        );
    }

    // Draw the stones.
    for stone in pattern {
        draw_circle(
            (stone.field_position.x_coord as f32 + 0.5) * CELL_SIZE,
            (stone.field_position.y_coord as f32 + 0.5) * CELL_SIZE,
            STONE_RADIUS,
            get_stone_color(stone.stone_color),
        );
    }
}

/// Draws the movement options onto the game board with crosses. The color used for drawing the crosses
/// has to be handed over.
pub fn draw_movement_options(crosses: &Vec<FieldPosition>, stone: StoneColor) {
    let draw_color = get_stone_color(stone);
    for free_spot in crosses {
        let center = Vec2::new(
            CELL_SIZE * (free_spot.x_coord as f32 + 0.5),
            CELL_SIZE * (free_spot.y_coord as f32 + 0.5),
        );
        let start = center - Vec2::new(STONE_RADIUS, STONE_RADIUS);
        let end = center + Vec2::new(STONE_RADIUS, STONE_RADIUS);
        draw_line(start.x, start.y, end.x, end.y, 2.0, draw_color);
        let start = center + Vec2::new(STONE_RADIUS, -STONE_RADIUS);
        let end = center + Vec2::new(-STONE_RADIUS, STONE_RADIUS);
        draw_line(start.x, start.y, end.x, end.y, 2.0, draw_color);
    }
}
