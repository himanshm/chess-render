//! # chess-render
//!
//! A configurable, embeddable chess GUI built with [Macroquad](https://macroquad.rs/).
//!
//! This crate provides a ready-to-use chess board widget that handles:
//!
//! - Rendering the board and pieces from a texture atlas (with a built-in default texture).
//! - Valid move generation and enforcement via the [`chess`](https://crates.io/crates/chess) crate.
//! - Two-player local play with automatic board flip after each move (optional).
//! - Optional UCI engine integration using the [`uci`](https://crates.io/crates/uci) crate
//!   (enable the `uci` feature).
//! - Endgame detection (checkmate, stalemate, draw by insufficient material) and a restart
//!   button.
//! - Automatic centering of the board in the window (configurable).
//! - Usability features: turn indicator, last-move highlighting, king-in-check highlighting,
//!   legal-move dots, promotion pop-up, coordinate labels, and a "New Game" button.
//!
//! # Quick Start
//!
//! ```ignore
//! use chess_render::{ChessConfig, ChessGui};
//!
//! fn main() {
//!     let config = ChessConfig::builder().square_size(80.0).build();
//!     let mut gui = ChessGui::new(config);
//!     // … macroquad loop …
//! }
//! ```
//!
//! # Features
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `uci`   | Enables UCI engine integration (adds the `uci` crate dependency). |

// ──────────────────────────────────────────────────────────────────────────────
// Imports
// ──────────────────────────────────────────────────────────────────────────────

use chess::{
    BitBoard, Board, BoardStatus, ChessMove, Color as ChessColor, Piece as ChessPiece, Rank,
    Square, ALL_FILES, ALL_RANKS,
};
use macroquad::prelude::*;

#[cfg(feature = "uci")]
use log::info;
use log::{error, warn};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[cfg(feature = "uci")]
use uci::Uci;

// Use the egui version re-exported by egui_macroquad to avoid version conflicts.
use egui_macroquad::egui::{Align2, Window};

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/// Number of squares per rank/file.
const BOARD_SIZE: u32 = 8;

/// Default tile size in the texture atlas (pixels).
const ATLAS_TILE: f32 = 64.0;

/// Expected atlas width (6 tiles × 64 px).
const ATLAS_WIDTH: u32 = 384;

/// Expected atlas height (2 tiles × 64 px).
const ATLAS_HEIGHT: u32 = 128;

/// Highlight alpha for last-move / selection overlays.
const HIGHLIGHT_ALPHA: f32 = 0.35;

/// Alpha for legal-move dot indicators.
const DOT_ALPHA: f32 = 0.40;

/// Dot radius relative to square size (fraction).
const DOT_RADIUS_FRAC: f32 = 1.0 / 6.0;

/// Capture-ring radius relative to square size (fraction).
const CAPTURE_RING_FRAC: f32 = 1.0 / 3.0;

/// Background colour (light steel-blue).
const BG_COLOR: Color = Color::new(180.0 / 255.0, 220.0 / 255.0, 255.0 / 255.0, 1.0);

// Embed the default piece texture directly into the binary.
const DEFAULT_PIECES_PNG: &[u8] = include_bytes!("assets/pieces.png");

// ──────────────────────────────────────────────────────────────────────────────
// Error types
// ──────────────────────────────────────────────────────────────────────────────

/// Errors that can occur when using the chess GUI.
#[derive(Error, Debug)]
pub enum ChessError {
    /// Texture loading failed (custom or default).
    #[error("Failed to load piece texture: {0}")]
    TextureLoad(String),

    /// The provided FEN string could not be parsed.
    #[error("Invalid FEN: {0}")]
    InvalidFen(String),

    /// A UCI engine operation failed.
    #[error("UCI engine error: {0}")]
    UciError(String),
}

// ──────────────────────────────────────────────────────────────────────────────
// Game result
// ──────────────────────────────────────────────────────────────────────────────

/// The result of a finished chess game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    /// White delivered checkmate.
    WhiteWins,
    /// Black delivered checkmate.
    BlackWins,
    /// The game ended in a draw (stalemate, insufficient material, etc.).
    Draw,
}

impl fmt::Display for GameResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WhiteWins => write!(f, "White Wins!"),
            Self::BlackWins => write!(f, "Black Wins!"),
            Self::Draw => write!(f, "Game Drawn"),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Square helper
// ──────────────────────────────────────────────────────────────────────────────

/// Convert 0-7 file/rank indices to a [`chess::Square`].
///
/// Returns `None` if either index is out of range.
#[inline]
pub(crate) fn get_square(file: u32, rank: u32) -> Option<Square> {
    if file >= BOARD_SIZE || rank >= BOARD_SIZE {
        return None;
    }
    // SAFETY: the bounds check above guarantees `from_index` will succeed.
    let f = ALL_FILES[file as usize];
    let r = ALL_RANKS[rank as usize];
    Some(Square::make_square(r, f))
}

// ──────────────────────────────────────────────────────────────────────────────
// Game-result detection
// ──────────────────────────────────────────────────────────────────────────────

/// Check whether the position has ended and, if so, determine the result.
///
/// In addition to checkmate and stalemate, this also treats positions with
/// insufficient material as draws.
fn check_game_result(board: &Board) -> Option<GameResult> {
    match board.status() {
        BoardStatus::Checkmate => {
            // The side to move is in checkmate → the *other* side wins.
            let winner = match board.side_to_move() {
                ChessColor::White => GameResult::BlackWins,
                ChessColor::Black => GameResult::WhiteWins,
            };
            Some(winner)
        }
        BoardStatus::Stalemate => Some(GameResult::Draw),
        BoardStatus::Ongoing => None,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Configuration
// ──────────────────────────────────────────────────────────────────────────────

/// Configuration for the chess GUI appearance and behaviour.
///
/// Use [`ChessConfig::builder`] for an ergonomic construction API, or create via
/// `ChessConfig::default()` and modify fields directly.
#[derive(Debug, Clone)]
pub struct ChessConfig {
    /// Colour of light squares.
    pub light_square_color: Color,
    /// Colour of dark squares.
    pub dark_square_color: Color,
    /// Pixel size of each board square (must be > 0).
    pub square_size: f32,
    /// Manual board offset `(x, y)` from top-left (ignored when `center_board` is true).
    pub board_offset: (f32, f32),
    /// Automatically centre the board in the window.
    pub center_board: bool,
    /// Automatically flip the board perspective after each move.
    pub auto_flip_perspective: bool,
    /// Default promotion piece when the user does not explicitly choose.
    pub promotion_piece: ChessPiece,
    /// Optional path to a custom piece texture (PNG). If `None`, the built-in
    /// default texture is used.
    pub piece_texture_path: Option<String>,
    /// Show algebraic coordinate labels on the board edges.
    pub show_coordinates: bool,
    /// Show legal-move dot indicators on the board.
    pub show_legal_moves: bool,
    /// Highlight the king square when it is in check.
    pub show_check_highlight: bool,
    #[cfg(feature = "uci")]
    /// Path to a UCI-compatible chess engine executable.
    pub uci_engine_path: Option<String>,
    #[cfg(feature = "uci")]
    /// Which colour the UCI engine should play as.
    pub uci_plays_as: ChessColor,
    #[cfg(feature = "uci")]
    /// Time in milliseconds the engine should spend per move.
    pub uci_move_time_ms: u64,
}

/// Ergonomic builder for [`ChessConfig`].
///
/// # Example
///
/// ```ignore
/// let config = ChessConfig::builder()
///     .square_size(80.0)
///     .auto_flip_perspective(false)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ChessConfigBuilder {
    inner: ChessConfig,
}

impl ChessConfigBuilder {
    /// Set the light-square colour.
    pub fn light_square_color(mut self, c: Color) -> Self {
        self.inner.light_square_color = c;
        self
    }

    /// Set the dark-square colour.
    pub fn dark_square_color(mut self, c: Color) -> Self {
        self.inner.dark_square_color = c;
        self
    }

    /// Set the pixel size of each square. Clamped to a minimum of 16.0.
    pub fn square_size(mut self, s: f32) -> Self {
        self.inner.square_size = s.max(16.0);
        self
    }

    /// Set a manual board offset `(x, y)`.
    pub fn board_offset(mut self, x: f32, y: f32) -> Self {
        self.inner.board_offset = (x, y);
        self
    }

    /// Enable or disable automatic board centering (default: `true`).
    pub fn center_board(mut self, yes: bool) -> Self {
        self.inner.center_board = yes;
        self
    }

    /// Enable or disable automatic board flipping after each move (default: `true`).
    pub fn auto_flip_perspective(mut self, yes: bool) -> Self {
        self.inner.auto_flip_perspective = yes;
        self
    }

    /// Set the default promotion piece.
    pub fn promotion_piece(mut self, p: ChessPiece) -> Self {
        self.inner.promotion_piece = p;
        self
    }

    /// Set a custom piece texture path.
    pub fn piece_texture_path(mut self, path: impl Into<String>) -> Self {
        self.inner.piece_texture_path = Some(path.into());
        self
    }

    /// Enable or disable coordinate labels (default: `true`).
    pub fn show_coordinates(mut self, yes: bool) -> Self {
        self.inner.show_coordinates = yes;
        self
    }

    /// Enable or disable legal-move dot indicators (default: `true`).
    pub fn show_legal_moves(mut self, yes: bool) -> Self {
        self.inner.show_legal_moves = yes;
        self
    }

    /// Enable or disable king-in-check highlighting (default: `true`).
    pub fn show_check_highlight(mut self, yes: bool) -> Self {
        self.inner.show_check_highlight = yes;
        self
    }

    /// Set the UCI engine executable path (requires `uci` feature).
    #[cfg(feature = "uci")]
    pub fn uci_engine_path(mut self, path: impl Into<String>) -> Self {
        self.inner.uci_engine_path = Some(path.into());
        self
    }

    /// Set the colour the UCI engine should play as (requires `uci` feature).
    #[cfg(feature = "uci")]
    pub fn uci_plays_as(mut self, c: ChessColor) -> Self {
        self.inner.uci_plays_as = c;
        self
    }

    /// Set the UCI engine thinking time in milliseconds (requires `uci` feature).
    #[cfg(feature = "uci")]
    pub fn uci_move_time_ms(mut self, ms: u64) -> Self {
        self.inner.uci_move_time_ms = ms;
        self
    }

    /// Consume the builder and produce a validated [`ChessConfig`].
    pub fn build(mut self) -> ChessConfig {
        self.inner.validate();
        self.inner
    }
}

impl Default for ChessConfig {
    fn default() -> Self {
        Self {
            light_square_color: Color::new(255.0 / 255.0, 253.0 / 255.0, 208.0 / 255.0, 1.0),
            dark_square_color: GRAY,
            square_size: 64.0,
            board_offset: (0.0, 0.0),
            center_board: true,
            auto_flip_perspective: true,
            promotion_piece: ChessPiece::Queen,
            piece_texture_path: None,
            show_coordinates: true,
            show_legal_moves: true,
            show_check_highlight: true,
            #[cfg(feature = "uci")]
            uci_engine_path: None,
            #[cfg(feature = "uci")]
            uci_plays_as: ChessColor::Black,
            #[cfg(feature = "uci")]
            uci_move_time_ms: 1000,
        }
    }
}

impl ChessConfig {
    /// Return a builder initialised with default values.
    pub fn builder() -> ChessConfigBuilder {
        ChessConfigBuilder {
            inner: Self::default(),
        }
    }

    /// Validate configuration values, clamping or warning as needed.
    fn validate(&mut self) {
        if self.square_size <= 0.0 {
            warn!("square_size must be positive; clamping to 64.0");
            self.square_size = 64.0;
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal render-piece representation
// ──────────────────────────────────────────────────────────────────────────────

/// Internal piece representation for rendering.
///
/// Enum variants are ordered identically to the texture atlas layout:
/// row 0 (y = 0) = white pieces (King … Pawn), row 1 (y = 64) = black pieces.
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

/// All render pieces in atlas order.
const ALL_RENDER_PIECES: [RenderPiece; 12] = [
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
];

impl RenderPiece {
    fn from_chess(piece: ChessPiece, color: ChessColor) -> Self {
        match (color, piece) {
            (ChessColor::White, ChessPiece::King) => Self::WhiteKing,
            (ChessColor::White, ChessPiece::Queen) => Self::WhiteQueen,
            (ChessColor::White, ChessPiece::Bishop) => Self::WhiteBishop,
            (ChessColor::White, ChessPiece::Knight) => Self::WhiteKnight,
            (ChessColor::White, ChessPiece::Rook) => Self::WhiteRook,
            (ChessColor::White, ChessPiece::Pawn) => Self::WhitePawn,
            (ChessColor::Black, ChessPiece::King) => Self::BlackKing,
            (ChessColor::Black, ChessPiece::Queen) => Self::BlackQueen,
            (ChessColor::Black, ChessPiece::Bishop) => Self::BlackBishop,
            (ChessColor::Black, ChessPiece::Knight) => Self::BlackKnight,
            (ChessColor::Black, ChessPiece::Rook) => Self::BlackRook,
            (ChessColor::Black, ChessPiece::Pawn) => Self::BlackPawn,
        }
    }

    /// Texture-atlas coordinates (top-left corner in pixels).
    fn tex_coords(&self) -> (f32, f32) {
        let idx = *self as usize;
        let col = (idx % 6) as f32;
        let row = (idx / 6) as f32;
        (col * ATLAS_TILE, row * ATLAS_TILE)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Board geometry helper
// ──────────────────────────────────────────────────────────────────────────────

/// Encapsulates board-to-screen coordinate mapping for a given perspective.
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

    /// Map logical `(file, rank)` to screen `(x, y)`.
    #[inline]
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

    /// Map screen `(x, y)` back to logical `(file, rank)`, or `None` if outside the board.
    #[inline]
    fn screen_to_square(&self, x: f32, y: f32) -> Option<(u32, u32)> {
        let f = (x - self.offset_x) / self.square_size;
        let r = (y - self.offset_y) / self.square_size;
        if !(0.0..BOARD_SIZE as f32).contains(&f) || !(0.0..BOARD_SIZE as f32).contains(&r) {
            return None;
        }
        let file = f.floor() as u32;
        let rank = r.floor() as u32;
        let (logical_file, logical_rank) = if self.perspective == ChessColor::White {
            (file, 7 - rank)
        } else {
            (7 - file, rank)
        };
        Some((logical_file, logical_rank))
    }

    #[inline]
    fn board_pixels(&self) -> f32 {
        self.square_size * BOARD_SIZE as f32
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// UCI engine wrapper (feature-gated)
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "uci")]
struct UciEngineWrapper {
    engine: Uci,
    move_time_ms: u64,
}

#[cfg(feature = "uci")]
impl Drop for UciEngineWrapper {
    fn drop(&mut self) {
        // Gracefully shut down the engine process.
        if let Err(e) = self.engine.send("quit") {
            warn!("Failed to send 'quit' to UCI engine: {}", e);
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Main Chess GUI
// ──────────────────────────────────────────────────────────────────────────────

/// The main chess GUI struct.
///
/// Create via [`ChessGui::new`] with a [`ChessConfig`], call
/// [`ChessGui::load_pieces`] once to initialise textures (and optionally the UCI
/// engine), then call [`ChessGui::update`] every frame inside the macroquad loop.
pub struct ChessGui {
    board: Board,
    config: ChessConfig,
    pieces_texture: Option<Texture2D>,
    selected_square: Option<Square>,
    dragging_piece: Option<(Square, f32, f32)>,
    perspective: ChessColor,
    game_result: Option<GameResult>,
    status_message: String,
    /// Pre-computed texture source rectangles for each [`RenderPiece`], built once
    /// during construction.
    piece_rects: [Rect; 12],
    last_move: Option<(Square, Square)>,
    pending_promotion: Option<PendingPromotion>,
    error: Option<String>,
    /// Cached legal target squares for the currently selected piece.
    /// Invalidated whenever the selection or board changes.
    cached_legal_targets: Vec<Square>,
    /// Whether `cached_legal_targets` is valid for the current selection.
    legal_cache_valid: bool,
    #[cfg(feature = "uci")]
    uci_engine: Option<UciEngineWrapper>,
}

/// Stores the source and destination of a promotion move that is awaiting the
/// user's piece choice.
struct PendingPromotion {
    source: Square,
    dest: Square,
}

impl ChessGui {
    /// Create a new chess GUI with the given configuration.
    pub fn new(config: ChessConfig) -> Self {
        // Pre-build piece rects from the atlas layout.
        let rects = ALL_RENDER_PIECES.map(|variant| {
            let (x, y) = variant.tex_coords();
            Rect::new(x, y, ATLAS_TILE, ATLAS_TILE)
        });

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
            cached_legal_targets: Vec::with_capacity(32),
            legal_cache_valid: false,
            #[cfg(feature = "uci")]
            uci_engine: None,
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    /// Clear all transient UI/move state without resetting the board or perspective.
    fn clear_selection_state(&mut self) {
        self.selected_square = None;
        self.dragging_piece = None;
        self.cached_legal_targets.clear();
        self.legal_cache_valid = false;
    }

    /// Full reset of transient state (used by `restart`, `set_fen`, `set_board`).
    fn reset_state(&mut self) {
        self.clear_selection_state();
        self.game_result = None;
        self.status_message.clear();
        self.last_move = None;
        self.pending_promotion = None;
    }

    /// Invalidate the legal-targets cache so it is recomputed on the next frame.
    fn invalidate_legal_cache(&mut self) {
        self.legal_cache_valid = false;
    }

    /// Ensure `cached_legal_targets` is up-to-date for the current selection.
    fn ensure_legal_cache(&mut self) {
        if self.legal_cache_valid {
            return;
        }
        self.cached_legal_targets.clear();
        if let Some(sq) = self.selected_square {
            if self.board.piece_on(sq).is_some()
                && self.board.color_on(sq) == Some(self.board.side_to_move())
            {
                chess::MoveGen::new_legal(&self.board)
                    .filter(|m| m.get_source() == sq)
                    .for_each(|m| self.cached_legal_targets.push(m.get_dest()));
            }
        }
        self.legal_cache_valid = true;
    }

    fn get_board_offset(&self) -> (f32, f32) {
        if self.config.center_board {
            let board_pixels = self.config.square_size.round() * BOARD_SIZE as f32;
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

    // ── Texture / engine loading ─────────────────────────────────────────

    /// Load piece textures (and optionally start the UCI engine).
    ///
    /// Must be called once after creation and before the first [`update`](Self::update).
    /// On failure the internal `error` field is set and a [`ChessError`] is returned.
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

        // Validate texture dimensions — fall back to default on mismatch.
        if tex.width() == 0.0
            || tex.height() == 0.0
            || tex.width() as u32 != ATLAS_WIDTH
            || tex.height() as u32 != ATLAS_HEIGHT
        {
            warn!(
                "Invalid piece texture ({}x{}), falling back to default.",
                tex.width(),
                tex.height()
            );
            let default_tex = Texture2D::from_file_with_format(DEFAULT_PIECES_PNG, None);
            default_tex.set_filter(FilterMode::Nearest);
            if default_tex.width() == 0.0 || default_tex.height() == 0.0 {
                let msg = "Default texture also failed to load!";
                error!("{}", msg);
                self.error = Some(msg.to_string());
                return Err(ChessError::TextureLoad(msg.to_string()));
            }
            tex = default_tex;
        }

        self.pieces_texture = Some(tex);
        self.error = None;

        // Optionally start UCI engine.
        #[cfg(feature = "uci")]
        if let Some(ref path) = self.config.uci_engine_path {
            match self.init_uci_engine(path) {
                Ok(wrapper) => {
                    info!("UCI engine started: {}", path);
                    self.uci_engine = Some(wrapper);
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

    // ── Main update loop ─────────────────────────────────────────────────

    /// Run one frame of the GUI: draw, handle input, update engine.
    ///
    /// Call this inside the macroquad loop (after `next_frame().await` or
    /// equivalent).
    pub async fn update(&mut self) {
        // Check for game end (only when not awaiting promotion).
        if self.pending_promotion.is_none() && self.game_result.is_none() {
            if let Some(result) = check_game_result(&self.board) {
                self.game_result = Some(result);
                self.status_message = result.to_string();
            }
        }

        clear_background(BG_COLOR);

        // Keep legal-target cache fresh.
        self.invalidate_legal_cache();
        self.ensure_legal_cache();

        self.draw_board();

        let mut wants_pointer = false;
        let mut wants_keyboard = false;

        egui_macroquad::ui(|ctx| {
            self.build_ui(ctx);
            wants_pointer = ctx.wants_pointer_input();
            wants_keyboard = ctx.wants_keyboard_input();
        });

        egui_macroquad::draw();

        // Only handle board input when egui is idle and game is ongoing.
        let input_blocked = wants_pointer
            || wants_keyboard
            || self.game_result.is_some()
            || self.pending_promotion.is_some();

        if !input_blocked {
            self.handle_input();
            self.tick_uci_engine();
        }

        // Global keyboard shortcuts (always active).
        if is_key_pressed(KeyCode::R) {
            self.restart();
        }
        if is_key_pressed(KeyCode::F) {
            self.flip_perspective();
        }
    }

    /// Flip the board perspective.
    fn flip_perspective(&mut self) {
        self.perspective = match self.perspective {
            ChessColor::White => ChessColor::Black,
            ChessColor::Black => ChessColor::White,
        };
    }

    // ── UCI engine tick ─────────────────────────────────────────────────

    #[cfg(feature = "uci")]
    fn tick_uci_engine(&mut self) {
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
                        info!("Engine moved: {}", bestmove);
                    } else {
                        warn!("Engine played illegal move: {}", bestmove);
                    }
                }
            }
        }
    }

    #[cfg(not(feature = "uci"))]
    fn tick_uci_engine(&mut self) {
        // No-op when UCI feature is disabled.
    }

    // ── egui UI ──────────────────────────────────────────────────────────

    /// Build all egui UI elements.
    fn build_ui(&mut self, ctx: &egui_macroquad::egui::Context) {
        // Promotion dialog — takes precedence over everything else.
        if self.pending_promotion.is_some() {
            self.build_promotion_dialog(ctx);
            return;
        }

        // Main controls panel.
        Window::new("Controls")
            .anchor(Align2::RIGHT_TOP, (-10.0, 10.0))
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                // Status / turn indicator.
                let label = if let Some(result) = self.game_result {
                    result.to_string()
                } else if self.board.side_to_move() == ChessColor::White {
                    "White to move".to_string()
                } else {
                    "Black to move".to_string()
                };
                ui.label(label);

                ui.separator();

                if ui.button("New Game").clicked() {
                    self.restart();
                }
                if ui.button("Flip Board").clicked() {
                    self.flip_perspective();
                }

                if self.game_result.is_some() {
                    ui.label("Press R or click New Game");
                }
            });
    }

    /// Draw the promotion-piece selection dialog.
    fn build_promotion_dialog(&mut self, ctx: &egui_macroquad::egui::Context) {
        Window::new("Promotion")
            .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("Choose promotion piece:");
                ui.horizontal(|ui| {
                    let promotion_pieces = [
                        ChessPiece::Queen,
                        ChessPiece::Rook,
                        ChessPiece::Bishop,
                        ChessPiece::Knight,
                    ];
                    for &piece in &promotion_pieces {
                        let label = match piece {
                            ChessPiece::Queen => "\u{265B}",  // ♛
                            ChessPiece::Rook => "\u{265C}",   // ♜
                            ChessPiece::Bishop => "\u{265D}", // ♝
                            ChessPiece::Knight => "\u{265E}", // ♞
                            _ => "?",
                        };
                        if ui.button(label).clicked() {
                            if let Some(promo) = self.pending_promotion.take() {
                                let new_move =
                                    ChessMove::new(promo.source, promo.dest, Some(piece));
                                self.try_move(new_move);
                            }
                        }
                    }
                });
            });
    }

    // ── Board rendering ──────────────────────────────────────────────────

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

    /// Draw all 64 squares, last-move highlighting, selection, check highlight,
    /// and legal-move indicators.
    fn draw_squares_and_highlights(&self, geom: &BoardGeometry) {
        let size = geom.square_size;
        let check_square = if self.config.show_check_highlight {
            self.find_king_in_check()
        } else {
            None
        };

        for rank in 0..BOARD_SIZE {
            for file in 0..BOARD_SIZE {
                let is_light = (file + rank) % 2 == 0;
                let base_color = if is_light {
                    self.config.light_square_color
                } else {
                    self.config.dark_square_color
                };

                let (screen_x, screen_y) = geom.square_to_screen(file, rank);
                draw_rectangle(screen_x, screen_y, size, size, base_color);

                // We need the chess::Square for highlight checks.
                let sq = match get_square(file, rank) {
                    Some(s) => s,
                    None => continue,
                };

                // Last-move highlight (golden).
                if let Some((from, to)) = self.last_move {
                    if sq == from || sq == to {
                        draw_rectangle(
                            screen_x,
                            screen_y,
                            size,
                            size,
                            Color::new(1.0, 0.8, 0.0, HIGHLIGHT_ALPHA),
                        );
                    }
                }

                // Selected-square highlight (yellow).
                if Some(sq) == self.selected_square {
                    draw_rectangle(
                        screen_x,
                        screen_y,
                        size,
                        size,
                        Color::new(1.0, 1.0, 0.0, HIGHLIGHT_ALPHA),
                    );
                }

                // King-in-check highlight (red glow).
                if check_square == Some(sq) {
                    draw_rectangle(
                        screen_x,
                        screen_y,
                        size,
                        size,
                        Color::new(1.0, 0.0, 0.0, HIGHLIGHT_ALPHA),
                    );
                }

                // Legal-move indicators.
                if self.config.show_legal_moves && self.selected_square.is_some() {
                    if self.cached_legal_targets.contains(&sq) {
                        let cx = screen_x + size / 2.0;
                        let cy = screen_y + size / 2.0;
                        if self.board.piece_on(sq).is_some() {
                            // Capture: draw a ring.
                            draw_circle(
                                cx,
                                cy,
                                size * CAPTURE_RING_FRAC,
                                Color::new(0.0, 0.0, 0.0, DOT_ALPHA),
                            );
                        } else {
                            // Quiet move: draw a dot.
                            draw_circle(
                                cx,
                                cy,
                                size * DOT_RADIUS_FRAC,
                                Color::new(0.0, 0.0, 0.0, DOT_ALPHA),
                            );
                        }
                    }
                }
            }
        }
    }

    /// Find the square of the king that is currently in check, if any.
    fn find_king_in_check(&self) -> Option<Square> {
        if *self.board.checkers() == BitBoard::default() {
            return None;
        }
        let side = self.board.side_to_move();
        // Iterate over all squares to find the side-to-move's king.
        for rank in ALL_RANKS {
            for file in ALL_FILES {
                let sq = Square::make_square(rank, file);
                if self.board.piece_on(sq) == Some(ChessPiece::King)
                    && self.board.color_on(sq) == Some(side)
                {
                    return Some(sq);
                }
            }
        }
        None
    }

    fn draw_pieces(&self, texture: &Texture2D, geom: &BoardGeometry) {
        let size = geom.square_size;
        for rank in 0..BOARD_SIZE {
            for file in 0..BOARD_SIZE {
                let sq = match get_square(file, rank) {
                    Some(s) => s,
                    None => continue,
                };
                let Some(piece) = self.board.piece_on(sq) else {
                    continue;
                };

                // Skip the piece currently being dragged.
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
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn draw_dragged_piece(&self, texture: &Texture2D, _geom: &BoardGeometry) {
        let Some((sq, offset_x, offset_y)) = self.dragging_piece else {
            return;
        };
        let Some(piece) = self.board.piece_on(sq) else {
            return;
        };

        let color = self.board.color_on(sq).unwrap();
        let render_piece = RenderPiece::from_chess(piece, color);
        let rect = self.piece_rects[render_piece as usize];
        let (mx, my) = mouse_position();
        let size = self.config.square_size.round();

        draw_texture_ex(
            texture,
            mx - offset_x,
            my - offset_y,
            WHITE,
            DrawTextureParams {
                source: Some(rect),
                dest_size: Some(Vec2::new(size, size)),
                ..Default::default()
            },
        );
    }

    /// Draw file letters (a–h) along the bottom and rank numbers (1–8) along
    /// the left edge, coloured to contrast with their square.
    fn draw_coordinates(&self, geom: &BoardGeometry) {
        let size = geom.square_size;
        let font_size = (size / 4.0) as f32;

        for i in 0..BOARD_SIZE {
            // ── File labels along the bottom ──
            let file_ch = (b'a' + i as u8) as char;
            let file_label = file_ch.to_string();
            let is_light = (i + 0) % 2 == 0;
            let sq_color = if is_light {
                self.config.light_square_color
            } else {
                self.config.dark_square_color
            };
            let text_color = contrast_color(sq_color);

            let (x_bottom, _) = geom.square_to_screen(i, 0);
            let tx = x_bottom + size / 2.0
                - measure_text(&file_label, None, font_size as u16, 1.0).width / 2.0;
            let ty = geom.offset_y + geom.board_pixels() + 5.0 + font_size;
            draw_text(&file_label, tx, ty, font_size, text_color);

            // ── Rank labels along the left ──
            let rank_ch = (b'1' + i as u8) as char;
            let rank_label = rank_ch.to_string();
            let is_light_rank = (0 + i) % 2 == 0;
            let sq_color_rank = if is_light_rank {
                self.config.light_square_color
            } else {
                self.config.dark_square_color
            };
            let text_color_rank = contrast_color(sq_color_rank);

            let (_, y_rank) = geom.square_to_screen(0, i);
            let rx = geom.offset_x - 20.0;
            let ry = y_rank + size / 2.0 + font_size / 2.0;
            draw_text(&rank_label, rx, ry, font_size, text_color_rank);
        }
    }

    // ── Input handling ──────────────────────────────────────────────────

    fn handle_input(&mut self) {
        if self.game_result.is_some() {
            return;
        }

        let (mx, my) = mouse_position();
        let geom = self.geometry();

        let logical_sq = geom
            .screen_to_square(mx, my)
            .and_then(|(file, rank)| get_square(file, rank));

        // Clicked outside the board — clear selection.
        if logical_sq.is_none() {
            if is_mouse_button_released(MouseButton::Left) {
                self.clear_selection_state();
            }
            return;
        }

        let sq = logical_sq.unwrap();

        // ── Left press: start dragging / select ──
        if is_mouse_button_pressed(MouseButton::Left) {
            if self.board.piece_on(sq).is_some()
                && self.board.color_on(sq) == Some(self.board.side_to_move())
            {
                // If clicking the same square, deselect.
                if self.selected_square == Some(sq) {
                    self.clear_selection_state();
                    return;
                }
                self.selected_square = Some(sq);
                self.invalidate_legal_cache();

                let (screen_x, screen_y) = geom.square_to_screen(
                    sq.get_file().to_index() as u32,
                    sq.get_rank().to_index() as u32,
                );
                let offset_x = mx - screen_x;
                let offset_y = my - screen_y;
                self.dragging_piece = Some((sq, offset_x, offset_y));
            }
        }
        // ── Left release: attempt a move ──
        else if is_mouse_button_released(MouseButton::Left) {
            if let Some(from_sq) = self.selected_square {
                if from_sq != sq {
                    let is_pawn = self.board.piece_on(from_sq) == Some(ChessPiece::Pawn);
                    let promo_rank = if self.board.side_to_move() == ChessColor::White {
                        Rank::Eighth
                    } else {
                        Rank::First
                    };
                    let is_promotion = is_pawn && sq.get_rank() == promo_rank;

                    if is_promotion {
                        // Check if any promotion move from → to is legal.
                        let has_legal_promo = [
                            ChessPiece::Queen,
                            ChessPiece::Rook,
                            ChessPiece::Bishop,
                            ChessPiece::Knight,
                        ]
                        .iter()
                        .any(|&p| self.board.legal(ChessMove::new(from_sq, sq, Some(p))));

                        if has_legal_promo {
                            self.pending_promotion = Some(PendingPromotion {
                                source: from_sq,
                                dest: sq,
                            });
                            self.clear_selection_state();
                            return;
                        }
                    } else {
                        let chess_move = ChessMove::new(from_sq, sq, None);
                        if self.board.legal(chess_move) {
                            self.try_move(chess_move);
                        }
                    }
                }
                self.clear_selection_state();
            }
        }

        // ── Right click: cancel selection ──
        if is_mouse_button_pressed(MouseButton::Right) {
            self.clear_selection_state();
        }
    }

    // ── Game logic ──────────────────────────────────────────────────────

    /// Restart the game with the default starting position.
    ///
    /// The perspective is *preserved* if `auto_flip_perspective` is disabled,
    /// so a user who manually flipped the board won't have their preference
    /// overridden.
    pub fn restart(&mut self) {
        self.board = Board::default();
        self.reset_state();

        if self.config.auto_flip_perspective {
            self.perspective = ChessColor::White;
        }
        // When auto_flip is off, keep the user's current perspective.

        // Tell the UCI engine a new game has started.
        #[cfg(feature = "uci")]
        if let Some(ref mut uci_wrapper) = self.uci_engine {
            let _ = uci_wrapper.engine.send("ucinewgame");
        }
    }

    /// Attempt to execute a move on the board.
    ///
    /// Returns `true` if the move was legal and applied, `false` otherwise.
    /// On success, selection state is cleared, the last-move is recorded,
    /// the perspective may be flipped, and the game result is re-evaluated.
    pub fn try_move(&mut self, m: ChessMove) -> bool {
        if !self.board.legal(m) {
            return false;
        }
        self.last_move = Some((m.get_source(), m.get_dest()));
        self.board = self.board.make_move_new(m);

        if self.config.auto_flip_perspective {
            self.flip_perspective();
        }

        self.game_result = check_game_result(&self.board);
        self.pending_promotion = None;
        self.clear_selection_state();
        true
    }

    // ── Public accessors ──────────────────────────────────────────────────

    /// Read-only access to the underlying chess board.
    pub fn board(&self) -> &Board {
        &self.board
    }

    /// Current board perspective (which side is at the bottom).
    pub fn perspective(&self) -> ChessColor {
        self.perspective
    }

    /// Current position as a FEN string.
    pub fn fen(&self) -> String {
        self.board.to_string()
    }

    /// Computed board offset in pixels.
    pub fn board_offset(&self) -> (f32, f32) {
        self.get_board_offset()
    }

    /// Configured square size in pixels.
    pub fn square_size(&self) -> f32 {
        self.config.square_size
    }

    /// Current game result, if the game has ended.
    pub fn game_result(&self) -> Option<GameResult> {
        self.game_result
    }

    /// Status message (set when the game ends).
    pub fn status_message(&self) -> &str {
        &self.status_message
    }

    /// Last error message, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// All legal moves in the current position.
    pub fn legal_moves(&self) -> Vec<ChessMove> {
        chess::MoveGen::new_legal(&self.board).collect()
    }

    // ── Public mutators ──────────────────────────────────────────────────

    /// Load a new position from a FEN string.
    ///
    /// All transient state (selection, last-move, game-result) is reset.
    /// The perspective is set to the side to move.
    pub fn set_fen(&mut self, fen: &str) -> Result<(), ChessError> {
        Board::from_str(fen)
            .map(|b| {
                self.board = b;
                self.reset_state();
                self.perspective = self.board.side_to_move();
            })
            .map_err(|e| ChessError::InvalidFen(e.to_string()))
    }

    /// Replace the board with an arbitrary position.
    ///
    /// All transient state is reset; perspective follows the side to move.
    pub fn set_board(&mut self, board: Board) {
        self.board = board;
        self.reset_state();
        self.perspective = self.board.side_to_move();
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Free helper functions
// ──────────────────────────────────────────────────────────────────────────────

/// Return black or white depending on which contrasts best with `color`.
fn contrast_color(color: Color) -> Color {
    // ITU-R BT.601 luminance.
    let luminance = 0.299 * color.r + 0.587 * color.g + 0.114 * color.b;
    if luminance > 0.5 {
        BLACK
    } else {
        WHITE
    }
}

/// Parse a UCI `bestmove` string (e.g. `"e2e4"` or `"e7e8q"`) into a
/// [`ChessMove`]. Returns `None` if the string is malformed or represents
/// `(none)`.
#[cfg(feature = "uci")]
fn parse_uci_bestmove(bestmove: &str) -> Option<ChessMove> {
    let move_str = bestmove.split_whitespace().next()?;

    if move_str == "(none)" {
        return None;
    }

    let bytes = move_str.as_bytes();
    if bytes.len() < 4 {
        return None;
    }

    // Validate file/rank characters.
    let from_file = bytes[0].wrapping_sub(b'a') as u32;
    let from_rank = bytes[1].wrapping_sub(b'1') as u32;
    let to_file = bytes[2].wrapping_sub(b'a') as u32;
    let to_rank = bytes[3].wrapping_sub(b'1') as u32;

    if from_file >= BOARD_SIZE
        || from_rank >= BOARD_SIZE
        || to_file >= BOARD_SIZE
        || to_rank >= BOARD_SIZE
    {
        return None;
    }

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
