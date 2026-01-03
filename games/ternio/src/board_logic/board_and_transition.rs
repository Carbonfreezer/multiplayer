//! This is the system for prepared board information for visualization and animation administration.

use crate::board_logic::board_representation::{
    FieldPosition, GameBoard, NUM_OF_COLORS, StoneColor, StonePlacement,
};
use crate::network_logic::basic_commands::{DeltaInformation, GameState};
use crate::network_logic::view_state::ViewState;
use crate::render_system::animator::Animator;
use crate::render_system::media::{Media, draw_game_board, draw_movement_options};

/// Represents a board and a stone currently in transition.
pub struct TransitionBoard {
    /// The stone animator we use.
    stone_animator: Animator,
    /// The final move we execute.
    move_command: DeltaInformation,
    /// Current score for all players / colors.
    red_green_blue: [i8; NUM_OF_COLORS],
}

impl TransitionBoard {
    /// Creates a new transition module. The  board as is and the move command in form of the delta information
    /// to extract the information for stone flipping.
    ///
    /// # Panic
    /// The delta information handed over has to be a move command.
    pub fn new(move_command: DeltaInformation, game_board: &GameBoard) -> TransitionBoard {
        let DeltaInformation::MakeMove(turn) = &move_command else {
            panic! {"Wrong delta information in new."};
        };

        let flipped_stones =
            game_board.get_all_flipped_stones(turn.field_position.clone(), turn.stone_color);
        let buffered_positions = game_board.get_stone_placement();
        let filtered_positions: Vec<StonePlacement> = buffered_positions
            .iter()
            .filter(|original| {
                flipped_stones
                    .iter()
                    .all(|flipped| flipped.field_position != original.field_position)
            })
            .cloned()
            .collect();

        let animator = Animator::new(
            buffered_positions,
            filtered_positions,
            flipped_stones,
            turn.clone(),
        );

        TransitionBoard {
            stone_animator: animator,
            move_command,
            red_green_blue: game_board.get_score(),
        }
    }

    /// Updates and returns true if the stone animation is finished. If this is the case the move is
    /// made permanent in the board contained in view state.
    pub fn update(&mut self, delta_time: f32, internal_state: &mut ViewState) -> bool {
        let finished = self.stone_animator.update(delta_time);
        if finished {
            internal_state.apply_delta(&self.move_command);
        }
        finished
    }

    /// Renders the stones as indicated, which is done by the animator.
    pub fn render(&self, media: &Media) {
        self.stone_animator.render();
        media.draw_score(self.red_green_blue);
    }
}

/// This is a helper structure that gets build for rendering, not that everything has to be build every frame.
/// This is done in the case of not being in animation transition.
pub struct BufferedBoardForRendering {
    /// All stones being placed on the board.
    stone_collection: Vec<StonePlacement>,
    /// The possible moves we have upcoming.
    possible_moves: Vec<FieldPosition>,
    /// The next player to move, needed to color the upcoming field positions *possible_moves*.
    next_move_color: StoneColor,
    /// The current score for the different players
    score: [i8; NUM_OF_COLORS],
    /// The game state we are currently in.
    game_ended: bool,
    /// The next player to display. (max be YOU!)
    next_player_to_display: String,
    /// All player names  enumerated by red green blue.
    player_names: [String; NUM_OF_COLORS],
}

impl BufferedBoardForRendering {
    /// Creates the board rendering from the view state, which has the board and the player id, that
    /// belongs to the current client. This is needed to determine, when it is the local players turn.
    ///
    /// # Panic
    /// The board should not get constructed when we are in the start up phase.
    pub fn new(view_state: &ViewState, player_id: u16) -> BufferedBoardForRendering {
        let player_names = view_state.get_player_names_in_rgb_sequence();
        let (next_move, mut next_player) = match view_state.game_state {
            GameState::Move(color) => (color, player_names[color as usize].clone()),
            GameState::GameOver => (StoneColor::Red, String::from("")),
            GameState::AwaitingPlayers | GameState::AssigningPlayers => {
                panic!("Wrong game state!");
            }
        };
        if view_state.player_colors[player_id as usize] == next_move {
            next_player = String::from("YOU !");
        }

        let stone_collection = view_state.game_board.get_stone_placement();
        let possible_moves = view_state.game_board.get_all_legal_moves(next_move);
        let score = view_state.game_board.get_score();
        BufferedBoardForRendering {
            stone_collection,
            possible_moves,
            next_move_color: next_move,
            score,
            game_ended: view_state.game_state == GameState::GameOver,
            next_player_to_display: next_player,
            player_names,
        }
    }

    /// Renders the buffered board, with movement options and the header.
    pub fn render(&self, media: &Media) {
        draw_game_board(&self.stone_collection);
        draw_movement_options(&self.possible_moves, self.next_move_color);
        media.draw_score(self.score);

        if self.game_ended {
            let max = *self.score.iter().max().unwrap();
            let winners: Vec<_> = self
                .score
                .iter()
                .enumerate()
                .filter(|(_, s)| **s == max)
                .map(|(index, _)| index)
                .collect();
            if let [winner] = winners[..] {
                media.draw_header(format!("{} has won!", self.player_names[winner]).as_str());
            } else {
                media.draw_header("Game Over - Draw!")
            }
        } else {
            media.draw_header(format!("Next: {}", self.next_player_to_display).as_str());
        }
    }

    /// Gets the possible moves.
    pub fn possible_moves(&self) -> &Vec<FieldPosition> {
        &self.possible_moves
    }
}

/// The different presentation states the board may be in.
pub enum PresentationState {
    /// The presentation has not yet been set.
    None,
    /// We are currently animating stones  by executing a move.
    Animating(TransitionBoard),
    /// The animation is completed and on the correct client we are waiting for movement input.
    WaitingForInput(BufferedBoardForRendering),
}
