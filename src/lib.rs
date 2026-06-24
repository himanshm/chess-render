//! # chess-render
//!
//! A configurable, embeddable chess GUI built with [Macroquad](https://macroquad.rs/).
//!
//! This crate provides a ready‑to‑use chess board widget that handles:
//! - Rendering the board and pieces from a texture atlas.
//! - Valid move generation and enforcement via the [`chess`](https://crates.io/crates/chess) crate.
//! - Two‑player local play with automatic board flip after each move.
//! - Optional UCI engine integration (behind the `uci` feature flag).
//! - Endgame detection (checkmate, stalemate) and a restart button.
//!
//! ## Quick Start
//!
//! The typical usage is to create a [`ChessGui`] instance inside a Macroquad application,
//! load the piece textures with [`load_pieces`](ChessGui::load_pieces), and then call
//! [`update`](ChessGui::update) every frame.
//!
//! ```no_run
//! use chess_render::{ChessGui, ChessConfig};
//!
//! #[macroquad::main("Chess")]
//! async fn main() {
//!     let config = ChessConfig::default();
//!     let mut gui = ChessGui::new(config);
//!     gui.load_pieces().await;
//!
//!     loop {
//!         gui.update().await;
//!         next_frame().await;
//!     }
//! }
//! ```
//!
//! ## Features
//!
//! - **`uci`** – enables UCI engine support. When this feature is active, you can set
//!   [`uci_engine_path`](ChessConfig::uci_engine_path) to play against a UCI‑compatible engine.
//!   The engine will move as Black (this is a placeholder implementation that only plays the first
//!   legal move; a full engine integration is left as an exercise for the user).
//!
//! ## Piece Texture Specification
//!
//! The piece texture must be a PNG of exactly **384×128** pixels.
//! It contains 6 white pieces and 6 black pieces, each 64×64 pixels.
//!
//! ### Layout
//!
//! | Row        | X=0     | X=64    | X=128   | X=192   | X=256   | X=320   |
//! |------------|---------|---------|---------|---------|---------|---------|
//! | **y=0**    | White King | White Queen | White Bishop | White Knight | White Rook | White Pawn |
//! | **y=64**   | Black King | Black Queen | Black Bishop | Black Knight | Black Rook | Black Pawn |
//!
//! If a custom path is not provided (or fails to load), the crate falls back to a built‑in
//! default texture embedded in the binary.
//!
//! ## Coordinate System
//!
//! The board is drawn as an 8×8 grid where each square is 64×64 pixels.
//! The board is always aligned to the top‑left corner of the window (0,0) and is 512×512 pixels.
//! The perspective can be flipped so that the current player’s side is at the bottom.
//!
//! ## Game Flow
//!
//! 1. The game starts with a standard starting position.
//! 2. Players take turns by clicking and dragging pieces.
//! 3. Legal moves are enforced; pawn promotion is automatically done to a Queen.
//! 4. After each move, the board flips to show the opponent’s perspective.
//! 5. When checkmate or stalemate occurs, an overlay message is shown and the user can press `R` to restart.
//! 6. If the `uci` feature is enabled and an engine path is set, the engine moves as Black.
//!
//! ## Customisation
//!
//! Use [`ChessConfig`] to change the square colours and optionally provide a custom piece sheet.
//! You can also enable UCI engine play.

use chess::{
    Board, ChessMove, Color as ChessColor, File, MoveGen, Piece as ChessPiece, Rank, Square,
};
use macroquad::prelude::*;

// Embed the default piece texture directly into the binary.
// This assumes the file is located at `src/assets/pieces.png`.
const DEFAULT_PIECES_PNG: &[u8] = include_bytes!("assets/pieces.png");

/// Custom game result enum since the `chess` crate doesn't provide one directly.
///
/// This represents the outcome of a finished game:
/// - `WhiteWins` – White checkmated Black.
/// - `BlackWins` – Black checkmated White.
/// - `Draw` – Stalemate (or, in future, other draw conditions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    /// White has won by checkmate.
    WhiteWins,
    /// Black has won by checkmate.
    BlackWins,
    /// The game ended in a draw (currently only stalemate is detected).
    Draw,
}

/// Helper to convert 0-7 file/rank indices to `chess::Square`.
///
/// # Panics
/// This function uses `unreachable!()` if the input is outside 0..7.
/// It is only called with valid indices from board iteration, so it is safe.
fn get_square(file: u32, rank: u32) -> Square {
    let f = match file {
        0 => File::A,
        1 => File::B,
        2 => File::C,
        3 => File::D,
        4 => File::E,
        5 => File::F,
        6 => File::G,
        7 => File::H,
        _ => unreachable!(),
    };
    let r = match rank {
        0 => Rank::First,
        1 => Rank::Second,
        2 => Rank::Third,
        3 => Rank::Fourth,
        4 => Rank::Fifth,
        5 => Rank::Sixth,
        6 => Rank::Seventh,
        7 => Rank::Eighth,
        _ => unreachable!(),
    };
    Square::make_square(r, f)
}

/// Checks the board state to determine if the game has ended.
///
/// Returns `Some(GameResult)` if the side to move has no legal moves:
/// - If the king is in check → checkmate (winner is the other side).
/// - If the king is not in check → stalemate (draw).
fn check_game_result(board: &Board) -> Option<GameResult> {
    let mut move_gen = MoveGen::new_legal(board);
    if move_gen.next().is_none() {
        // No legal moves → checkmate or stalemate
        if board.checkers().0 == 0 {
            Some(GameResult::Draw) // Stalemate
        } else if board.side_to_move() == ChessColor::White {
            Some(GameResult::BlackWins) // Checkmate: White to move but in check
        } else {
            Some(GameResult::WhiteWins) // Checkmate: Black to move but in check
        }
    } else {
        None
    }
}

/// Configuration for the chess GUI appearance and behaviour.
///
/// Use the builder‑style or direct field assignment to customise the look and optional
/// engine path. All fields have sensible defaults.
#[derive(Debug, Clone)]
pub struct ChessConfig {
    /// Colour of the light squares. Default is an off‑white (`#fffdcc`).
    pub light_square_color: Color,
    /// Colour of the dark squares. Default is `GRAY`.
    pub dark_square_color: Color,
    /// Optional path to a custom piece texture sheet (PNG, 384×128).
    /// If `None`, the built‑in default pieces are used.
    pub piece_texture_path: Option<String>,
    /// If set, plays against a UCI engine at this path as Black.
    /// This field is only available when the `uci` feature is enabled.
    #[cfg(feature = "uci")]
    pub uci_engine_path: Option<String>,
}

impl Default for ChessConfig {
    fn default() -> Self {
        Self {
            light_square_color: Color::new(255. / 255., 253. / 255., 208. / 255., 1.),
            dark_square_color: GRAY,
            piece_texture_path: None,
            #[cfg(feature = "uci")]
            uci_engine_path: None,
        }
    }
}

/// Internal representation of a piece for rendering mapping.
///
/// This enum maps each chess piece + colour combination to a unique variant.
/// It provides the source rectangle coordinates inside the 384×128 texture atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderPiece {
    WhiteKing,
    WhiteQueen,
    WhiteBishop,
    WhiteKnight,
    WhiteRook,
    WhitePawn,
    BlackKing,
    BlackQueen,
    BlackBishop,
    BlackKnight,
    BlackRook,
    BlackPawn,
}

impl RenderPiece {
    /// Converts a `chess::Piece` and `chess::Color` into a `RenderPiece`.
    fn from_chess(piece: ChessPiece, color: ChessColor) -> Self {
        match (color, piece) {
            (ChessColor::White, ChessPiece::King) => RenderPiece::WhiteKing,
            (ChessColor::White, ChessPiece::Queen) => RenderPiece::WhiteQueen,
            (ChessColor::White, ChessPiece::Bishop) => RenderPiece::WhiteBishop,
            (ChessColor::White, ChessPiece::Knight) => RenderPiece::WhiteKnight,
            (ChessColor::White, ChessPiece::Rook) => RenderPiece::WhiteRook,
            (ChessColor::White, ChessPiece::Pawn) => RenderPiece::WhitePawn,
            (ChessColor::Black, ChessPiece::King) => RenderPiece::BlackKing,
            (ChessColor::Black, ChessPiece::Queen) => RenderPiece::BlackQueen,
            (ChessColor::Black, ChessPiece::Bishop) => RenderPiece::BlackBishop,
            (ChessColor::Black, ChessPiece::Knight) => RenderPiece::BlackKnight,
            (ChessColor::Black, ChessPiece::Rook) => RenderPiece::BlackRook,
            (ChessColor::Black, ChessPiece::Pawn) => RenderPiece::BlackPawn,
        }
    }

    /// Returns the `(x, y)` top‑left corner of the 64×64 sprite in the texture atlas.
    fn tex_coords(&self) -> (f32, f32) {
        match self {
            RenderPiece::WhiteKing => (0.0, 0.0),
            RenderPiece::WhiteQueen => (64.0, 0.0),
            RenderPiece::WhiteBishop => (128.0, 0.0),
            RenderPiece::WhiteKnight => (192.0, 0.0),
            RenderPiece::WhiteRook => (256.0, 0.0),
            RenderPiece::WhitePawn => (320.0, 0.0),
            RenderPiece::BlackKing => (0.0, 64.0),
            RenderPiece::BlackQueen => (64.0, 64.0),
            RenderPiece::BlackBishop => (128.0, 64.0),
            RenderPiece::BlackKnight => (192.0, 64.0),
            RenderPiece::BlackRook => (256.0, 64.0),
            RenderPiece::BlackPawn => (320.0, 64.0),
        }
    }
}

/// The main Chess GUI struct.
///
/// Holds the current board state, configuration, loaded texture, interaction state,
/// and game result. You create one instance and call [`update`](ChessGui::update)
/// in your game loop.
pub struct ChessGui {
    /// The current chess board (from the `chess` crate).
    board: Board,
    /// Configuration for appearance and optional engine.
    config: ChessConfig,
    /// Loaded piece texture (optional until loaded).
    pieces_texture: Option<Texture2D>,
    /// The square that is currently selected (clicked) by the user.
    selected_square: Option<Square>,
    /// If a piece is being dragged, stores the source square and the offset
    /// from the mouse click to the top‑left of the square.
    dragging_piece: Option<(Square, f32, f32)>,
    /// Which side is shown at the bottom of the board (White or Black).
    perspective: ChessColor,
    /// The result of the game, if it has ended.
    game_result: Option<GameResult>,
    /// A status message (e.g., "White Wins!", "Game Drawn").
    status_message: String,
}

impl ChessGui {
    /// Creates a new chess GUI with the given configuration.
    ///
    /// The board is initialised to the standard starting position.
    /// The piece texture is not loaded yet – you must call [`load_pieces`](ChessGui::load_pieces)
    /// before rendering.
    ///
    /// # Example
    /// ```
    /// let config = ChessConfig::default();
    /// let gui = ChessGui::new(config);
    /// ```
    pub fn new(config: ChessConfig) -> Self {
        Self {
            board: Board::default(),
            config,
            pieces_texture: None,
            selected_square: None,
            dragging_piece: None,
            perspective: ChessColor::White,
            game_result: None,
            status_message: String::new(),
        }
    }

    /// Loads the piece textures from the configured path or falls back to the built‑in default.
    ///
    /// This method should be called once before the main loop, or at least before the first
    /// call to [`update`](ChessGui::update). It reads the file asynchronously (using `std::fs`
    /// in a blocking manner – see note below).
    ///
    /// # Blocking I/O
    ///
    /// This method uses synchronous `std::fs::read`, which may block the async executor.
    /// In a real application you might want to load textures asynchronously; this is a
    /// simplification suitable for most Macroquad games.
    ///
    /// # Errors
    ///
    /// If the custom path fails to load, an error is printed to stderr and the default
    /// texture is used instead. If the loaded image does not have the expected dimensions
    /// (384×128), a warning is printed, but the texture is still used.
    pub async fn load_pieces(&mut self) {
        let image_data = if let Some(ref path) = self.config.piece_texture_path {
            match std::fs::read(path) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!(
                        "Failed to load custom texture from {}: {}. Falling back to default.",
                        path, e
                    );
                    DEFAULT_PIECES_PNG.to_vec()
                }
            }
        } else {
            DEFAULT_PIECES_PNG.to_vec()
        };

        self.pieces_texture = Some(Texture2D::from_file_with_format(&image_data, None));

        if let Some(tex) = &self.pieces_texture {
            if tex.width() as u32 != 384 || tex.height() as u32 != 128 {
                eprintln!(
                    "Warning: piece texture is {}x{}, expected 384x128.",
                    tex.width(),
                    tex.height()
                );
            }
        }
    }

    /// Updates the GUI: draws the board, handles user input, and processes engine moves.
    ///
    /// This method must be called once per frame in your game loop. It assumes that
    /// the Macroquad context is active and that the window is at least 512×512 pixels.
    /// It clears the background with the light square colour, draws the board and pieces,
    /// and processes mouse events.
    ///
    /// # Game Flow
    ///
    /// - If the game is not over, the user can interact with pieces via click‑and‑drag.
    /// - If the `uci` feature is enabled and an engine path is set, the engine moves
    ///   as Black automatically after each human move (it plays the first legal move found).
    /// - When the game ends, an overlay message is shown and pressing the `R` key restarts.
    ///
    /// # Panics
    ///
    /// This method may panic if the piece texture is not loaded before calling it;
    /// however it will display an error text instead of crashing.
    pub async fn update(&mut self) {
        // Check for game end
        if self.game_result.is_none() {
            self.game_result = check_game_result(&self.board);
            if let Some(result) = &self.game_result {
                self.status_message = match result {
                    GameResult::WhiteWins => "White Wins!".to_string(),
                    GameResult::BlackWins => "Black Wins!".to_string(),
                    GameResult::Draw => "Game Drawn".to_string(),
                };
            }
        }

        clear_background(self.config.light_square_color);
        self.draw_board();

        if self.game_result.is_none() {
            self.handle_input();

            // UCI Engine Move (Placeholder)
            #[cfg(feature = "uci")]
            if let Some(ref _engine_path) = self.config.uci_engine_path {
                if self.board.side_to_move() == ChessColor::Black && self.dragging_piece.is_none() {
                    let moves: Vec<ChessMove> = MoveGen::new_legal(&self.board).collect();
                    if !moves.is_empty() {
                        let m = moves[0];
                        // make_move_new returns the new board state immutably
                        self.board = self.board.make_move_new(m);
                        self.perspective = ChessColor::White;
                        self.game_result = check_game_result(&self.board);
                    }
                }
            }
        } else {
            // Draw End Game Message
            draw_text(
                &self.status_message,
                512.0 / 2.0 - measure_text(&self.status_message, None, 40, 1.0).width / 2.0,
                512.0 / 2.0,
                40.0,
                RED,
            );
            draw_text(
                "Press R to Restart",
                512.0 / 2.0 - measure_text("Press R to Restart", None, 20, 1.0).width / 2.0,
                512.0 / 2.0 + 40.0,
                20.0,
                BLACK,
            );

            if is_key_pressed(KeyCode::R) {
                self.restart();
            }
        }
    }

    /// Resets the game to the initial position.
    ///
    /// Clears the board, selected square, dragging state, sets perspective to White,
    /// and clears the game result and status message.
    fn restart(&mut self) {
        self.board = Board::default();
        self.selected_square = None;
        self.dragging_piece = None;
        self.perspective = ChessColor::White;
        self.game_result = None;
        self.status_message.clear();
    }

    /// Draws the board, pieces, and any dragged piece.
    ///
    /// This method is called by [`update`](ChessGui::update) and assumes the texture is loaded.
    /// It draws all 64 squares, then pieces (except the one being dragged), and finally
    /// the dragged piece on top.
    fn draw_board(&self) {
        let Some(texture) = &self.pieces_texture else {
            draw_text("Load pieces texture first!", 100.0, 256.0, 20.0, RED);
            return;
        };

        for rank in 0..8 {
            for file in 0..8 {
                let is_light = (file + rank) % 2 == 0;
                let color = if is_light {
                    self.config.light_square_color
                } else {
                    self.config.dark_square_color
                };

                // Calculate screen coordinates based on perspective
                let (screen_x, screen_y) = if self.perspective == ChessColor::White {
                    (file as f32 * 64.0, (7 - rank) as f32 * 64.0)
                } else {
                    ((7 - file) as f32 * 64.0, rank as f32 * 64.0)
                };

                draw_rectangle(screen_x, screen_y, 64.0, 64.0, color);

                let sq = get_square(file as u32, rank as u32);

                if let Some(piece) = self.board.piece_on(sq) {
                    let color_on_sq = self.board.color_on(sq).unwrap();
                    let render_piece = RenderPiece::from_chess(piece, color_on_sq);

                    // Don't draw if dragging this specific piece
                    if let Some((drag_sq, _, _)) = self.dragging_piece {
                        if drag_sq == sq {
                            continue;
                        }
                    }

                    let (tx, ty) = render_piece.tex_coords();
                    draw_texture_ex(
                        texture,
                        screen_x,
                        screen_y,
                        WHITE,
                        DrawTextureParams {
                            source: Some(Rect::new(tx, ty, 64.0, 64.0)),
                            ..Default::default()
                        },
                    );
                }
            }
        }

        // Draw dragged piece
        if let Some((sq, offset_x, offset_y)) = self.dragging_piece {
            if let Some(piece) = self.board.piece_on(sq) {
                let color_on_sq = self.board.color_on(sq).unwrap();
                let render_piece = RenderPiece::from_chess(piece, color_on_sq);
                let (mx, my) = mouse_position();

                let (tx, ty) = render_piece.tex_coords();
                draw_texture_ex(
                    texture,
                    mx - offset_x,
                    my - offset_y,
                    WHITE,
                    DrawTextureParams {
                        source: Some(Rect::new(tx, ty, 64.0, 64.0)),
                        ..Default::default()
                    },
                );
            }
        }
    }

    /// Processes mouse input for piece selection, dragging, and move execution.
    ///
    /// This method is called only when the game is not over. It handles:
    /// - Left mouse button press: select a piece if it belongs to the side to move.
    /// - Mouse movement while dragging: updates the dragged piece position (handled by the render loop).
    /// - Left mouse button release: attempt to move the selected piece to the target square.
    ///
    /// If the move is legal, it is executed, the board is updated, perspective is flipped,
    /// and the game result is re‑evaluated in the next call to [`update`](ChessGui::update).
    fn handle_input(&mut self) {
        let (mx, my) = mouse_position();

        let file = (mx / 64.0) as i32;
        let rank = (my / 64.0) as i32;

        if file < 0 || file > 7 || rank < 0 || rank > 7 {
            if is_mouse_button_released(MouseButton::Left) {
                self.dragging_piece = None;
                self.selected_square = None;
            }
            return;
        }

        // Map screen coordinates to logical board coordinates based on perspective
        let logical_file = if self.perspective == ChessColor::White {
            file
        } else {
            7 - file
        };
        let logical_rank = if self.perspective == ChessColor::White {
            7 - rank
        } else {
            rank
        };

        let sq = get_square(logical_file as u32, logical_rank as u32);

        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(_piece) = self.board.piece_on(sq) {
                if self.board.color_on(sq) == Some(self.board.side_to_move()) {
                    self.selected_square = Some(sq);

                    // Calculate exact offset based on where the user clicked
                    let (screen_x, screen_y) = if self.perspective == ChessColor::White {
                        (logical_file as f32 * 64.0, (7 - logical_rank) as f32 * 64.0)
                    } else {
                        ((7 - logical_file) as f32 * 64.0, logical_rank as f32 * 64.0)
                    };
                    let offset_x = mx - screen_x;
                    let offset_y = my - screen_y;

                    self.dragging_piece = Some((sq, offset_x, offset_y));
                }
            }
        } else if is_mouse_button_released(MouseButton::Left) {
            if let Some(from_sq) = self.selected_square {
                if from_sq != sq {
                    // Auto-promote to Queen for simplicity
                    let promote_to = if self.board.piece_on(from_sq) == Some(ChessPiece::Pawn) {
                        if (self.board.side_to_move() == ChessColor::White && logical_rank == 7)
                            || (self.board.side_to_move() == ChessColor::Black && logical_rank == 0)
                        {
                            Some(ChessPiece::Queen)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let chess_move = ChessMove::new(from_sq, sq, promote_to);
                    if self.board.legal(chess_move) {
                        // make_move_new returns the new board state immutably
                        self.board = self.board.make_move_new(chess_move);

                        // Flip perspective for local two-player mode
                        self.perspective = match self.perspective {
                            ChessColor::White => ChessColor::Black,
                            ChessColor::Black => ChessColor::White,
                        };
                    }
                }
                self.selected_square = None;
                self.dragging_piece = None;
            }
        }
    }
}
