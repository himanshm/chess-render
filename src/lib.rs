//! # chess-render
//!
//! A configurable, embeddable chess GUI built with [Macroquad](https://macroquad.rs/).
//!
//! This crate provides a ready‑to‑use chess board widget that handles:
//! - Rendering the board and pieces from a texture atlas.
//! - Valid move generation and enforcement via the [`chess`](https://crates.io/crates/chess) crate.
//! - Two‑player local play with automatic board flip after each move (optional).
//! - Optional UCI engine integration (behind the `uci` feature flag) – now with proper asynchronous
//!   communication via a background thread.
//! - Endgame detection (checkmate, stalemate) and a restart button.
//! - Automatic centering of the board in the window (configurable).
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
//!     let config = ChessConfig::default(); // board is centered automatically
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
//!   The engine moves as Black (configurable via `uci_plays_as`).
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
//! The board is drawn as an 8×8 grid where each square has the size given by `square_size`
//! (default 64). The board is placed either centered in the window (default) or at a fixed
//! offset if `center_board` is set to `false`. The perspective can be flipped so that the
//! current player’s side is at the bottom.
//!
//! ## Game Flow
//!
//! 1. The game starts with a standard starting position.
//! 2. Players take turns by clicking and dragging pieces.
//! 3. Legal moves are enforced; pawn promotion uses a pop‑up selector (Queen, Rook, Bishop, Knight).
//! 4. After each move, the board flips to show the opponent’s perspective if `auto_flip_perspective` is `true`.
//! 5. When checkmate or stalemate occurs, an overlay message is shown and the user can press `R` or click the "New Game" button to restart.
//! 6. If the `uci` feature is enabled and an engine path is set, the engine moves as configured
//!    (by default as Black) using an asynchronous background process.

use std::str::FromStr;

use chess::{
    Board, BoardStatus, ChessMove, Color as ChessColor, File, MoveGen, Piece as ChessPiece, Rank,
    Square,
};
use macroquad::prelude::*;

// Embed the default piece texture directly into the binary.
const DEFAULT_PIECES_PNG: &[u8] = include_bytes!("assets/pieces.png");

/// Custom game result enum that covers all end‑game states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    WhiteWins,
    BlackWins,
    Draw,
}

/// Helper to convert 0-7 file/rank indices to `chess::Square`.
/// Returns `None` if the indices are out of range.
pub(crate) fn get_square(file: u32, rank: u32) -> Option<Square> {
    let f = match file {
        0 => File::A,
        1 => File::B,
        2 => File::C,
        3 => File::D,
        4 => File::E,
        5 => File::F,
        6 => File::G,
        7 => File::H,
        _ => return None,
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
        _ => return None,
    };
    Some(Square::make_square(r, f))
}

/// Checks the board state for checkmate or stalemate using `Board::status()`.
/// Returns `Some(GameResult)` if the game has ended.
fn check_game_result(board: &Board) -> Option<GameResult> {
    match board.status() {
        BoardStatus::Checkmate => {
            if board.side_to_move() == ChessColor::White {
                Some(GameResult::BlackWins)
            } else {
                Some(GameResult::WhiteWins)
            }
        }
        BoardStatus::Stalemate => Some(GameResult::Draw),
        _ => None,
    }
}

/// Configuration for the chess GUI appearance and behaviour.
#[derive(Debug, Clone)]
pub struct ChessConfig {
    /// Colour of the light squares. Default: off‑white (`#fffdcc`).
    pub light_square_color: Color,
    /// Colour of the dark squares. Default: `GRAY`.
    pub dark_square_color: Color,
    /// Size of each square in pixels. Default: 64.
    pub square_size: f32,
    /// Offset of the board from the top‑left corner of the window. Only used when `center_board` is `false`.
    /// Default: (0.0, 0.0).
    pub board_offset: (f32, f32),
    /// If `true`, the board is automatically centered in the window (ignoring `board_offset`). Default: `true`.
    pub center_board: bool,
    /// Whether to automatically flip the board perspective after each move. Default: true.
    pub auto_flip_perspective: bool,
    /// The piece to promote to when a pawn reaches the last rank. Default: Queen.
    /// **Note:** This is now used as the *default* promotion piece; the user can still choose a different one via the pop‑up.
    pub promotion_piece: ChessPiece,
    /// Optional path to a custom piece texture sheet (PNG, 384×128).
    /// If `None`, the built‑in default pieces are used.
    pub piece_texture_path: Option<String>,
    /// If set, plays against a UCI engine at this path.
    #[cfg(feature = "uci")]
    pub uci_engine_path: Option<String>,
    /// Which side the UCI engine should play. Default: `ChessColor::Black`.
    #[cfg(feature = "uci")]
    pub uci_plays_as: ChessColor,
    /// Time (in milliseconds) to give the engine per move. Default: 1000 ms.
    #[cfg(feature = "uci")]
    pub uci_move_time_ms: u64,
}

impl Default for ChessConfig {
    fn default() -> Self {
        Self {
            light_square_color: Color::new(255. / 255., 253. / 255., 208. / 255., 1.),
            dark_square_color: GRAY,
            square_size: 64.0,
            board_offset: (0.0, 0.0),
            center_board: true,
            auto_flip_perspective: true,
            promotion_piece: ChessPiece::Queen,
            piece_texture_path: None,
            #[cfg(feature = "uci")]
            uci_engine_path: None,
            #[cfg(feature = "uci")]
            uci_plays_as: ChessColor::Black,
            #[cfg(feature = "uci")]
            uci_move_time_ms: 1000,
        }
    }
}

/// Internal representation of a piece for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    // Cached rectangles for piece sprites
    piece_rects: std::collections::HashMap<RenderPiece, Rect>,

    // ---------- Usability enhancements ----------
    last_move: Option<(Square, Square)>, // last move source & destination
    pending_promotion: Option<ChessMove>, // move awaiting promotion choice
    promotion_choices: Vec<ChessPiece>,  // always [Queen, Rook, Bishop, Knight]
    // --------------------------------------------
    #[cfg(feature = "uci")]
    uci_engine: Option<uci::UciEngine>,
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
            piece_rects: std::collections::HashMap::new(),
            last_move: None,
            pending_promotion: None,
            promotion_choices: vec![
                ChessPiece::Queen,
                ChessPiece::Rook,
                ChessPiece::Bishop,
                ChessPiece::Knight,
            ],
            #[cfg(feature = "uci")]
            uci_engine: None,
        }
    }

    /// Returns the current board offset (either the configured offset or centered, depending on `center_board`).
    fn get_board_offset(&self) -> (f32, f32) {
        if self.config.center_board {
            let board_pixels = self.config.square_size * 8.0;
            let ox = (screen_width() - board_pixels) / 2.0;
            let oy = (screen_height() - board_pixels) / 2.0;
            (ox, oy)
        } else {
            self.config.board_offset
        }
    }

    /// Asynchronously loads the piece texture from the configured path or falls back to the default.
    pub async fn load_pieces(&mut self) {
        let image_data = if let Some(ref path) = self.config.piece_texture_path {
            match load_file(path).await {
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

        let mut tex = Texture2D::from_file_with_format(&image_data, None);
        tex.set_filter(FilterMode::Nearest);

        if tex.width() == 0.0
            || tex.height() == 0.0
            || tex.width() as u32 != 384
            || tex.height() as u32 != 128
        {
            eprintln!(
                "Invalid piece texture (dimensions {}x{}), reloading default.",
                tex.width(),
                tex.height()
            );
            let default_tex = Texture2D::from_file_with_format(DEFAULT_PIECES_PNG, None);
            default_tex.set_filter(FilterMode::Nearest);
            tex = default_tex;
            if tex.width() == 0.0 || tex.height() == 0.0 {
                eprintln!("CRITICAL: Default texture also failed to load!");
                self.pieces_texture = None;
                return;
            }
        }

        for variant in [
            RenderPiece::WhiteKing,
            RenderPiece::WhiteQueen,
            RenderPiece::WhiteBishop,
            RenderPiece::WhiteKnight,
            RenderPiece::WhiteRook,
            RenderPiece::WhitePawn,
            RenderPiece::BlackKing,
            RenderPiece::BlackQueen,
            RenderPiece::BlackBishop,
            RenderPiece::BlackKnight,
            RenderPiece::BlackRook,
            RenderPiece::BlackPawn,
        ] {
            let (x, y) = variant.tex_coords();
            self.piece_rects
                .insert(variant, Rect::new(x, y, 64.0, 64.0));
        }

        self.pieces_texture = Some(tex);

        #[cfg(feature = "uci")]
        if let Some(ref path) = self.config.uci_engine_path {
            match uci::UciEngine::new(path, self.config.uci_move_time_ms) {
                Ok(engine) => {
                    self.uci_engine = Some(engine);
                    eprintln!("UCI engine started: {}", path);
                }
                Err(e) => {
                    eprintln!("Failed to start UCI engine: {}", e);
                    self.uci_engine = None;
                }
            }
        }
    }

    /// Updates the GUI: draws the board, handles input, and processes engine moves.
    pub async fn update(&mut self) {
        // Check game result (only if not already ended and not waiting for promotion)
        if self.pending_promotion.is_none() && self.game_result.is_none() {
            self.game_result = check_game_result(&self.board);
            if let Some(result) = &self.game_result {
                self.status_message = match result {
                    GameResult::WhiteWins => "White Wins!".to_string(),
                    GameResult::BlackWins => "Black Wins!".to_string(),
                    GameResult::Draw => "Game Drawn".to_string(),
                };
            }
        }

        clear_background(Color::new(180. / 255., 220. / 255., 255. / 255., 1.));
        self.draw_board();

        // Handle promotion pop‑up if active
        if self.pending_promotion.is_some() {
            self.draw_promotion_dialog();
            self.handle_promotion_input();
            return; // skip further input and engine moves while promoting
        }

        if self.game_result.is_none() {
            self.handle_input();

            #[cfg(feature = "uci")]
            if let Some(ref mut engine) = self.uci_engine {
                if self.board.side_to_move() == self.config.uci_plays_as
                    && self.dragging_piece.is_none()
                    && !engine.has_pending_move()
                {
                    let fen = self.board.to_string();
                    engine.request_move(fen);
                }

                if let Some(m) = engine.try_take_move() {
                    if self.try_move(m) {
                        // move applied
                    } else {
                        eprintln!("Engine played an illegal move! Ignoring.");
                    }
                }
            }
        } else {
            // End game overlay – drawn by draw_board already, but we also show a restart button
            // The "New Game" button is drawn every frame, so it will be visible.
        }
    }

    // ---------- Drawing methods ----------

    fn draw_board(&self) {
        let Some(texture) = &self.pieces_texture else {
            draw_text("Load pieces texture first!", 100.0, 256.0, 20.0, RED);
            return;
        };

        let size = self.config.square_size;
        let (ox, oy) = self.get_board_offset();
        let board_pixels = size * 8.0;

        // Draw squares
        for rank in 0..8 {
            for file in 0..8 {
                let is_light = (file + rank) % 2 == 0;
                let color = if is_light {
                    self.config.light_square_color
                } else {
                    self.config.dark_square_color
                };

                let (screen_x, screen_y) = if self.perspective == ChessColor::White {
                    (ox + file as f32 * size, oy + (7 - rank) as f32 * size)
                } else {
                    (ox + (7 - file) as f32 * size, oy + rank as f32 * size)
                };

                draw_rectangle(screen_x, screen_y, size, size, color);

                let sq = match get_square(file as u32, rank as u32) {
                    Some(s) => s,
                    None => continue,
                };

                // Highlight last move
                if let Some((from, to)) = self.last_move {
                    if sq == from || sq == to {
                        draw_rectangle(
                            screen_x,
                            screen_y,
                            size,
                            size,
                            Color::new(1.0, 0.8, 0.0, 0.3), // golden
                        );
                    }
                }

                // Highlight selected square
                if Some(sq) == self.selected_square {
                    draw_rectangle(
                        screen_x,
                        screen_y,
                        size,
                        size,
                        Color::new(1.0, 1.0, 0.0, 0.3),
                    );
                }

                // Draw move indicators
                if let Some(sel_sq) = self.selected_square {
                    if let Some(_piece) = self.board.piece_on(sel_sq) {
                        if self.board.color_on(sel_sq) == Some(self.board.side_to_move()) {
                            let moves = MoveGen::new_legal(&self.board)
                                .filter(|m| m.get_source() == sel_sq)
                                .map(|m| m.get_dest())
                                .collect::<Vec<_>>();
                            if moves.contains(&sq) {
                                let cx = screen_x + size / 2.0;
                                let cy = screen_y + size / 2.0;
                                draw_circle(cx, cy, size / 6.0, Color::new(0.0, 0.0, 0.0, 0.4));
                                if self.board.piece_on(sq).is_some() {
                                    draw_circle(cx, cy, size / 3.0, Color::new(0.0, 0.0, 0.0, 0.4));
                                }
                            }
                        }
                    }
                }

                // Draw piece
                if let Some(piece) = self.board.piece_on(sq) {
                    let color_on_sq = self.board.color_on(sq).unwrap();
                    let render_piece = RenderPiece::from_chess(piece, color_on_sq);

                    if let Some((drag_sq, _, _)) = self.dragging_piece {
                        if drag_sq == sq {
                            continue;
                        }
                    }

                    if let Some(rect) = self.piece_rects.get(&render_piece) {
                        draw_texture_ex(
                            texture,
                            screen_x,
                            screen_y,
                            WHITE,
                            DrawTextureParams {
                                source: Some(*rect),
                                dest_size: Some(Vec2::new(size, size)),
                                rotation: 0.0,
                                flip_x: false,
                                flip_y: false,
                                pivot: None,
                            },
                        );
                    }
                }
            }
        }

        // Draw board coordinates with dynamic contrast
        let font_size = (size / 4.0) as u16;
        for i in 0..8 {
            let file_label = (b'a' + i as u8) as char;
            let rank_label = (b'1' + i as u8) as char;

            // File labels (bottom) – use background color of the square below
            let file_sq = if self.perspective == ChessColor::White {
                get_square(i as u32, 0).unwrap()
            } else {
                get_square(7 - i as u32, 7).unwrap()
            };
            let file_sq_color = if self.board.piece_on(file_sq).is_some() {
                // fallback to square color
                if (i + 0) % 2 == 0 {
                    self.config.light_square_color
                } else {
                    self.config.dark_square_color
                }
            } else {
                if (i + 0) % 2 == 0 {
                    self.config.light_square_color
                } else {
                    self.config.dark_square_color
                }
            };
            let file_text_color = contrast_color(file_sq_color);

            let x = if self.perspective == ChessColor::White {
                ox + i as f32 * size + size / 2.0
            } else {
                ox + (7 - i) as f32 * size + size / 2.0
            };
            let y_bottom = oy + 8.0 * size + 5.0;
            draw_text(
                &file_label.to_string(),
                x - measure_text(&file_label.to_string(), None, font_size, 1.0).width / 2.0,
                y_bottom + font_size as f32,
                font_size as f32,
                file_text_color,
            );

            // Rank labels (left)
            // FIX: removed unused variable 'rank_sq'
            let rank_sq_color = if (0 + i) % 2 == 0 {
                self.config.light_square_color
            } else {
                self.config.dark_square_color
            };
            let rank_text_color = contrast_color(rank_sq_color);

            let y = if self.perspective == ChessColor::White {
                oy + (7 - i) as f32 * size + size / 2.0
            } else {
                oy + i as f32 * size + size / 2.0
            };
            let x_left = ox - 20.0;
            draw_text(
                &rank_label.to_string(),
                x_left,
                y + font_size as f32 / 2.0,
                font_size as f32,
                rank_text_color,
            );
        }

        // Draw dragged piece
        if let Some((sq, offset_x, offset_y)) = self.dragging_piece {
            if let Some(piece) = self.board.piece_on(sq) {
                let color_on_sq = self.board.color_on(sq).unwrap();
                let render_piece = RenderPiece::from_chess(piece, color_on_sq);
                let (mx, my) = mouse_position();

                if let Some(rect) = self.piece_rects.get(&render_piece) {
                    draw_texture_ex(
                        texture,
                        mx - offset_x,
                        my - offset_y,
                        WHITE,
                        DrawTextureParams {
                            source: Some(*rect),
                            dest_size: Some(Vec2::new(size, size)),
                            rotation: 0.0,
                            flip_x: false,
                            flip_y: false,
                            pivot: None,
                        },
                    );
                }
            }
        }

        // ---------- Turn indicator (below the board) ----------
        let turn_text = if self.game_result.is_some() {
            self.status_message.clone()
        } else {
            format!(
                "{} to move",
                if self.board.side_to_move() == ChessColor::White {
                    "White"
                } else {
                    "Black"
                }
            )
        };
        let text_size = measure_text(&turn_text, None, 24, 1.0);
        draw_text(
            &turn_text,
            ox + board_pixels / 2.0 - text_size.width / 2.0,
            oy + board_pixels + 35.0,
            24.0,
            BLACK,
        );

        // ---------- "New Game" button (bottom‑right of the board) ----------
        let btn_w = 120.0;
        let btn_h = 30.0;
        let btn_x = ox + board_pixels - btn_w - 10.0;
        let btn_y = oy + board_pixels + 10.0;
        draw_rectangle(btn_x, btn_y, btn_w, btn_h, DARKGRAY);
        draw_rectangle_lines(btn_x, btn_y, btn_w, btn_h, 1.0, BLACK);
        let btn_text = "New Game";
        let btn_text_size = measure_text(btn_text, None, 18, 1.0);
        draw_text(
            btn_text,
            btn_x + (btn_w - btn_text_size.width) / 2.0,
            btn_y + (btn_h + btn_text_size.height) / 2.0 - 2.0,
            18.0,
            WHITE,
        );
        // Store button bounds for input handling (we'll check in handle_input)
        // We'll just use global coordinates; we'll check in input handler.
    }

    fn draw_promotion_dialog(&self) {
        let size = self.config.square_size;
        let (ox, oy) = self.get_board_offset();
        let board_pixels = size * 8.0;

        // Semi‑transparent overlay
        draw_rectangle(
            ox,
            oy,
            board_pixels,
            board_pixels,
            Color::new(0., 0., 0., 0.7),
        );

        // Four piece choices (Queen, Rook, Bishop, Knight)
        let choice_size = size * 0.8;
        let gap = size * 0.3;
        let total_width = choice_size * 4.0 + gap * 3.0;
        let start_x = ox + (board_pixels - total_width) / 2.0;
        let start_y = oy + (board_pixels - choice_size) / 2.0;

        let promotion_pieces = [
            ChessPiece::Queen,
            ChessPiece::Rook,
            ChessPiece::Bishop,
            ChessPiece::Knight,
        ];

        let color = self.board.side_to_move(); // the side that is promoting

        for (i, &piece) in promotion_pieces.iter().enumerate() {
            let x = start_x + i as f32 * (choice_size + gap);
            let y = start_y;
            draw_rectangle(x, y, choice_size, choice_size, DARKGRAY);
            draw_rectangle_lines(x, y, choice_size, choice_size, 2.0, WHITE);

            // Draw the piece image
            let render_piece = RenderPiece::from_chess(piece, color);
            if let Some(rect) = self.piece_rects.get(&render_piece) {
                if let Some(texture) = &self.pieces_texture {
                    draw_texture_ex(
                        texture,
                        x,
                        y,
                        WHITE,
                        DrawTextureParams {
                            source: Some(*rect),
                            dest_size: Some(Vec2::new(choice_size, choice_size)),
                            rotation: 0.0,
                            flip_x: false,
                            flip_y: false,
                            pivot: None,
                        },
                    );
                }
            }
        }

        // Label
        let label = "Choose promotion piece";
        let label_size = measure_text(label, None, 20, 1.0);
        draw_text(
            label,
            ox + (board_pixels - label_size.width) / 2.0,
            oy + 30.0,
            20.0,
            WHITE,
        );
    }

    // ---------- Input handling ----------

    fn handle_input(&mut self) {
        let (mx, my) = mouse_position();
        let size = self.config.square_size;
        let (ox, oy) = self.get_board_offset();
        let board_pixels = size * 8.0;

        // Check "New Game" button click (bottom‑right)
        let btn_w = 120.0;
        let btn_h = 30.0;
        let btn_x = ox + board_pixels - btn_w - 10.0;
        let btn_y = oy + board_pixels + 10.0;
        if is_mouse_button_pressed(MouseButton::Left)
            && mx >= btn_x
            && mx <= btn_x + btn_w
            && my >= btn_y
            && my <= btn_y + btn_h
        {
            self.restart();
            return;
        }

        // If game over, ignore other input (except button above)
        if self.game_result.is_some() {
            return;
        }

        // Handle board clicks
        let file = ((mx - ox) / size) as i32;
        let rank = ((my - oy) / size) as i32;

        if file < 0 || file > 7 || rank < 0 || rank > 7 {
            if is_mouse_button_released(MouseButton::Left) {
                self.dragging_piece = None;
                self.selected_square = None;
            }
            return;
        }

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

        let sq = match get_square(logical_file as u32, logical_rank as u32) {
            Some(s) => s,
            None => return,
        };

        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(_piece) = self.board.piece_on(sq) {
                if self.board.color_on(sq) == Some(self.board.side_to_move()) {
                    self.selected_square = Some(sq);

                    let (screen_x, screen_y) = if self.perspective == ChessColor::White {
                        (
                            ox + logical_file as f32 * size,
                            oy + (7 - logical_rank) as f32 * size,
                        )
                    } else {
                        (
                            ox + (7 - logical_file) as f32 * size,
                            oy + logical_rank as f32 * size,
                        )
                    };
                    let offset_x = mx - screen_x;
                    let offset_y = my - screen_y;

                    self.dragging_piece = Some((sq, offset_x, offset_y));
                }
            }
        } else if is_mouse_button_released(MouseButton::Left) {
            if let Some(from_sq) = self.selected_square {
                if from_sq != sq {
                    // Check if this move is a pawn promotion
                    let is_pawn = self.board.piece_on(from_sq) == Some(ChessPiece::Pawn);
                    let promo_rank = if self.board.side_to_move() == ChessColor::White {
                        7
                    } else {
                        0
                    };
                    let is_promotion = is_pawn && logical_rank == promo_rank;

                    if is_promotion {
                        // Create the move with a placeholder promotion (e.g., Queen) – we'll use the config default
                        // But we store the move and set pending_promotion
                        let move_candidate =
                            ChessMove::new(from_sq, sq, Some(self.config.promotion_piece));
                        if self.board.legal(move_candidate) {
                            self.pending_promotion = Some(move_candidate);
                            self.selected_square = None;
                            self.dragging_piece = None;
                            return;
                        }
                    } else {
                        let chess_move = ChessMove::new(from_sq, sq, None);
                        if self.board.legal(chess_move) {
                            self.try_move(chess_move);
                        }
                    }
                }
                self.selected_square = None;
                self.dragging_piece = None;
            }
        }
    }

    // FIX: changed &self to &mut self
    fn handle_promotion_input(&mut self) {
        // Promotion pop‑up: click on one of the four piece icons
        let size = self.config.square_size;
        let (ox, oy) = self.get_board_offset();
        let board_pixels = size * 8.0;

        let choice_size = size * 0.8;
        let gap = size * 0.3;
        let total_width = choice_size * 4.0 + gap * 3.0;
        let start_x = ox + (board_pixels - total_width) / 2.0;
        let start_y = oy + (board_pixels - choice_size) / 2.0;

        let promotion_pieces = [
            ChessPiece::Queen,
            ChessPiece::Rook,
            ChessPiece::Bishop,
            ChessPiece::Knight,
        ];

        let (mx, my) = mouse_position();
        if is_mouse_button_pressed(MouseButton::Left) {
            for (i, &piece) in promotion_pieces.iter().enumerate() {
                let x = start_x + i as f32 * (choice_size + gap);
                let y = start_y;
                if mx >= x && mx <= x + choice_size && my >= y && my <= y + choice_size {
                    // User selected this piece
                    // FIX: removed 'mut' from move_candidate (not needed)
                    if let Some(move_candidate) = self.pending_promotion.take() {
                        let new_move = ChessMove::new(
                            move_candidate.get_source(),
                            move_candidate.get_dest(),
                            Some(piece),
                        );
                        self.try_move(new_move);
                    }
                    return;
                }
            }
        }
    }

    // ---------- Game logic ----------

    fn restart(&mut self) {
        self.board = Board::default();
        self.selected_square = None;
        self.dragging_piece = None;
        self.perspective = ChessColor::White;
        self.game_result = None;
        self.status_message.clear();
        self.last_move = None;
        self.pending_promotion = None;
        #[cfg(feature = "uci")]
        if let Some(ref mut engine) = self.uci_engine {
            engine.reset();
        }
    }

    /// Attempts to make a move. Returns `true` if legal and applied.
    pub fn try_move(&mut self, m: ChessMove) -> bool {
        if self.board.legal(m) {
            // Record last move
            self.last_move = Some((m.get_source(), m.get_dest()));
            self.board = self.board.make_move_new(m);
            if self.config.auto_flip_perspective {
                self.perspective = match self.perspective {
                    ChessColor::White => ChessColor::Black,
                    ChessColor::Black => ChessColor::White,
                };
            }
            self.game_result = check_game_result(&self.board);
            self.pending_promotion = None;
            true
        } else {
            false
        }
    }

    // ---------- Public API ----------

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn perspective(&self) -> ChessColor {
        self.perspective
    }

    pub fn fen(&self) -> String {
        self.board.to_string()
    }

    pub fn board_offset(&self) -> (f32, f32) {
        self.get_board_offset()
    }

    pub fn square_size(&self) -> f32 {
        self.config.square_size
    }

    pub fn set_fen(&mut self, fen: &str) -> Result<(), String> {
        Board::from_str(fen)
            .map(|b| {
                self.board = b;
                self.game_result = None;
                self.status_message.clear();
                self.selected_square = None;
                self.dragging_piece = None;
                self.last_move = None;
                self.pending_promotion = None;
                self.perspective = self.board.side_to_move();
            })
            .map_err(|e| e.to_string())
    }

    pub fn set_board(&mut self, board: Board) {
        self.board = board;
        self.game_result = None;
        self.status_message.clear();
        self.selected_square = None;
        self.dragging_piece = None;
        self.last_move = None;
        self.pending_promotion = None;
        self.perspective = self.board.side_to_move();
    }

    pub fn legal_moves(&self) -> Vec<ChessMove> {
        MoveGen::new_legal(&self.board).collect()
    }
}

// ---------- Helper functions ----------

/// Returns a contrasting color (black or white) based on the luminance of the given color.
fn contrast_color(color: Color) -> Color {
    let luminance = 0.299 * color.r + 0.587 * color.g + 0.114 * color.b;
    if luminance > 0.5 {
        BLACK
    } else {
        WHITE
    }
}

// -----------------------------------------------------------------------------
// UCI Engine integration (only when the feature is enabled)
// -----------------------------------------------------------------------------

#[cfg(feature = "uci")]
mod uci {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread;
    use std::time::{Duration, Instant};

    pub struct UciEngine {
        child: Child,
        stdin: std::process::ChildStdin,
        request_sender: Sender<String>,
        response_receiver: Receiver<ChessMove>,
        has_pending: bool,
        request_time: Option<Instant>,
        reader_thread: Option<thread::JoinHandle<()>>,
        move_timeout_ms: u64,
    }

    impl UciEngine {
        pub fn new(path: &str, move_time_ms: u64) -> Result<Self, String> {
            let mut child = Command::new(path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to spawn engine: {}", e))?;

            let mut stdin = child.stdin.take().expect("failed to open stdin");
            let stdout = child.stdout.take().expect("failed to open stdout");
            let mut reader = BufReader::new(stdout);

            stdin
                .write_all(b"uci\n")
                .and_then(|_| stdin.flush())
                .map_err(|e| format!("Failed to write to engine: {}", e))?;

            let mut line = String::new();
            let mut ready = false;
            while let Ok(n) = reader.read_line(&mut line) {
                if n == 0 {
                    break;
                }
                if line.trim() == "uciok" {
                    ready = true;
                    break;
                }
                line.clear();
            }
            if !ready {
                let _ = child.kill();
                return Err("Engine did not respond with 'uciok'".to_string());
            }

            stdin
                .write_all(b"ucinewgame\n")
                .and_then(|_| stdin.flush())
                .map_err(|e| format!("Failed to write 'ucinewgame': {}", e))?;

            let (req_tx, req_rx) = mpsc::channel();
            let (res_tx, res_rx) = mpsc::channel();

            let reader_thread = thread::spawn(move || {
                engine_thread_loop(child, stdin, reader, req_rx, res_tx, move_time_ms);
            });

            Ok(UciEngine {
                child,
                stdin,
                request_sender: req_tx,
                response_receiver: res_rx,
                has_pending: false,
                request_time: None,
                reader_thread: Some(reader_thread),
                move_timeout_ms: move_time_ms,
            })
        }

        pub fn request_move(&mut self, fen: String) {
            if !self.has_pending {
                let _ = self.request_sender.send(fen);
                self.has_pending = true;
                self.request_time = Some(Instant::now());
            }
        }

        pub fn try_take_move(&mut self) -> Option<ChessMove> {
            if self.has_pending {
                if let Some(t) = self.request_time {
                    let elapsed = t.elapsed().as_millis() as u64;
                    if elapsed > self.move_timeout_ms + 2000 {
                        eprintln!("Engine request timed out, clearing pending.");
                        self.has_pending = false;
                        self.request_time = None;
                        return None;
                    }
                }
            }

            if let Ok(m) = self.response_receiver.try_recv() {
                self.has_pending = false;
                self.request_time = None;
                Some(m)
            } else {
                None
            }
        }

        pub fn has_pending_move(&self) -> bool {
            self.has_pending
        }

        pub fn reset(&mut self) {
            self.has_pending = false;
            self.request_time = None;
            let _ = self.stdin.write_all(b"ucinewgame\n");
            let _ = self.stdin.flush();
            while let Ok(_) = self.response_receiver.try_recv() {}
        }
    }

    impl Drop for UciEngine {
        fn drop(&mut self) {
            let _ = self.stdin.write_all(b"quit\n");
            let _ = self.stdin.flush();
            let _ = self.child.kill();
            if let Some(handle) = self.reader_thread.take() {
                let _ = handle.join();
            }
        }
    }

    fn engine_thread_loop(
        mut child: Child,
        mut stdin: std::process::ChildStdin,
        mut reader: BufReader<std::process::ChildStdout>,
        request_rx: Receiver<String>,
        response_tx: Sender<ChessMove>,
        move_time_ms: u64,
    ) {
        let (line_tx, line_rx) = mpsc::channel();
        let reader_handle = thread::spawn(move || {
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line) {
                if n == 0 {
                    break;
                }
                if let Some(bestmove) = parse_bestmove(&line) {
                    let _ = line_tx.send(bestmove);
                }
                line.clear();
            }
        });

        loop {
            let fen = match request_rx.recv() {
                Ok(f) => f,
                Err(_) => break,
            };

            let cmd = format!("position fen {}\ngo movetime {}\n", fen, move_time_ms);
            if let Err(e) = stdin.write_all(cmd.as_bytes()).and_then(|_| stdin.flush()) {
                eprintln!("Error writing to engine: {}", e);
                break;
            }

            let timeout = Duration::from_millis(move_time_ms + 500);
            let start = Instant::now();
            let mut move_received = None;
            while start.elapsed() < timeout {
                if let Ok(m) = line_rx.try_recv() {
                    move_received = Some(m);
                    break;
                }
                thread::sleep(Duration::from_millis(10));
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
            }

            if let Some(m) = move_received {
                let _ = response_tx.send(m);
            } else {
                eprintln!("Engine did not respond in time, skipping this move.");
            }
        }

        let _ = child.kill();
        let _ = reader_handle.join();
    }

    fn parse_bestmove(line: &str) -> Option<ChessMove> {
        let trimmed = line.trim();
        if !trimmed.starts_with("bestmove") {
            return None;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }
        let move_str = parts[1];
        if move_str == "(none)" {
            return None;
        }
        let bytes = move_str.as_bytes();
        if bytes.len() < 4 {
            return None;
        }
        let from_file = (bytes[0] - b'a') as u32;
        let from_rank = (bytes[1] - b'1') as u32;
        let to_file = (bytes[2] - b'a') as u32;
        let to_rank = (bytes[3] - b'1') as u32;
        let from_sq = super::get_square(from_file, from_rank)?;
        let to_sq = super::get_square(to_file, to_rank)?;
        let promotion = if bytes.len() >= 5 {
            match bytes[4] {
                b'q' => Some(ChessPiece::Queen),
                b'r' => Some(ChessPiece::Rook),
                b'b' => Some(ChessPiece::Bishop),
                b'n' => Some(ChessPiece::Knight),
                _ => None,
            }
        } else {
            None
        };
        Some(ChessMove::new(from_sq, to_sq, promotion))
    }
}

#[cfg(feature = "uci")]
pub use uci::UciEngine;
