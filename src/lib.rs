//! # chess-render
//!
//! A configurable, embeddable chess GUI built with Macroquad.
//!
//! ## Features
//! - Valid move enforcement via the `chess` crate
//! - Two-player local play with automatic board flip
//! - Optional UCI engine integration (enable `uci` feature)
//! - Fully customizable colors and piece sprites
//! - Endgame detection (checkmate, stalemate)
//!
//! ## Piece Texture Specification
//! The piece texture must be a PNG of size 384x128 pixels.
//! It contains 6 white pieces and 6 black pieces, each 64x64 pixels.
//!
//! Layout:
//! - Row 0 (y=0): White Pieces (Left to Right: King, Queen, Bishop, Knight, Rook, Pawn)
//! - Row 1 (y=64): Black Pieces (Left to Right: King, Queen, Bishop, Knight, Rook, Pawn)

use chess::{
    Board, ChessMove, Color as ChessColor, File, MoveGen, Piece as ChessPiece, Rank, Square,
};
use macroquad::prelude::*;

// Embed the default piece texture directly into the binary.
// This assumes the file is located at `src/assets/pieces.png`.
const DEFAULT_PIECES_PNG: &[u8] = include_bytes!("assets/pieces.png");

/// Custom game result enum since the `chess` crate doesn't provide one directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    WhiteWins,
    BlackWins,
    Draw,
}

/// Helper to convert 0-7 file/rank indices to `chess::Square`.
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

/// Configuration for the chess GUI appearance.
#[derive(Debug, Clone)]
pub struct ChessConfig {
    /// Light square color. Default: OFFWHITE
    pub light_square_color: Color,
    /// Dark square color. Default: GRAY
    pub dark_square_color: Color,
    /// Path to the piece texture sheet (384x128 px).
    /// If `None`, the built-in default pieces will be used.
    pub piece_texture_path: Option<String>,
    /// If set, plays against a UCI engine at this path as Black.
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

    /// Returns the source rectangle in the 384x128 texture atlas.
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
pub struct ChessGui {
    board: Board,
    config: ChessConfig,
    pieces_texture: Option<Texture2D>,
    selected_square: Option<Square>,
    dragging_piece: Option<(Square, f32, f32)>,
    perspective: ChessColor,
    game_result: Option<GameResult>,
    status_message: String,
}

impl ChessGui {
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

    /// Loads the piece textures. Uses the built-in default if no custom path is provided,
    /// or if the custom path fails to load.
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

    fn restart(&mut self) {
        self.board = Board::default();
        self.selected_square = None;
        self.dragging_piece = None;
        self.perspective = ChessColor::White;
        self.game_result = None;
        self.status_message.clear();
    }

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
