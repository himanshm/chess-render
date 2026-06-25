//! # chess-render
//!
//! A configurable, embeddable chess GUI built with [Macroquad](https://macroquad.rs/).
//!
//! This crate provides a ready‑to‑use chess board widget that handles:
//!
//! - Rendering the board and pieces from a texture atlas (with a built‑in default texture).
//! - Valid move generation and enforcement via the `[chess](https://crates.io/crates/chess)` crate.
//! - Two‑player local play with automatic board flip after each move (optional).
//! - Optional UCI engine integration using the `[uci](https://crates.io/crates/uci)` crate.
//! - Endgame detection (checkmate, stalemate) and a restart button.
//! - Automatic centering of the board in the window (configurable).
//! - Usability features: turn indicator, last‑move highlighting, promotion pop‑up, and a "New Game" button.

use chess::{
    Board, BoardStatus, ChessMove, Color as ChessColor, File, Piece as ChessPiece, Rank, Square,
};
use macroquad::prelude::*;

// External crates
use log::{error, warn};
use std::str::FromStr;
use thiserror::Error;
#[cfg(feature = "uci")]
use uci::Uci;

// Use the egui version re-exported by egui_macroquad to avoid version conflicts.
use egui_macroquad::egui::{Align2, Window};

// Enum utilities
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

// Embed the default piece texture directly into the binary.
const DEFAULT_PIECES_PNG: &[u8] = include_bytes!("assets/pieces.png");

/// Custom error type for the library.
#[derive(Error, Debug)]
pub enum ChessError {
    #[error("Failed to load piece texture: {0}")]
    TextureLoad(String),
    #[error("Invalid FEN: {0}")]
    InvalidFen(String),
    #[error("UCI engine error: {0}")]
    UciError(String),
    #[error("Other error: {0}")]
    Other(String),
}

/// The result of a finished chess game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    WhiteWins,
    BlackWins,
    Draw,
}

/// Helper to convert 0-7 file/rank indices to a [`chess::Square`].
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
    pub light_square_color: Color,
    pub dark_square_color: Color,
    pub square_size: f32,
    pub board_offset: (f32, f32),
    pub center_board: bool,
    pub auto_flip_perspective: bool,
    pub promotion_piece: ChessPiece,
    pub piece_texture_path: Option<String>,
    pub show_coordinates: bool,
    #[cfg(feature = "uci")]
    pub uci_engine_path: Option<String>,
    #[cfg(feature = "uci")]
    pub uci_plays_as: ChessColor,
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
            show_coordinates: true,
            #[cfg(feature = "uci")]
            uci_engine_path: None,
            #[cfg(feature = "uci")]
            uci_plays_as: ChessColor::Black,
            #[cfg(feature = "uci")]
            uci_move_time_ms: 1000,
        }
    }
}

/// Internal piece representation for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter)]
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

/// Helper for board geometry.
struct BoardGeometry {
    offset_x: f32,
    offset_y: f32,
    square_size: f32,
    perspective: ChessColor,
}

impl BoardGeometry {
    fn new(offset: (f32, f32), square_size: f32, perspective: ChessColor) -> Self {
        Self {
            offset_x: offset.0,
            offset_y: offset.1,
            square_size,
            perspective,
        }
    }

    fn square_to_screen(&self, file: u32, rank: u32) -> (f32, f32) {
        let (f, r) = if self.perspective == ChessColor::White {
            (file, 7 - rank)
        } else {
            (7 - file, rank)
        };
        (
            self.offset_x + f as f32 * self.square_size,
            self.offset_y + r as f32 * self.square_size,
        )
    }

    fn screen_to_square(&self, x: f32, y: f32) -> Option<(u32, u32)> {
        let f = (x - self.offset_x) / self.square_size;
        let r = (y - self.offset_y) / self.square_size;
        if f >= 0.0 && f < 8.0 && r >= 0.0 && r < 8.0 {
            let file = f.floor() as u32;
            let rank = r.floor() as u32;
            let (logical_file, logical_rank) = if self.perspective == ChessColor::White {
                (file, 7 - rank)
            } else {
                (7 - file, rank)
            };
            Some((logical_file, logical_rank))
        } else {
            None
        }
    }

    fn board_pixels(&self) -> f32 {
        self.square_size * 8.0
    }
}

/// Main chess GUI struct.
pub struct ChessGui {
    board: Board,
    config: ChessConfig,
    pieces_texture: Option<Texture2D>,
    selected_square: Option<Square>,
    dragging_piece: Option<(Square, f32, f32)>,
    perspective: ChessColor,
    game_result: Option<GameResult>,
    status_message: String,
    piece_rects: [Rect; 12],
    last_move: Option<(Square, Square)>,
    pending_promotion: Option<ChessMove>,
    error: Option<String>,
    #[cfg(feature = "uci")]
    uci_engine: Option<UciEngineWrapper>,
}

#[cfg(feature = "uci")]
struct UciEngineWrapper {
    engine: Uci,
    move_time_ms: u64,
}

impl ChessGui {
    pub fn new(config: ChessConfig) -> Self {
        // Build piece rects using strum iteration
        let mut rects = [Rect::new(0.0, 0.0, 64.0, 64.0); 12];
        for (i, variant) in RenderPiece::iter().enumerate() {
            let (x, y) = variant.tex_coords();
            rects[i] = Rect::new(x, y, 64.0, 64.0);
        }

        Self {
            board: Board::default(),
            config,
            pieces_texture: None,
            selected_square: None,
            dragging_piece: None,
            perspective: ChessColor::White,
            game_result: None,
            status_message: String::new(),
            piece_rects: rects,
            last_move: None,
            pending_promotion: None,
            error: None,
            #[cfg(feature = "uci")]
            uci_engine: None,
        }
    }

    fn get_board_offset(&self) -> (f32, f32) {
        if self.config.center_board {
            let board_pixels = self.config.square_size.round() * 8.0;
            let ox = (screen_width() - board_pixels) / 2.0;
            let oy = (screen_height() - board_pixels) / 2.0;
            (ox, oy)
        } else {
            self.config.board_offset
        }
    }

    fn geometry(&self) -> BoardGeometry {
        let offset = self.get_board_offset();
        BoardGeometry::new(offset, self.config.square_size.round(), self.perspective)
    }

    pub async fn load_pieces(&mut self) -> Result<(), ChessError> {
        let image_data = if let Some(ref path) = self.config.piece_texture_path {
            match load_file(path).await {
                Ok(data) => data,
                Err(e) => {
                    let msg = format!("Failed to load custom texture from {}: {}", path, e);
                    error!("{}", msg);
                    self.error = Some(msg.clone());
                    return Err(ChessError::TextureLoad(msg));
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
            warn!(
                "Invalid piece texture ({}x{}), reloading default.",
                tex.width(),
                tex.height()
            );
            let default_tex = Texture2D::from_file_with_format(DEFAULT_PIECES_PNG, None);
            default_tex.set_filter(FilterMode::Nearest);
            tex = default_tex;
            if tex.width() == 0.0 || tex.height() == 0.0 {
                let msg = "Default texture also failed to load!";
                error!("{}", msg);
                self.error = Some(msg.to_string());
                return Err(ChessError::TextureLoad(msg.to_string()));
            }
        }

        self.pieces_texture = Some(tex);
        self.error = None;

        #[cfg(feature = "uci")]
        if let Some(ref path) = self.config.uci_engine_path {
            match self.init_uci_engine(path) {
                Ok(wrapper) => {
                    self.uci_engine = Some(wrapper);
                    log::info!("UCI engine started: {}", path);
                }
                Err(e) => {
                    let msg = format!("Failed to start UCI engine: {}", e);
                    error!("{}", msg);
                    self.error = Some(msg);
                }
            }
        }

        Ok(())
    }

    #[cfg(feature = "uci")]
    fn init_uci_engine(&self, path: &str) -> Result<UciEngineWrapper, ChessError> {
        let mut engine = Uci::new(path).map_err(|e| ChessError::UciError(e.to_string()))?;
        engine
            .start()
            .map_err(|e| ChessError::UciError(e.to_string()))?;
        engine
            .send("ucinewgame")
            .map_err(|e| ChessError::UciError(e.to_string()))?;

        Ok(UciEngineWrapper {
            engine,
            move_time_ms: self.config.uci_move_time_ms,
        })
    }

    pub async fn update(&mut self) {
        // Recompute result if not pending promotion and no result.
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

        let mut wants_pointer_input = false;
        let mut wants_keyboard_input = false;

        // Build egui UI using egui_macroquad's closure API.
        // The `ctx` here is `egui_macroquad::egui::Context` (egui 0.31),
        // which matches the types used in `build_ui`.
        egui_macroquad::ui(|ctx| {
            self.build_ui(ctx);
            wants_pointer_input = ctx.wants_pointer_input();
            wants_keyboard_input = ctx.wants_keyboard_input();
        });

        // Render the egui UI (takes no arguments).
        egui_macroquad::draw();

        // Handle input only if egui doesn't capture it
        if !wants_pointer_input
            && !wants_keyboard_input
            && self.game_result.is_none()
            && self.pending_promotion.is_none()
        {
            self.handle_input();
        }

        // Keyboard shortcuts (still active)
        if is_key_pressed(KeyCode::R) {
            self.restart();
        }
        if is_key_pressed(KeyCode::F) {
            self.perspective = match self.perspective {
                ChessColor::White => ChessColor::Black,
                ChessColor::Black => ChessColor::White,
            };
        }

        // Engine integration (only if no UI is blocking)
        if !wants_pointer_input
            && !wants_keyboard_input
            && self.game_result.is_none()
            && self.pending_promotion.is_none()
        {
            #[cfg(feature = "uci")]
            if let Some(ref mut uci_wrapper) = self.uci_engine {
                let engine = &mut uci_wrapper.engine;
                if self.board.side_to_move() == self.config.uci_plays_as
                    && self.dragging_piece.is_none()
                    && !engine.is_searching()
                {
                    let fen = self.board.to_string();
                    if let Err(e) = engine.send(&format!("position fen {}", fen)) {
                        error!("Failed to send position to engine: {}", e);
                    } else if let Err(e) =
                        engine.send(&format!("go movetime {}", uci_wrapper.move_time_ms))
                    {
                        error!("Failed to send go command: {}", e);
                    }
                }

                if let Ok(Some(bestmove)) = engine.bestmove() {
                    if let Some(m) = parse_uci_bestmove(&bestmove) {
                        if self.try_move(m) {
                            log::info!("Engine moved: {}", bestmove);
                        } else {
                            warn!("Engine played illegal move: {}", bestmove);
                        }
                    }
                }
            }
        }
    }

    /// Build all egui UI elements.
    ///
    /// Note: `ctx` is `egui_macroquad::egui::Context` (egui 0.31), NOT the
    /// standalone `egui::Context` (0.34). All egui types used here must come
    /// from `egui_macroquad::egui`.
    fn build_ui(&mut self, ctx: &egui_macroquad::egui::Context) {
        // Promotion dialog
        if self.pending_promotion.is_some() {
            Window::new("Promotion")
                .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Choose promotion piece:");
                    ui.horizontal(|ui| {
                        let pieces = [
                            ChessPiece::Queen,
                            ChessPiece::Rook,
                            ChessPiece::Bishop,
                            ChessPiece::Knight,
                        ];
                        for &piece in &pieces {
                            let label = match piece {
                                ChessPiece::Queen => "♛",
                                ChessPiece::Rook => "♜",
                                ChessPiece::Bishop => "♝",
                                ChessPiece::Knight => "♞",
                                _ => "?",
                            };
                            if ui.button(label).clicked() {
                                if let Some(move_candidate) = self.pending_promotion.take() {
                                    let new_move = ChessMove::new(
                                        move_candidate.get_source(),
                                        move_candidate.get_dest(),
                                        Some(piece),
                                    );
                                    self.try_move(new_move);
                                }
                            }
                        }
                    });
                });
            return;
        }

        // Main UI: turn indicator + control buttons
        Window::new("Controls")
            .anchor(Align2::RIGHT_TOP, (-10.0, 10.0))
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                let label = if let Some(result) = self.game_result {
                    match result {
                        GameResult::WhiteWins => "White Wins!",
                        GameResult::BlackWins => "Black Wins!",
                        GameResult::Draw => "Draw",
                    }
                } else if self.board.side_to_move() == ChessColor::White {
                    "White to move"
                } else {
                    "Black to move"
                };
                ui.label(label);

                ui.separator();

                if ui.button("New Game").clicked() {
                    self.restart();
                }
                if ui.button("Flip Board").clicked() {
                    self.perspective = match self.perspective {
                        ChessColor::White => ChessColor::Black,
                        ChessColor::Black => ChessColor::White,
                    };
                }

                if self.game_result.is_some() {
                    ui.label("Press R or click New Game");
                }
            });
    }

    // ---------- Board rendering ----------
    fn draw_board(&self) {
        let Some(texture) = &self.pieces_texture else {
            draw_text("Load pieces texture first!", 100.0, 256.0, 20.0, RED);
            if let Some(err) = &self.error {
                draw_text(&format!("Error: {}", err), 100.0, 300.0, 16.0, RED);
            }
            return;
        };

        let geom = self.geometry();

        self.draw_squares_and_highlights(&geom);
        self.draw_pieces(texture, &geom);
        if self.config.show_coordinates {
            self.draw_coordinates(&geom);
        }
        self.draw_dragged_piece(texture, &geom);
    }

    fn draw_squares_and_highlights(&self, geom: &BoardGeometry) {
        let size = geom.square_size;
        for rank in 0..8 {
            for file in 0..8 {
                let is_light = (file + rank) % 2 == 0;
                let color = if is_light {
                    self.config.light_square_color
                } else {
                    self.config.dark_square_color
                };

                let (screen_x, screen_y) = geom.square_to_screen(file, rank);
                draw_rectangle(screen_x, screen_y, size, size, color);

                let sq = match get_square(file, rank) {
                    Some(s) => s,
                    None => continue,
                };

                if let Some((from, to)) = self.last_move {
                    if sq == from || sq == to {
                        draw_rectangle(
                            screen_x,
                            screen_y,
                            size,
                            size,
                            Color::new(1.0, 0.8, 0.0, 0.3),
                        );
                    }
                }

                if Some(sq) == self.selected_square {
                    draw_rectangle(
                        screen_x,
                        screen_y,
                        size,
                        size,
                        Color::new(1.0, 1.0, 0.0, 0.3),
                    );
                }

                if let Some(sel_sq) = self.selected_square {
                    if self.board.piece_on(sel_sq).is_some()
                        && self.board.color_on(sel_sq) == Some(self.board.side_to_move())
                    {
                        let legal_targets = self.legal_targets_for(sel_sq);
                        if legal_targets.contains(&sq) {
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
        }
    }

    fn draw_pieces(&self, texture: &Texture2D, geom: &BoardGeometry) {
        let size = geom.square_size;
        for rank in 0..8 {
            for file in 0..8 {
                let sq = match get_square(file, rank) {
                    Some(s) => s,
                    None => continue,
                };
                if let Some(piece) = self.board.piece_on(sq) {
                    if let Some((drag_sq, _, _)) = self.dragging_piece {
                        if drag_sq == sq {
                            continue;
                        }
                    }
                    let color = self.board.color_on(sq).unwrap();
                    let render_piece = RenderPiece::from_chess(piece, color);
                    let rect = self.piece_rects[render_piece as usize];
                    let (screen_x, screen_y) = geom.square_to_screen(file, rank);
                    draw_texture_ex(
                        texture,
                        screen_x,
                        screen_y,
                        WHITE,
                        DrawTextureParams {
                            source: Some(rect),
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

    fn draw_dragged_piece(&self, texture: &Texture2D, geom: &BoardGeometry) {
        if let Some((sq, offset_x, offset_y)) = self.dragging_piece {
            if let Some(piece) = self.board.piece_on(sq) {
                let color = self.board.color_on(sq).unwrap();
                let render_piece = RenderPiece::from_chess(piece, color);
                let rect = self.piece_rects[render_piece as usize];
                let (mx, my) = mouse_position();
                let size = geom.square_size;
                draw_texture_ex(
                    texture,
                    mx - offset_x,
                    my - offset_y,
                    WHITE,
                    DrawTextureParams {
                        source: Some(rect),
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

    fn draw_coordinates(&self, geom: &BoardGeometry) {
        let size = geom.square_size;
        let font_size = (size / 4.0) as u16;
        for i in 0..8 {
            let file_label = (b'a' + i as u8) as char;
            let rank_label = (b'1' + i as u8) as char;

            let file_sq_color = if (i + 0) % 2 == 0 {
                self.config.light_square_color
            } else {
                self.config.dark_square_color
            };
            let file_text_color = contrast_color(file_sq_color);

            let (x_bottom, _) = geom.square_to_screen(i, 0);
            let x = x_bottom + size / 2.0;
            let y_bottom = geom.offset_y + geom.board_pixels() + 5.0;
            draw_text(
                &file_label.to_string(),
                x - measure_text(&file_label.to_string(), None, font_size, 1.0).width / 2.0,
                y_bottom + font_size as f32,
                font_size as f32,
                file_text_color,
            );

            let rank_sq_color = if (0 + i) % 2 == 0 {
                self.config.light_square_color
            } else {
                self.config.dark_square_color
            };
            let rank_text_color = contrast_color(rank_sq_color);

            let (_, y_rank) = geom.square_to_screen(0, i);
            let y = y_rank + size / 2.0;
            let x_left = geom.offset_x - 20.0;
            draw_text(
                &rank_label.to_string(),
                x_left,
                y + font_size as f32 / 2.0,
                font_size as f32,
                rank_text_color,
            );
        }
    }

    // ---------- Input handling ----------
    fn handle_input(&mut self) {
        let (mx, my) = mouse_position();
        let geom = self.geometry();

        if self.game_result.is_some() {
            return;
        }

        let logical_sq = if let Some((file, rank)) = geom.screen_to_square(mx, my) {
            get_square(file, rank)
        } else {
            None
        };

        if logical_sq.is_none() {
            if is_mouse_button_released(MouseButton::Left) {
                self.dragging_piece = None;
                self.selected_square = None;
            }
            return;
        }

        let sq = logical_sq.unwrap();

        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(_piece) = self.board.piece_on(sq) {
                if self.board.color_on(sq) == Some(self.board.side_to_move()) {
                    self.selected_square = Some(sq);
                    let (screen_x, screen_y) = geom.square_to_screen(
                        sq.get_file().to_index() as u32,
                        sq.get_rank().to_index() as u32,
                    );
                    let offset_x = mx - screen_x;
                    let offset_y = my - screen_y;
                    self.dragging_piece = Some((sq, offset_x, offset_y));
                }
            }
        } else if is_mouse_button_released(MouseButton::Left) {
            if let Some(from_sq) = self.selected_square {
                if from_sq != sq {
                    let is_pawn = self.board.piece_on(from_sq) == Some(ChessPiece::Pawn);
                    let promo_rank = if self.board.side_to_move() == ChessColor::White {
                        7
                    } else {
                        0
                    };
                    let is_promotion = is_pawn && sq.get_rank().to_index() as u32 == promo_rank;

                    if is_promotion {
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

        if is_mouse_button_pressed(MouseButton::Right) {
            self.selected_square = None;
            self.dragging_piece = None;
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
        if let Some(ref mut uci_wrapper) = self.uci_engine {
            let _ = uci_wrapper.engine.send("ucinewgame");
        }
    }

    pub fn try_move(&mut self, m: ChessMove) -> bool {
        if self.board.legal(m) {
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

    fn legal_targets_for(&self, source: Square) -> Vec<Square> {
        chess::MoveGen::new_legal(&self.board)
            .filter(|m| m.get_source() == source)
            .map(|m| m.get_dest())
            .collect()
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

    pub fn set_fen(&mut self, fen: &str) -> Result<(), ChessError> {
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
            .map_err(|e| ChessError::InvalidFen(e.to_string()))
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
        chess::MoveGen::new_legal(&self.board).collect()
    }
}

// ---------- Helper functions ----------
fn contrast_color(color: Color) -> Color {
    let luminance = 0.299 * color.r + 0.587 * color.g + 0.114 * color.b;
    if luminance > 0.5 {
        BLACK
    } else {
        WHITE
    }
}

#[cfg(feature = "uci")]
fn parse_uci_bestmove(bestmove: &str) -> Option<ChessMove> {
    let parts: Vec<&str> = bestmove.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    let move_str = parts[0];
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
    let from_sq = get_square(from_file, from_rank)?;
    let to_sq = get_square(to_file, to_rank)?;
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
