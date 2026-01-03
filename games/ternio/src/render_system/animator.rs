//! The task of the animator is to show the materialization of stones and the flipping animation of stones to be flipped.

use crate::board_logic::board_representation::{FlipInformation, StonePlacement};
use crate::render_system::media::{CELL_SIZE, STONE_RADIUS, draw_game_board, get_stone_color};
use macroquad::shapes::{draw_circle, draw_ellipse};
use std::f32::consts::PI;

/// The time we reserve for scaling the newly placed stone.
const TIME_FOR_SCALING: f32 = 0.25;

/// The time we reserve for flipping the stones being enclosed.
const TIME_FOR_FLIPPING: f32 = 0.75;

/// The animator  is responsible for inserting a new stone and flipping the existing ones.
pub struct Animator {
    /// These are all stones, that are currently on the board. Not including the stone placed.
    all_stones: Vec<StonePlacement>,
    /// These are the stones, that are on the board, that do not undergo flipping animation.
    static_stones: Vec<StonePlacement>,
    /// The stones that undergo a flipping animation.
    flipping_stones: Vec<FlipInformation>,
    /// The stone that gets newly placed
    materializing_place: StonePlacement,
    /// The time passed in the animation.
    time_passed: f32,
}

/// A smoothstep function in the range of 0..1 with vanishing derivatives at the extrema.
fn smoothstep_normalized(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Animator {
    /// Creates the animator.
    ///
    /// # Arguments
    /// * `all_stones`: The stones that are currently on the board.
    /// * `static_stones`: The stones that are on the board and that do not undergo flipping animation.
    /// * `flipping_stones`: The stones that undergo flipping animation.
    /// * `materializing_place`: The position where the new stone gets placed.
    pub fn new(
        all_stones: Vec<StonePlacement>,
        static_stones: Vec<StonePlacement>,
        flipping_stones: Vec<FlipInformation>,
        materializing_place: StonePlacement,
    ) -> Animator {
        Animator {
            all_stones,
            static_stones,
            flipping_stones,
            materializing_place,
            time_passed: 0.0,
        }
    }

    /// Does an update and returns if the animation is over.
    pub fn update(&mut self, delta_time: f32) -> bool {
        self.time_passed += delta_time;
        self.time_passed > (TIME_FOR_SCALING + TIME_FOR_FLIPPING)
    }

    /// Draws the materializing stone with the indicated radius.
    fn draw_marked_stone_with_radius(&self, radius: f32) {
        draw_circle(
            (self.materializing_place.field_position.x_coord as f32 + 0.5) * CELL_SIZE,
            (self.materializing_place.field_position.y_coord as f32 + 0.5) * CELL_SIZE,
            radius,
            get_stone_color(self.materializing_place.stone_color),
        );
    }

    /// If we animate we render the complete board. The animation is split into two phases.
    /// In pase a the newly placed stone materializes at its position and in phase 2 the
    /// flipping stones are animated into their new position.
    pub fn render(&self) {
        // See if we are in materializing phase.
        if self.time_passed < TIME_FOR_SCALING {
            let size = smoothstep_normalized(self.time_passed / TIME_FOR_SCALING) * STONE_RADIUS;
            draw_game_board(&self.all_stones);
            self.draw_marked_stone_with_radius(size);
        } else {
            draw_game_board(&self.static_stones);
            // Draw the newly set stone.
            self.draw_marked_stone_with_radius(STONE_RADIUS);

            let flipping_phase = (self.time_passed - TIME_FOR_SCALING) / TIME_FOR_FLIPPING;
            let first_half = flipping_phase < 0.5;

            let x_scaling = STONE_RADIUS * (flipping_phase * PI).cos().abs();

            // Draw the animated flipping stones.
            for flip in self.flipping_stones.iter() {
                draw_ellipse(
                    (flip.field_position.x_coord as f32 + 0.5) * CELL_SIZE,
                    (flip.field_position.y_coord as f32 + 0.5) * CELL_SIZE,
                    x_scaling,
                    STONE_RADIUS,
                    0.0,
                    get_stone_color(if first_half {
                        flip.source_color
                    } else {
                        flip.destination_color
                    }),
                );
            }
        }
    }
}
