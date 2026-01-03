//! Contains the basic board representation with the option to find out, what the move options are.
//! Also has a couple of helper structures.

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::ops::AddAssign;

/// The number of colors and players we have in the game.
pub const NUM_OF_COLORS: usize = 3;
/// The extension of the board we have in every dimension.
pub const BOARD_DIM: usize = 9;
/// The same as [`BOARD_DIM`] just in i8 as often needed.
pub const BOARD_DIMS: i8 = BOARD_DIM as i8;

/// Encodes a position on the game field. Origin is in the lower left point.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct FieldPosition {
    /// Horizontal position from left to right
    pub x_coord: i8,
    /// Vertical position from bottom to top.
    pub y_coord: i8,
}

impl FieldPosition {
    /// Checks if we are a valid position, which means we are on the board in the required range.
    pub fn is_valid(&self) -> bool {
        !(self.x_coord < 0
            || self.y_coord < 0
            || self.x_coord >= BOARD_DIMS
            || self.y_coord >= BOARD_DIMS)
    }
}

/// Implements the adding of scan direction for the field position.
impl AddAssign<&ScanDirection> for FieldPosition {
    fn add_assign(&mut self, rhs: &ScanDirection) {
        self.x_coord += rhs.x_dir;
        self.y_coord += rhs.y_dir;
    }
}

#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
/// The field content we have for the board.
pub enum FieldContent {
    /// There is no stone on the board.
    Empty,
    /// There is a stone on the board in the indicated color.
    Stone(StoneColor),
}

#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
/// The three real colors as they appear on the game field.
pub enum StoneColor {
    Red,
    Green,
    Blue,
}

impl StoneColor {
    /// Gives the cycle of stone colors beginning from self. This is an aid function
    /// to help determine what the next eligible player is.
    pub fn cycle_from_next(&self) -> [StoneColor; NUM_OF_COLORS] {
        use StoneColor::*;
        match self {
            Red => [Green, Blue, Red],
            Green => [Blue, Red, Green],
            Blue => [Red, Green, Blue],
        }
    }
}

/// Returns the information which stone at which position should be flipped from which to which color.
pub struct FlipInformation {
    /// The position of the flipping stone.
    pub field_position: FieldPosition,
    /// The color we come from.
    pub source_color: StoneColor,
    /// The color we go to.
    pub destination_color: StoneColor,
}

/// Contains the placement information for placing a stone.
#[derive(Serialize, Deserialize, Clone)]
pub struct StonePlacement {
    /// Where should the stone go to.
    pub field_position: FieldPosition,
    /// What is the color of the stone.
    pub stone_color: StoneColor,
}

/// The game board that is also a part of the view state.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct GameBoard {
    /// The contents of the field positions. Dimensions are [`BOARD_DIM`] × [`BOARD_DIM`].
    pub fields: [[FieldContent; BOARD_DIM]; BOARD_DIM],
}

/// Indicates a direction in which we want to walk from a local direction.
struct ScanDirection {
    x_dir: i8,
    y_dir: i8,
}

use ScanDirection as D;
/// Buffers the 8 possible scan directions N, NE, E , SE, S, SW, W, NW.
#[rustfmt::skip] 
static SCAN_DIRECTIONS: [ScanDirection; 8] = [
    D { x_dir: 0, y_dir: 1 },
    D { x_dir: 1, y_dir: 1 },
    D { x_dir: 1, y_dir: 0 },
    D { x_dir: 1, y_dir: -1 },
    D { x_dir: 0, y_dir: -1 },
    D { x_dir: -1, y_dir: -1 },
    D { x_dir: -1, y_dir: 0 },
    D { x_dir: -1, y_dir: 1 },
];

impl GameBoard {
    /// Creates a new game board with empty fields.
    pub fn new() -> Self {
        let fields = [[FieldContent::Empty; BOARD_DIM]; BOARD_DIM];
        GameBoard { fields }
    }

    /// Puts the board into a start configuration.
    pub fn reset_board(&mut self) {
        use FieldContent::*;
        use StoneColor::*;
        self.fields = [[Empty; BOARD_DIM]; BOARD_DIM];
        self.fields[3][4] = Stone(Red);
        self.fields[4][4] = Stone(Red);
        self.fields[5][4] = Stone(Red);

        self.fields[3][3] = Stone(Green);
        self.fields[5][3] = Stone(Green);
        self.fields[4][5] = Stone(Green);

        self.fields[3][5] = Stone(Blue);
        self.fields[5][5] = Stone(Blue);
        self.fields[4][3] = Stone(Blue);
    }

    /// Gets the stone color of an indicated field as an option.
    fn get_optional_stone_color(&self, x_pos: i8, y_pos: i8) -> Option<StoneColor> {
        match self.fields[x_pos as usize][y_pos as usize] {
            FieldContent::Stone(color) => Some(color),
            FieldContent::Empty => None,
        }
    }

    /// Asks for all the stone placements on the board.
    pub fn get_stone_placement(&self) -> Vec<StonePlacement> {
        (0..BOARD_DIMS)
            .cartesian_product(0..BOARD_DIMS)
            .map(|(x_coord, y_coord)| {
                (
                    self.get_optional_stone_color(x_coord, y_coord),
                    FieldPosition { x_coord, y_coord },
                )
            })
            .filter_map(|(color, field_position)| {
                color.map(|stone_color| StonePlacement {
                    field_position,
                    stone_color,
                })
            })
            .collect()
    }

    /// Gets the current stone amount  for red, green, blue.
    pub fn get_score(&self) -> [i8; NUM_OF_COLORS] {
        let mut result: [i8; NUM_OF_COLORS] = [0; NUM_OF_COLORS];

        for i in 0..BOARD_DIM {
            for j in 0..BOARD_DIM {
                if let FieldContent::Stone(color) = self.fields[i][j] {
                    result[color as usize] += 1;
                }
            }
        }

        result
    }

    /// Gets the stone color at the indicated position, assumes here, that the field is not empty.
    pub fn select_field(&self, field: &FieldPosition) -> StoneColor {
        self.get_optional_stone_color(field.x_coord, field.y_coord)
            .expect("Field should not be empty")
    }

    /// Checks if a field is empty
    pub fn is_empty(&self, field: &FieldPosition) -> bool {
        self.fields[field.x_coord as usize][field.y_coord as usize] == FieldContent::Empty
    }

    /// Checks of the indicated move for the indicated color would be legal.
    pub fn is_legal_move(&self, test_position: FieldPosition, test_stone: StoneColor) -> bool {
        if !test_position.is_valid() || !self.is_empty(&test_position) {
            return false;
        }

        SCAN_DIRECTIONS.iter().any(|dir| {
            self.get_potentially_flipped_stones(test_position.clone(), test_stone, dir) > 0
        })
    }

    /// Gets all legal moves for the indicated color.
    pub fn get_all_legal_moves(&self, test_stone: StoneColor) -> Vec<FieldPosition> {
        (0..BOARD_DIMS)
            .cartesian_product(0..BOARD_DIMS)
            .map(|(x_coord, y_coord)| FieldPosition { x_coord, y_coord })
            .filter(|pos| self.is_legal_move(pos.clone(), test_stone))
            .collect()
    }

    /// Gets the flipped stone positions that get applied, if we place a stone with the indicated color at the indicated position.
    /// We assume at this point that the move ss legal.
    pub fn get_all_flipped_stones(
        &self,
        test_position: FieldPosition,
        test_stone: StoneColor,
    ) -> Vec<FlipInformation> {
        debug_assert!(self.is_legal_move(test_position.clone(), test_stone));
        let mut result = vec![];
        for dir in SCAN_DIRECTIONS.iter() {
            let amount_of_flipped_stones =
                self.get_potentially_flipped_stones(test_position.clone(), test_stone, dir);
            let mut base_point = test_position.clone();
            for _ in 0..amount_of_flipped_stones {
                base_point += dir;
                let flip_info = FlipInformation {
                    field_position: base_point.clone(),
                    source_color: self.select_field(&base_point),
                    destination_color: test_stone,
                };
                result.push(flip_info);
            }
        }

        result
    }

    /// Places an indicated stone with the indicated color at a specific position, without any flipping operations.
    fn place_single_stone(&mut self, test_position: &FieldPosition, test_stone: StoneColor) {
        self.fields[test_position.x_coord as usize][test_position.y_coord as usize] =
            FieldContent::Stone(test_stone);
    }

    /// Sets a stone at the indicated position, assumes the move is valid. It performs all necessary flipping operations.
    pub fn set_stone(&mut self, test_position: &FieldPosition, test_stone: StoneColor) {
        debug_assert!(self.is_legal_move(test_position.clone(), test_stone));
        for dir in SCAN_DIRECTIONS.iter() {
            let amount_of_flipped_stones =
                self.get_potentially_flipped_stones(test_position.clone(), test_stone, dir);
            let mut base_point = test_position.clone();
            for _ in 0..amount_of_flipped_stones {
                base_point += dir;
                self.place_single_stone(&base_point, test_stone);
            }
        }
        self.place_single_stone(test_position, test_stone);
    }

    /// Checks the amount of potentially flipped stones, if we place a stone of the indicated color at the indicated position.
    /// and check for the indicated direction.
    fn get_potentially_flipped_stones(
        &self,
        mut test_position: FieldPosition,
        test_stone: StoneColor,
        scan_direction: &ScanDirection,
    ) -> u8 {
        if !test_position.is_valid() {
            return 0;
        }

        // Get first neighbor.
        test_position += scan_direction;
        if !test_position.is_valid() || self.is_empty(&test_position) {
            return 0;
        }
        let partner_stone = self.select_field(&test_position);
        if partner_stone == test_stone {
            return 0;
        }
        // We already have one stone.
        let mut counter = 1;
        loop {
            test_position += scan_direction;
            if !test_position.is_valid() || self.is_empty(&test_position) {
                return 0;
            }

            let scan_stone = self.select_field(&test_position);
            if test_stone == scan_stone {
                return counter;
            }
            if scan_stone != partner_stone {
                return 0;
            }
            counter += 1;
        }
    }
}
