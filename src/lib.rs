//! # chess-render
//!
//! A configurable, embeddable chess GUI built with [Macroquad](https://macroquad.rs/).
//!
//! Features:
//! - Board and piece rendering from a texture atlas
//! - Built-in default piece texture
//! - Legal move generation via the `chess` crate
//! - Local two-player play
//! - Optional UCI engine integration
//! - Undo / redo
//! - SAN move history
//! - PGN export
//! - FEN export
//! - Chess clocks
//! - Resign / draw support
//! - Draw detection:
//!   - checkmate
//!   - stalemate
//!   - insufficient material
//!   - fifty-move rule
//!   - threefold repetition
//! - Board themes
//! - Responsive board sizing
//! - Move animation
//! - Coordinate labels
//! - Legal-move dots
//! - Check highlighting
//! - Last-move highlighting
//! - Promotion dialog
//!
//! # Quick Start
//!
//! ```ignore
//! use chess_render::{ChessConfig, ChessGui};
//! use macroquad::prelude::*;
//!
//! #[macroquad::main("Chess Render")]
//! async fn main() {
//!     let config = ChessConfig::default();
//!     let mut gui = ChessGui::new(config);
//!     gui.load_pieces().await.expect("failed to load pieces");
//!
//!     loop {
//!         gui.update().await;
//!         next_frame().await;
//!     }
//! }
//! ```

use chess::{BitBoard, BoardStatus, Rank, ALL_FILES, ALL_RANKS};
pub use chess::{Board, ChessMove, Color as ChessColor, Piece as ChessPiece, Square};

use egui_macroquad::egui::{Align2, ComboBox, ScrollArea, Slider, TextEdit, Window};
use log::{error, info, warn};
use macroquad::prelude::*;
use std::collections::VecDeque;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[cfg(feature = "uci")]
use uci::Uci;

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

const BOARD_SIZE: u32 = 8;

const ATLAS_TILE: f32 = 64.0;
const ATLAS_WIDTH: u32 = 384;
const ATLAS_HEIGHT: u32 = 128;

const HIGHLIGHT_ALPHA: f32 = 0.35;
const DOT_ALPHA: f32 = 0.40;
const DOT_RADIUS_FRAC: f32 = 1.0 / 6.0;
const CAPTURE_RING_FRAC: f32 = 1.0 / 3.0;

const BG_COLOR: Color = Color::new(180.0 / 255.0, 220.0 / 255.0, 255.0 / 255.0, 1.0);

const DEFAULT_PIECES_PNG: &[u8] = include_bytes!("assets/pieces.png");

// ──────────────────────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum ChessError {
    #[error("Failed to load piece texture: {0}")]
    TextureLoad(String),

    #[error("Invalid FEN: {0}")]
    InvalidFen(String),

    #[error("UCI engine error: {0}")]
    UciError(String),
}

// ──────────────────────────────────────────────────────────────────────────────
// Game result types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    WhiteWins,
    BlackWins,
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

impl GameResult {
    pub fn pgn_result(self) -> &'static str {
        match self {
            Self::WhiteWins => "1-0",
            Self::BlackWins => "0-1",
            Self::Draw => "1/2-1/2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEndReason {
    Checkmate,
    Stalemate,
    InsufficientMaterial,
    FiftyMoveRule,
    ThreefoldRepetition,
    Resignation,
    Agreement,
    Timeout,
}

impl fmt::Display for GameEndReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Checkmate => write!(f, "checkmate"),
            Self::Stalemate => write!(f, "stalemate"),
            Self::InsufficientMaterial => write!(f, "insufficient material"),
            Self::FiftyMoveRule => write!(f, "fifty-move rule"),
            Self::ThreefoldRepetition => write!(f, "threefold repetition"),
            Self::Resignation => write!(f, "resignation"),
            Self::Agreement => write!(f, "agreement"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}

fn opposite_color(color: ChessColor) -> ChessColor {
    match color {
        ChessColor::White => ChessColor::Black,
        ChessColor::Black => ChessColor::White,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Theme
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoardTheme {
    #[default]
    Classic,
    Blue,
    Green,
    Wood,
    Mono,
    HighContrast,
    Custom,
}

impl BoardTheme {
    pub fn name(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Blue => "Blue",
            Self::Green => "Green",
            Self::Wood => "Wood",
            Self::Mono => "Mono",
            Self::HighContrast => "High Contrast",
            Self::Custom => "Custom",
        }
    }

    pub fn colors(self) -> Option<(Color, Color)> {
        match self {
            Self::Classic => Some((
                Color::new(240.0 / 255.0, 217.0 / 255.0, 181.0 / 255.0, 1.0),
                Color::new(181.0 / 255.0, 136.0 / 255.0, 99.0 / 255.0, 1.0),
            )),
            Self::Blue => Some((
                Color::new(222.0 / 255.0, 235.0 / 255.0, 255.0 / 255.0, 1.0),
                Color::new(90.0 / 255.0, 130.0 / 255.0, 200.0 / 255.0, 1.0),
            )),
            Self::Green => Some((
                Color::new(238.0 / 255.0, 238.0 / 255.0, 210.0 / 255.0, 1.0),
                Color::new(110.0 / 255.0, 140.0 / 255.0, 90.0 / 255.0, 1.0),
            )),
            Self::Wood => Some((
                Color::new(233.0 / 255.0, 211.0 / 255.0, 180.0 / 255.0, 1.0),
                Color::new(145.0 / 255.0, 100.0 / 255.0, 62.0 / 255.0, 1.0),
            )),
            Self::Mono => Some((
                Color::new(0.85, 0.85, 0.85, 1.0),
                Color::new(0.45, 0.45, 0.45, 1.0),
            )),
            Self::HighContrast => Some((
                Color::new(0.95, 0.95, 0.95, 1.0),
                Color::new(0.20, 0.20, 0.20, 1.0),
            )),
            Self::Custom => None,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Time control
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeControl {
    pub enabled: bool,
    pub initial_secs: f32,
    pub increment_secs: f32,
}

impl Default for TimeControl {
    fn default() -> Self {
        Self {
            enabled: false,
            initial_secs: 300.0,
            increment_secs: 2.0,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// UCI types
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "uci")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineSide {
    None,
    White,
    Black,
    Both,
}

#[cfg(feature = "uci")]
impl Default for EngineSide {
    fn default() -> Self {
        Self::Black
    }
}

#[cfg(feature = "uci")]
impl EngineSide {
    pub fn controls(self, color: ChessColor) -> bool {
        match self {
            Self::None => false,
            Self::White => color == ChessColor::White,
            Self::Black => color == ChessColor::Black,
            Self::Both => true,
        }
    }
}

#[cfg(feature = "uci")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UciSearchLimit {
    MoveTime(u64),
    Depth(u32),
    Nodes(u64),
}

#[cfg(feature = "uci")]
impl Default for UciSearchLimit {
    fn default() -> Self {
        Self::MoveTime(1000)
    }
}

#[cfg(feature = "uci")]
impl UciSearchLimit {
    fn go_command(&self) -> String {
        match self {
            Self::MoveTime(ms) => format!("go movetime {ms}"),
            Self::Depth(depth) => format!("go depth {depth}"),
            Self::Nodes(nodes) => format!("go nodes {nodes}"),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Configuration
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ChessConfig {
    pub light_square_color: Color,
    pub dark_square_color: Color,
    pub board_theme: BoardTheme,

    pub square_size: f32,
    pub board_offset: (f32, f32),
    pub center_board: bool,
    pub responsive_board: bool,
    pub min_square_size: f32,
    pub max_square_size: f32,

    pub auto_flip_perspective: bool,
    pub promotion_piece: ChessPiece,
    pub piece_texture_path: Option<String>,

    pub show_coordinates: bool,
    pub coordinate_scale: f32,

    pub show_legal_moves: bool,
    pub show_check_highlight: bool,
    pub show_last_move: bool,
    pub show_grid: bool,
    pub show_border: bool,
    pub border_thickness: f32,

    pub piece_scale: f32,

    pub last_move_color: Color,
    pub selected_color: Color,
    pub check_color: Color,
    pub legal_move_color: Color,
    pub capture_ring_color: Color,
    pub grid_color: Color,
    pub border_color: Color,

    pub animate_moves: bool,
    pub animation_duration: f32,

    pub history_limit: usize,

    pub time_control: TimeControl,

    pub show_controls: bool,
    pub show_move_list: bool,
    pub show_clock: bool,
    pub auto_scroll_move_list: bool,

    pub pgn_event: String,
    pub pgn_site: String,
    pub pgn_white: String,
    pub pgn_black: String,

    #[cfg(feature = "uci")]
    pub uci_engine_path: Option<String>,
    #[cfg(feature = "uci")]
    pub engine_side: EngineSide,
    #[cfg(feature = "uci")]
    pub uci_search_limit: UciSearchLimit,
    #[cfg(feature = "uci")]
    pub uci_options: Vec<(String, String)>,
}

impl Default for ChessConfig {
    fn default() -> Self {
        let (light, dark) = BoardTheme::Classic.colors().expect("classic theme colors");

        Self {
            light_square_color: light,
            dark_square_color: dark,
            board_theme: BoardTheme::Classic,

            square_size: 64.0,
            board_offset: (0.0, 0.0),
            center_board: true,
            responsive_board: false,
            min_square_size: 32.0,
            max_square_size: 160.0,

            auto_flip_perspective: true,
            promotion_piece: ChessPiece::Queen,
            piece_texture_path: None,

            show_coordinates: true,
            coordinate_scale: 0.25,

            show_legal_moves: true,
            show_check_highlight: true,
            show_last_move: true,
            show_grid: false,
            show_border: true,
            border_thickness: 4.0,

            piece_scale: 1.0,

            last_move_color: Color::new(1.0, 0.8, 0.0, HIGHLIGHT_ALPHA),
            selected_color: Color::new(1.0, 1.0, 0.0, HIGHLIGHT_ALPHA),
            check_color: Color::new(1.0, 0.0, 0.0, HIGHLIGHT_ALPHA),
            legal_move_color: Color::new(0.0, 0.0, 0.0, DOT_ALPHA),
            capture_ring_color: Color::new(0.0, 0.0, 0.0, DOT_ALPHA + 0.10),
            grid_color: Color::new(0.0, 0.0, 0.0, 0.18),
            border_color: Color::new(30.0 / 255.0, 30.0 / 255.0, 30.0 / 255.0, 1.0),

            animate_moves: true,
            animation_duration: 0.18,

            history_limit: 512,

            time_control: TimeControl::default(),

            show_controls: true,
            show_move_list: true,
            show_clock: true,
            auto_scroll_move_list: true,

            pgn_event: "Casual Game".to_string(),
            pgn_site: "Local".to_string(),
            pgn_white: "Player 1".to_string(),
            pgn_black: "Player 2".to_string(),

            #[cfg(feature = "uci")]
            uci_engine_path: None,
            #[cfg(feature = "uci")]
            engine_side: EngineSide::default(),
            #[cfg(feature = "uci")]
            uci_search_limit: UciSearchLimit::default(),
            #[cfg(feature = "uci")]
            uci_options: Vec::new(),
        }
    }
}

impl ChessConfig {
    pub fn builder() -> ChessConfigBuilder {
        ChessConfigBuilder {
            inner: Self::default(),
        }
    }

    pub fn apply_theme(&mut self, theme: BoardTheme) {
        self.board_theme = theme;
        if let Some((light, dark)) = theme.colors() {
            self.light_square_color = light;
            self.dark_square_color = dark;
        }
    }

    fn validate(&mut self) {
        if self.square_size <= 0.0 {
            warn!("square_size must be positive; falling back to 64.0");
            self.square_size = 64.0;
        }

        if self.min_square_size <= 0.0 {
            self.min_square_size = 32.0;
        }

        if self.max_square_size <= 0.0 {
            self.max_square_size = 160.0;
        }

        if self.max_square_size < self.min_square_size {
            std::mem::swap(&mut self.min_square_size, &mut self.max_square_size);
        }

        self.square_size = self
            .square_size
            .clamp(self.min_square_size, self.max_square_size);
        self.piece_scale = self.piece_scale.clamp(0.5, 1.0);
        self.coordinate_scale = self.coordinate_scale.clamp(0.10, 0.60);

        if self.animation_duration < 0.0 {
            self.animation_duration = 0.0;
        }
        if self.animation_duration > 2.0 {
            self.animation_duration = 2.0;
        }

        if self.history_limit < 16 {
            self.history_limit = 512;
        }
        if self.history_limit > 100_000 {
            self.history_limit = 100_000;
        }

        if self.border_thickness < 0.0 {
            self.border_thickness = 0.0;
        }

        if self.time_control.initial_secs < 1.0 {
            self.time_control.initial_secs = 1.0;
        }
        if self.time_control.increment_secs < 0.0 {
            self.time_control.increment_secs = 0.0;
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChessConfigBuilder {
    inner: ChessConfig,
}

impl ChessConfigBuilder {
    pub fn light_square_color(mut self, c: Color) -> Self {
        self.inner.light_square_color = c;
        self.inner.board_theme = BoardTheme::Custom;
        self
    }

    pub fn dark_square_color(mut self, c: Color) -> Self {
        self.inner.dark_square_color = c;
        self.inner.board_theme = BoardTheme::Custom;
        self
    }

    pub fn theme(mut self, theme: BoardTheme) -> Self {
        self.inner.apply_theme(theme);
        self
    }

    pub fn square_size(mut self, s: f32) -> Self {
        self.inner.square_size = s;
        self
    }

    pub fn board_offset(mut self, x: f32, y: f32) -> Self {
        self.inner.board_offset = (x, y);
        self
    }

    pub fn center_board(mut self, yes: bool) -> Self {
        self.inner.center_board = yes;
        self
    }

    pub fn responsive_board(mut self, yes: bool) -> Self {
        self.inner.responsive_board = yes;
        self
    }

    pub fn min_square_size(mut self, s: f32) -> Self {
        self.inner.min_square_size = s;
        self
    }

    pub fn max_square_size(mut self, s: f32) -> Self {
        self.inner.max_square_size = s;
        self
    }

    pub fn auto_flip_perspective(mut self, yes: bool) -> Self {
        self.inner.auto_flip_perspective = yes;
        self
    }

    pub fn promotion_piece(mut self, p: ChessPiece) -> Self {
        self.inner.promotion_piece = p;
        self
    }

    pub fn piece_texture_path(mut self, path: impl Into<String>) -> Self {
        self.inner.piece_texture_path = Some(path.into());
        self
    }

    pub fn show_coordinates(mut self, yes: bool) -> Self {
        self.inner.show_coordinates = yes;
        self
    }

    pub fn coordinate_scale(mut self, scale: f32) -> Self {
        self.inner.coordinate_scale = scale;
        self
    }

    pub fn show_legal_moves(mut self, yes: bool) -> Self {
        self.inner.show_legal_moves = yes;
        self
    }

    pub fn show_check_highlight(mut self, yes: bool) -> Self {
        self.inner.show_check_highlight = yes;
        self
    }

    pub fn show_last_move(mut self, yes: bool) -> Self {
        self.inner.show_last_move = yes;
        self
    }

    pub fn show_grid(mut self, yes: bool) -> Self {
        self.inner.show_grid = yes;
        self
    }

    pub fn show_border(mut self, yes: bool) -> Self {
        self.inner.show_border = yes;
        self
    }

    pub fn border_thickness(mut self, thickness: f32) -> Self {
        self.inner.border_thickness = thickness;
        self
    }

    pub fn piece_scale(mut self, scale: f32) -> Self {
        self.inner.piece_scale = scale;
        self
    }

    pub fn animate_moves(mut self, yes: bool) -> Self {
        self.inner.animate_moves = yes;
        self
    }

    pub fn animation_duration(mut self, seconds: f32) -> Self {
        self.inner.animation_duration = seconds;
        self
    }

    pub fn history_limit(mut self, limit: usize) -> Self {
        self.inner.history_limit = limit;
        self
    }

    pub fn clock(mut self, initial_secs: f32, increment_secs: f32) -> Self {
        self.inner.time_control = TimeControl {
            enabled: true,
            initial_secs,
            increment_secs,
        };
        self
    }

    pub fn time_control(mut self, tc: TimeControl) -> Self {
        self.inner.time_control = tc;
        self
    }

    pub fn show_controls(mut self, yes: bool) -> Self {
        self.inner.show_controls = yes;
        self
    }

    pub fn show_move_list(mut self, yes: bool) -> Self {
        self.inner.show_move_list = yes;
        self
    }

    pub fn show_clock(mut self, yes: bool) -> Self {
        self.inner.show_clock = yes;
        self
    }

    pub fn auto_scroll_move_list(mut self, yes: bool) -> Self {
        self.inner.auto_scroll_move_list = yes;
        self
    }

    pub fn pgn_headers(
        mut self,
        event: impl Into<String>,
        site: impl Into<String>,
        white: impl Into<String>,
        black: impl Into<String>,
    ) -> Self {
        self.inner.pgn_event = event.into();
        self.inner.pgn_site = site.into();
        self.inner.pgn_white = white.into();
        self.inner.pgn_black = black.into();
        self
    }

    #[cfg(feature = "uci")]
    pub fn uci_engine_path(mut self, path: impl Into<String>) -> Self {
        self.inner.uci_engine_path = Some(path.into());
        self
    }

    #[cfg(feature = "uci")]
    pub fn engine_plays_as(mut self, side: EngineSide) -> Self {
        self.inner.engine_side = side;
        self
    }

    #[cfg(feature = "uci")]
    pub fn uci_search_limit(mut self, limit: UciSearchLimit) -> Self {
        self.inner.uci_search_limit = limit;
        self
    }

    #[cfg(feature = "uci")]
    pub fn uci_move_time_ms(mut self, ms: u64) -> Self {
        self.inner.uci_search_limit = UciSearchLimit::MoveTime(ms);
        self
    }

    #[cfg(feature = "uci")]
    pub fn uci_option(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.inner.uci_options.push((name.into(), value.into()));
        self
    }

    pub fn build(mut self) -> ChessConfig {
        self.inner.validate();
        self.inner
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal render piece
// ──────────────────────────────────────────────────────────────────────────────

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

    fn tex_coords(&self) -> (f32, f32) {
        let idx = *self as usize;
        let col = (idx % 6) as f32;
        let row = (idx / 6) as f32;
        (col * ATLAS_TILE, row * ATLAS_TILE)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Geometry
// ──────────────────────────────────────────────────────────────────────────────

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
// Clock
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct ClockState {
    white: f32,
    black: f32,
}

impl ClockState {
    fn new(tc: &TimeControl) -> Self {
        Self {
            white: tc.initial_secs,
            black: tc.initial_secs,
        }
    }

    fn tick(&mut self, side: ChessColor, dt: f32) -> bool {
        let t = match side {
            ChessColor::White => &mut self.white,
            ChessColor::Black => &mut self.black,
        };

        *t -= dt;
        if *t < 0.0 {
            *t = 0.0;
            true
        } else {
            false
        }
    }

    fn add_increment(&mut self, side: ChessColor, inc: f32) {
        if inc <= 0.0 {
            return;
        }

        match side {
            ChessColor::White => self.white += inc,
            ChessColor::Black => self.black += inc,
        }
    }

    fn formatted(&self, side: ChessColor) -> String {
        let secs = match side {
            ChessColor::White => self.white,
            ChessColor::Black => self.black,
        }
        .max(0.0) as u32;

        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Animation / history types
// ──────────────────────────────────────────────────────────────────────────────

struct PieceAnimation {
    piece: ChessPiece,
    color: ChessColor,
    from: Square,
    to: Square,
    start: f64,
    duration: f64,
    pending_flip: Option<ChessColor>,
}

#[derive(Debug, Clone)]
pub struct MoveRecord {
    pub color: ChessColor,
    pub san: String,
    pub uci: String,
    pub source: Square,
    pub dest: Square,
}

#[derive(Clone)]
struct HistorySnapshot {
    board: Board,
    last_move: Option<(Square, Square)>,
    game_result: Option<GameResult>,
    game_end_reason: Option<GameEndReason>,
    status_message: String,
    halfmove_clock: u32,
    position_history: VecDeque<String>,
    clock: ClockState,
    move_records: Vec<MoveRecord>,
    perspective: ChessColor,
}

struct PendingPromotion {
    source: Square,
    dest: Square,
}

// ──────────────────────────────────────────────────────────────────────────────
// UCI wrapper
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "uci")]
struct UciEngineWrapper {
    engine: Uci,
    search_limit: UciSearchLimit,
}

#[cfg(feature = "uci")]
impl Drop for UciEngineWrapper {
    fn drop(&mut self) {
        if let Err(e) = self.engine.send("quit") {
            warn!("Failed to send quit to UCI engine: {e}");
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Main GUI
// ──────────────────────────────────────────────────────────────────────────────

pub struct ChessGui {
    board: Board,
    config: ChessConfig,

    pieces_texture: Option<Texture2D>,

    selected_square: Option<Square>,
    dragging_piece: Option<(Square, f32, f32)>,

    perspective: ChessColor,

    game_result: Option<GameResult>,
    game_end_reason: Option<GameEndReason>,
    status_message: String,

    piece_rects: [Rect; 12],
    last_move: Option<(Square, Square)>,
    pending_promotion: Option<PendingPromotion>,
    error: Option<String>,

    cached_legal_targets: Vec<Square>,
    legal_cache_valid: bool,

    move_records: Vec<MoveRecord>,
    undo_stack: Vec<HistorySnapshot>,
    redo_stack: Vec<HistorySnapshot>,

    position_history: VecDeque<String>,
    halfmove_clock: u32,

    clock: ClockState,
    animation: Option<PieceAnimation>,

    settings_open: bool,
    export_open: bool,
    export_text: String,

    #[cfg(feature = "uci")]
    uci_engine: Option<UciEngineWrapper>,
}

impl ChessGui {
    pub fn new(config: ChessConfig) -> Self {
        let rects = ALL_RENDER_PIECES.map(|variant| {
            let (x, y) = variant.tex_coords();
            Rect::new(x, y, ATLAS_TILE, ATLAS_TILE)
        });

        let clock = ClockState::new(&config.time_control);

        let mut position_history = VecDeque::with_capacity(config.history_limit.max(16) + 1);
        position_history.push_back(position_key(&Board::default()));

        Self {
            board: Board::default(),
            config,
            pieces_texture: None,
            selected_square: None,
            dragging_piece: None,
            perspective: ChessColor::White,
            game_result: None,
            game_end_reason: None,
            status_message: String::new(),
            piece_rects: rects,
            last_move: None,
            pending_promotion: None,
            error: None,
            cached_legal_targets: Vec::with_capacity(64),
            legal_cache_valid: false,
            move_records: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            position_history,
            halfmove_clock: 0,
            clock,
            animation: None,
            settings_open: false,
            export_open: false,
            export_text: String::new(),

            #[cfg(feature = "uci")]
            uci_engine: None,
        }
    }

    // ── State helpers ────────────────────────────────────────────────────

    fn clear_selection_state(&mut self) {
        self.selected_square = None;
        self.dragging_piece = None;
        self.cached_legal_targets.clear();
        self.legal_cache_valid = false;
    }

    fn reset_transient(&mut self) {
        self.clear_selection_state();
        self.game_result = None;
        self.game_end_reason = None;
        self.status_message.clear();
        self.last_move = None;
        self.pending_promotion = None;
        self.animation = None;
    }

    fn reset_history_and_clocks(&mut self) {
        self.move_records.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.position_history.clear();
        self.position_history.push_back(position_key(&self.board));
        self.halfmove_clock = 0;
        self.clock = ClockState::new(&self.config.time_control);
    }

    fn invalidate_legal_cache(&mut self) {
        self.legal_cache_valid = false;
    }

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

    fn current_square_size(&self) -> f32 {
        let min = self.config.min_square_size.min(self.config.max_square_size);
        let max = self.config.min_square_size.max(self.config.max_square_size);

        let base = if self.config.responsive_board {
            let margin = if self.config.show_coordinates {
                80.0
            } else {
                32.0
            };

            let available = (screen_width().min(screen_height()) - margin).max(160.0);
            available / BOARD_SIZE as f32
        } else {
            self.config.square_size
        };

        base.clamp(min, max).round()
    }

    fn get_board_offset(&self) -> (f32, f32) {
        if self.config.center_board {
            let board_pixels = self.current_square_size() * BOARD_SIZE as f32;
            let ox = (screen_width() - board_pixels) / 2.0;
            let oy = (screen_height() - board_pixels) / 2.0;
            (ox, oy)
        } else {
            self.config.board_offset
        }
    }

    fn geometry(&self) -> BoardGeometry {
        BoardGeometry::new(
            self.get_board_offset(),
            self.current_square_size(),
            self.perspective,
        )
    }

    fn is_animating(&self) -> bool {
        self.animation.is_some()
    }

    // ── Loading ──────────────────────────────────────────────────────────

    pub async fn load_pieces(&mut self) -> Result<(), ChessError> {
        let custom_path = self.config.piece_texture_path.clone();

        let image_data = if let Some(path) = custom_path {
            match load_file(&path).await {
                Ok(data) => data,
                Err(e) => {
                    let msg = format!("Failed to load custom texture from {path}: {e}");
                    error!("{msg}");
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
                let msg = "Default texture also failed to load!".to_string();
                error!("{msg}");
                self.error = Some(msg.clone());
                return Err(ChessError::TextureLoad(msg));
            }

            tex = default_tex;
        }

        self.pieces_texture = Some(tex);
        self.error = None;
        info!(
            "Piece texture loaded ({}x{})",
            self.pieces_texture
                .as_ref()
                .map(|t| t.width())
                .unwrap_or(0.0),
            self.pieces_texture
                .as_ref()
                .map(|t| t.height())
                .unwrap_or(0.0)
        );

        #[cfg(feature = "uci")]
        {
            let engine_path = self.config.uci_engine_path.clone();
            if let Some(path) = engine_path {
                match self.init_uci_engine(&path) {
                    Ok(wrapper) => {
                        info!("UCI engine started: {path}");
                        self.uci_engine = Some(wrapper);
                    }
                    Err(e) => {
                        let msg = format!("Failed to start UCI engine: {e}");
                        error!("{msg}");
                        self.error = Some(msg);
                    }
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

        for (name, value) in &self.config.uci_options {
            engine
                .send(&format!("setoption name {name} value {value}"))
                .map_err(|e| ChessError::UciError(e.to_string()))?;
        }

        engine
            .send("ucinewgame")
            .map_err(|e| ChessError::UciError(e.to_string()))?;

        Ok(UciEngineWrapper {
            engine,
            search_limit: self.config.uci_search_limit,
        })
    }

    // ── Main update ──────────────────────────────────────────────────────

    pub async fn update(&mut self) {
        let dt = get_frame_time();

        self.update_animation();

        if self.game_result.is_none() && self.pending_promotion.is_none() && !self.is_animating() {
            self.update_clock(dt);
        }

        if self.pending_promotion.is_none() && self.game_result.is_none() {
            self.evaluate_game_end();
        }

        clear_background(BG_COLOR);

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

        let input_blocked = wants_pointer
            || wants_keyboard
            || self.game_result.is_some()
            || self.pending_promotion.is_some()
            || self.is_animating();

        if !input_blocked {
            self.handle_input();
            self.tick_uci_engine();
        }

        self.handle_shortcuts(!wants_keyboard);
    }

    fn update_animation(&mut self) {
        if let Some(anim) = &self.animation {
            let now = get_time();
            if now - anim.start >= anim.duration {
                let pending = anim.pending_flip;
                self.animation = None;

                if let Some(p) = pending {
                    self.perspective = p;
                }
            }
        }
    }

    fn update_clock(&mut self, dt: f32) {
        if !self.config.time_control.enabled || self.game_result.is_some() {
            return;
        }

        let side = self.board.side_to_move();
        if self.clock.tick(side, dt) {
            let winner = opposite_color(side);
            let result = match winner {
                ChessColor::White => GameResult::WhiteWins,
                ChessColor::Black => GameResult::BlackWins,
            };

            self.finish_game(result, GameEndReason::Timeout);
        }
    }

    fn evaluate_game_end(&mut self) {
        if self.game_result.is_some() {
            return;
        }

        if let Some((result, reason)) = check_board_status_result(&self.board) {
            self.finish_game(result, reason);
            return;
        }

        if insufficient_material(&self.board) {
            self.finish_draw(GameEndReason::InsufficientMaterial);
            return;
        }

        if self.halfmove_clock >= 100 {
            self.finish_draw(GameEndReason::FiftyMoveRule);
            return;
        }

        if self.position_count_current() >= 3 {
            self.finish_draw(GameEndReason::ThreefoldRepetition);
        }
    }

    fn finish_game(&mut self, result: GameResult, reason: GameEndReason) {
        self.game_result = Some(result);
        self.game_end_reason = Some(reason);
        self.status_message = format!("{result} ({reason})");
        self.clear_selection_state();
        self.pending_promotion = None;
    }

    fn finish_draw(&mut self, reason: GameEndReason) {
        self.finish_game(GameResult::Draw, reason);
    }

    pub fn resign(&mut self) {
        if self.game_result.is_some() {
            return;
        }

        self.stop_uci_search();

        let loser = self.board.side_to_move();
        let winner = opposite_color(loser);

        let result = match winner {
            ChessColor::White => GameResult::WhiteWins,
            ChessColor::Black => GameResult::BlackWins,
        };

        self.finish_game(result, GameEndReason::Resignation);
    }

    pub fn offer_draw(&mut self) {
        if self.game_result.is_some() {
            return;
        }

        self.stop_uci_search();
        self.finish_draw(GameEndReason::Agreement);
    }

    // ── Undo / redo ──────────────────────────────────────────────────────

    fn current_snapshot(&self) -> HistorySnapshot {
        HistorySnapshot {
            board: self.board.clone(),
            last_move: self.last_move,
            game_result: self.game_result,
            game_end_reason: self.game_end_reason,
            status_message: self.status_message.clone(),
            halfmove_clock: self.halfmove_clock,
            position_history: self.position_history.clone(),
            clock: self.clock,
            move_records: self.move_records.clone(),
            perspective: self.perspective,
        }
    }

    fn restore_snapshot(&mut self, snap: HistorySnapshot) {
        self.board = snap.board;
        self.last_move = snap.last_move;
        self.game_result = snap.game_result;
        self.game_end_reason = snap.game_end_reason;
        self.status_message = snap.status_message;
        self.halfmove_clock = snap.halfmove_clock;
        self.position_history = snap.position_history;
        self.clock = snap.clock;
        self.move_records = snap.move_records;
        self.perspective = snap.perspective;

        self.clear_selection_state();
        self.pending_promotion = None;
        self.animation = None;
    }

    fn push_undo_snapshot(&mut self) {
        let snap = self.current_snapshot();
        self.undo_stack.push(snap);

        if self.undo_stack.len() > self.config.history_limit {
            let excess = self.undo_stack.len() - self.config.history_limit;
            self.undo_stack.drain(0..excess);
        }

        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        self.stop_uci_search();

        if let Some(snap) = self.undo_stack.pop() {
            self.redo_stack.push(self.current_snapshot());
            self.restore_snapshot(snap);
        }
    }

    pub fn redo(&mut self) {
        self.stop_uci_search();

        if let Some(snap) = self.redo_stack.pop() {
            self.undo_stack.push(self.current_snapshot());
            self.restore_snapshot(snap);
        }
    }

    #[cfg(feature = "uci")]
    fn stop_uci_search(&mut self) {
        if let Some(wrapper) = self.uci_engine.as_mut() {
            let _ = wrapper.engine.send("stop");
        }
    }

    #[cfg(not(feature = "uci"))]
    fn stop_uci_search(&mut self) {}

    // ── Game control ─────────────────────────────────────────────────────

    pub fn restart(&mut self) {
        self.stop_uci_search();

        self.board = Board::default();
        self.reset_transient();
        self.reset_history_and_clocks();

        if self.config.auto_flip_perspective {
            self.perspective = ChessColor::White;
        }

        #[cfg(feature = "uci")]
        if let Some(wrapper) = self.uci_engine.as_mut() {
            let _ = wrapper.engine.send("ucinewgame");
        }
    }

    pub fn try_move(&mut self, m: ChessMove) -> bool {
        if !self.board.legal(m) {
            return false;
        }

        self.push_undo_snapshot();

        let san = san_for_move(&self.board, m);
        let uci = move_to_uci(m);

        let mover = self.board.side_to_move();
        let source = m.get_source();
        let dest = m.get_dest();

        let moved_piece = self.board.piece_on(source).unwrap_or(ChessPiece::Pawn);
        let target = self.board.piece_on(dest);

        let is_capture = target.is_some()
            || (moved_piece == ChessPiece::Pawn && source.get_file() != dest.get_file());

        self.last_move = Some((source, dest));
        self.board = self.board.make_move_new(m);

        self.halfmove_clock = if moved_piece == ChessPiece::Pawn || is_capture {
            0
        } else {
            self.halfmove_clock.saturating_add(1)
        };

        self.position_history.push_back(position_key(&self.board));
        while self.position_history.len() > self.config.history_limit.saturating_add(1) {
            self.position_history.pop_front();
        }

        self.move_records.push(MoveRecord {
            color: mover,
            san,
            uci,
            source,
            dest,
        });

        if self.config.time_control.enabled {
            self.clock
                .add_increment(mover, self.config.time_control.increment_secs);
        }

        let pending_flip = if self.config.auto_flip_perspective {
            Some(opposite_color(mover))
        } else {
            None
        };

        self.pending_promotion = None;
        self.clear_selection_state();

        self.evaluate_game_end();

        if self.config.animate_moves && self.config.animation_duration > 0.0 {
            self.animation = Some(PieceAnimation {
                piece: moved_piece,
                color: mover,
                from: source,
                to: dest,
                start: get_time(),
                duration: self.config.animation_duration as f64,
                pending_flip,
            });
        } else if let Some(p) = pending_flip {
            self.perspective = p;
        }

        true
    }

    fn attempt_move_from_to(&mut self, from: Square, to: Square) -> bool {
        if from == to {
            return false;
        }

        let is_pawn = self.board.piece_on(from) == Some(ChessPiece::Pawn);
        let promo_rank = if self.board.side_to_move() == ChessColor::White {
            Rank::Eighth
        } else {
            Rank::First
        };

        if is_pawn && to.get_rank() == promo_rank {
            let has_legal_promotion = [
                ChessPiece::Queen,
                ChessPiece::Rook,
                ChessPiece::Bishop,
                ChessPiece::Knight,
            ]
            .iter()
            .any(|&p| self.board.legal(ChessMove::new(from, to, Some(p))));

            if has_legal_promotion {
                self.pending_promotion = Some(PendingPromotion {
                    source: from,
                    dest: to,
                });
                self.clear_selection_state();
                return true;
            }
        }

        let m = ChessMove::new(from, to, None);
        if self.board.legal(m) {
            self.try_move(m);
            true
        } else {
            false
        }
    }

    pub fn flip_perspective(&mut self) {
        self.perspective = opposite_color(self.perspective);
    }

    // ── UCI engine tick ──────────────────────────────────────────────────

    #[cfg(feature = "uci")]
    fn tick_uci_engine(&mut self) {
        let mut best: Option<String> = None;

        {
            let Some(wrapper) = self.uci_engine.as_mut() else {
                return;
            };

            let go_cmd = wrapper.search_limit.go_command();
            let controls_side = self.config.engine_side.controls(self.board.side_to_move());
            let engine = &mut wrapper.engine;

            if controls_side
                && self.game_result.is_none()
                && self.dragging_piece.is_none()
                && !self.is_animating()
                && self.pending_promotion.is_none()
                && !engine.is_searching()
            {
                let fen = self.board.to_string();

                if let Err(e) = engine.send(&format!("position fen {fen}")) {
                    error!("Failed to send position to engine: {e}");
                } else if let Err(e) = engine.send(&go_cmd) {
                    error!("Failed to send go command to engine: {e}");
                }
            }

            if let Ok(Some(bestmove)) = engine.bestmove() {
                best = Some(bestmove);
            }
        }

        if let Some(bestmove) = best {
            if let Some(m) = parse_uci_bestmove(&bestmove) {
                if self.try_move(m) {
                    info!("Engine moved: {bestmove}");
                } else {
                    warn!("Engine played illegal move: {bestmove}");
                }
            }
        }
    }

    #[cfg(not(feature = "uci"))]
    fn tick_uci_engine(&mut self) {}

    // ── Input ────────────────────────────────────────────────────────────

    fn handle_input(&mut self) {
        if self.game_result.is_some() || self.is_animating() {
            return;
        }

        let (mx, my) = mouse_position();
        let geom = self.geometry();

        let logical_sq = geom
            .screen_to_square(mx, my)
            .and_then(|(file, rank)| get_square(file, rank));

        let Some(sq) = logical_sq else {
            if is_mouse_button_released(MouseButton::Left) {
                self.clear_selection_state();
            }
            return;
        };

        if is_mouse_button_pressed(MouseButton::Left) {
            let own_piece = self.board.piece_on(sq).is_some()
                && self.board.color_on(sq) == Some(self.board.side_to_move());

            if own_piece {
                self.selected_square = Some(sq);
                self.invalidate_legal_cache();

                let (screen_x, screen_y) = geom.square_to_screen(
                    sq.get_file().to_index() as u32,
                    sq.get_rank().to_index() as u32,
                );

                let offset_x = mx - screen_x;
                let offset_y = my - screen_y;

                self.dragging_piece = Some((sq, offset_x, offset_y));
            } else if let Some(from) = self.selected_square {
                let _ = self.attempt_move_from_to(from, sq);
                self.clear_selection_state();
            } else {
                self.clear_selection_state();
            }
        } else if is_mouse_button_released(MouseButton::Left) {
            self.dragging_piece = None;

            if let Some(from) = self.selected_square {
                if from != sq {
                    let _ = self.attempt_move_from_to(from, sq);
                }
                self.clear_selection_state();
            }
        }

        if is_mouse_button_pressed(MouseButton::Right) {
            self.clear_selection_state();
        }
    }

    fn handle_shortcuts(&mut self, allow: bool) {
        if !allow {
            return;
        }

        if is_key_pressed(KeyCode::R) {
            self.restart();
        }

        if is_key_pressed(KeyCode::F) {
            if self.is_animating() {
                if let Some(anim) = self.animation.take() {
                    if let Some(p) = anim.pending_flip {
                        self.perspective = p;
                    }
                }
            } else {
                self.flip_perspective();
            }
        }

        if is_key_pressed(KeyCode::U) {
            self.undo();
        }

        if is_key_pressed(KeyCode::Y) {
            self.redo();
        }
    }

    // ── UI ───────────────────────────────────────────────────────────────

    fn build_ui(&mut self, ctx: &egui_macroquad::egui::Context) {
        if self.pending_promotion.is_some() {
            self.build_promotion_dialog(ctx);
            return;
        }

        let mut new_game = false;
        let mut undo = false;
        let mut redo = false;
        let mut flip = false;
        let mut resign = false;
        let mut draw = false;
        let mut toggle_settings = false;
        let mut export_fen = false;
        let mut export_pgn = false;
        let mut reset_clock = false;

        let status_label = self.status_text();
        let white_time = self.clock.formatted(ChessColor::White);
        let black_time = self.clock.formatted(ChessColor::Black);
        let game_over = self.game_result.is_some();
        let move_lines = self.move_lines();

        let show_controls = self.config.show_controls;
        let show_move_list = self.config.show_move_list;
        let show_clock = self.config.show_clock && self.config.time_control.enabled;
        let auto_scroll = self.config.auto_scroll_move_list;

        if show_controls {
            Window::new("Controls")
                .anchor(Align2::RIGHT_TOP, (-10.0, 10.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(status_label);

                    if show_clock {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(format!("White {white_time}"));
                            ui.label(format!("Black {black_time}"));
                        });
                    }

                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.button("New").clicked() {
                            new_game = true;
                        }
                        if ui.button("Flip").clicked() {
                            flip = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Undo").clicked() {
                            undo = true;
                        }
                        if ui.button("Redo").clicked() {
                            redo = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Export FEN").clicked() {
                            export_fen = true;
                        }
                        if ui.button("Export PGN").clicked() {
                            export_pgn = true;
                        }
                    });

                    if ui.button("Settings").clicked() {
                        toggle_settings = true;
                    }

                    if !game_over {
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Resign").clicked() {
                                resign = true;
                            }
                            if ui.button("½-½").clicked() {
                                draw = true;
                            }
                        });
                    }
                });
        }

        if show_move_list {
            Window::new("Moves")
                .anchor(Align2::LEFT_TOP, (10.0, 10.0))
                .resizable(true)
                .collapsible(true)
                .default_width(220.0)
                .show(ctx, |ui| {
                    ScrollArea::vertical()
                        .max_height(240.0)
                        .stick_to_bottom(auto_scroll)
                        .show(ui, |ui| {
                            if move_lines.is_empty() {
                                ui.label("No moves yet.");
                            } else {
                                for line in move_lines {
                                    ui.label(line);
                                }
                            }
                        });
                });
        }

        if self.settings_open {
            Window::new("Settings")
                .anchor(Align2::RIGHT_TOP, (-10.0, 300.0))
                .open(&mut self.settings_open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    let theme_name = self.config.board_theme.name();
                    let old_theme = self.config.board_theme;

                    ComboBox::from_label("Theme")
                        .selected_text(theme_name)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.config.board_theme,
                                BoardTheme::Classic,
                                BoardTheme::Classic.name(),
                            );
                            ui.selectable_value(
                                &mut self.config.board_theme,
                                BoardTheme::Blue,
                                BoardTheme::Blue.name(),
                            );
                            ui.selectable_value(
                                &mut self.config.board_theme,
                                BoardTheme::Green,
                                BoardTheme::Green.name(),
                            );
                            ui.selectable_value(
                                &mut self.config.board_theme,
                                BoardTheme::Wood,
                                BoardTheme::Wood.name(),
                            );
                            ui.selectable_value(
                                &mut self.config.board_theme,
                                BoardTheme::Mono,
                                BoardTheme::Mono.name(),
                            );
                            ui.selectable_value(
                                &mut self.config.board_theme,
                                BoardTheme::HighContrast,
                                BoardTheme::HighContrast.name(),
                            );
                            ui.selectable_value(
                                &mut self.config.board_theme,
                                BoardTheme::Custom,
                                BoardTheme::Custom.name(),
                            );
                        });

                    if old_theme != self.config.board_theme {
                        self.config.apply_theme(self.config.board_theme);
                    }

                    ui.separator();

                    ui.checkbox(&mut self.config.center_board, "Center board");
                    ui.checkbox(&mut self.config.responsive_board, "Responsive board");

                    ui.add_enabled(
                        !self.config.responsive_board,
                        Slider::new(&mut self.config.square_size, 16.0..=240.0).text("Square size"),
                    );

                    ui.add(
                        Slider::new(&mut self.config.min_square_size, 16.0..=240.0)
                            .text("Min square"),
                    );
                    ui.add(
                        Slider::new(&mut self.config.max_square_size, 16.0..=240.0)
                            .text("Max square"),
                    );
                    ui.add(
                        Slider::new(&mut self.config.piece_scale, 0.5..=1.0).text("Piece scale"),
                    );
                    ui.add(
                        Slider::new(&mut self.config.coordinate_scale, 0.1..=0.5)
                            .text("Coordinate scale"),
                    );
                    ui.add(
                        Slider::new(&mut self.config.animation_duration, 0.0..=1.5)
                            .text("Animation duration"),
                    );

                    ui.checkbox(&mut self.config.animate_moves, "Animate moves");
                    ui.checkbox(&mut self.config.auto_flip_perspective, "Auto-flip board");
                    ui.checkbox(&mut self.config.show_coordinates, "Coordinates");
                    ui.checkbox(&mut self.config.show_legal_moves, "Legal-move dots");
                    ui.checkbox(&mut self.config.show_check_highlight, "Check highlight");
                    ui.checkbox(&mut self.config.show_last_move, "Last-move highlight");
                    ui.checkbox(&mut self.config.show_grid, "Grid lines");
                    ui.checkbox(&mut self.config.show_border, "Border");
                    ui.checkbox(&mut self.config.show_move_list, "Move list");
                    ui.checkbox(&mut self.config.show_clock, "Show clock");
                    ui.checkbox(&mut self.config.time_control.enabled, "Clock enabled");

                    ui.add(
                        Slider::new(&mut self.config.time_control.initial_secs, 10.0..=3600.0)
                            .text("Initial seconds"),
                    );
                    ui.add(
                        Slider::new(&mut self.config.time_control.increment_secs, 0.0..=30.0)
                            .text("Increment seconds"),
                    );

                    if ui.button("Reset clock now").clicked() {
                        reset_clock = true;
                    }

                    ui.separator();
                    ui.label("PGN headers");

                    ui.horizontal(|ui| {
                        ui.label("Event");
                        ui.text_edit_singleline(&mut self.config.pgn_event);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Site");
                        ui.text_edit_singleline(&mut self.config.pgn_site);
                    });

                    ui.horizontal(|ui| {
                        ui.label("White");
                        ui.text_edit_singleline(&mut self.config.pgn_white);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Black");
                        ui.text_edit_singleline(&mut self.config.pgn_black);
                    });
                });
        }

        if self.export_open {
            let mut open = self.export_open;
            let mut close_export = false;

            Window::new("Export")
                .anchor(Align2::CENTER_CENTER, (0.0, 0.0))
                .open(&mut open)
                .resizable(true)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.add(TextEdit::multiline(&mut self.export_text).desired_width(360.0));

                    if ui.button("Close").clicked() {
                        close_export = true;
                    }
                });

            if close_export {
                open = false;
            }

            self.export_open = open;
        }

        if toggle_settings {
            self.settings_open = !self.settings_open;
        }

        if new_game {
            self.restart();
        }

        if flip {
            self.flip_perspective();
        }

        if undo {
            self.undo();
        }

        if redo {
            self.redo();
        }

        if resign {
            self.resign();
        }

        if draw {
            self.offer_draw();
        }

        if reset_clock {
            self.clock = ClockState::new(&self.config.time_control);
        }

        if export_fen {
            let text = self.fen();
            self.export_text = text;
            self.export_open = true;
        }

        if export_pgn {
            let text = self.export_pgn();
            self.export_text = text;
            self.export_open = true;
        }
    }

    fn build_promotion_dialog(&mut self, ctx: &egui_macroquad::egui::Context) {
        let mut chosen: Option<ChessPiece> = None;
        let mut cancel = false;

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
                            ChessPiece::Queen => "\u{265B}",
                            ChessPiece::Rook => "\u{265C}",
                            ChessPiece::Bishop => "\u{265D}",
                            ChessPiece::Knight => "\u{265E}",
                            _ => "?",
                        };

                        if ui.button(label).clicked() {
                            chosen = Some(piece);
                        }
                    }
                });

                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });

        if let Some(piece) = chosen {
            if let Some(promo) = self.pending_promotion.take() {
                let m = ChessMove::new(promo.source, promo.dest, Some(piece));
                self.try_move(m);
            }
        }

        if cancel {
            self.pending_promotion = None;
        }
    }

    fn status_text(&self) -> String {
        if let Some(result) = self.game_result {
            if let Some(reason) = self.game_end_reason {
                format!("{result} ({reason})")
            } else {
                result.to_string()
            }
        } else {
            let side = self.board.side_to_move();
            let name = match side {
                ChessColor::White => "White",
                ChessColor::Black => "Black",
            };
            format!("{name} to move")
        }
    }

    fn move_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current = String::new();
        let mut number = 1u32;
        let mut expect_white = true;

        for rec in &self.move_records {
            match rec.color {
                ChessColor::White => {
                    if !expect_white {
                        lines.push(current.trim().to_string());
                        current.clear();
                    }

                    current = format!("{}. {}", number, rec.san);
                    expect_white = false;
                }
                ChessColor::Black => {
                    if expect_white {
                        current = format!("{}. ... {}", number, rec.san);
                    } else {
                        current.push_str(&format!(" {}", rec.san));
                    }

                    lines.push(current.trim().to_string());
                    current.clear();

                    number += 1;
                    expect_white = true;
                }
            }
        }

        if !current.is_empty() {
            lines.push(current.trim().to_string());
        }

        lines
    }

    // ── Rendering ────────────────────────────────────────────────────────

    fn draw_board(&self) {
        let Some(texture) = &self.pieces_texture else {
            draw_text("Load pieces texture first!", 100.0, 256.0, 20.0, RED);
            if let Some(err) = &self.error {
                draw_text(&format!("Error: {err}"), 100.0, 300.0, 16.0, RED);
            }
            return;
        };

        let geom = self.geometry();

        self.draw_squares_and_highlights(&geom);
        self.draw_pieces(texture, &geom);
        self.draw_animated_piece(texture, &geom);
        self.draw_dragged_piece(texture, &geom);

        if self.config.show_coordinates {
            self.draw_coordinates(&geom);
        }
    }

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

                let Some(sq) = get_square(file, rank) else {
                    continue;
                };

                if self.config.show_last_move {
                    if let Some((from, to)) = self.last_move {
                        if sq == from || sq == to {
                            draw_rectangle(
                                screen_x,
                                screen_y,
                                size,
                                size,
                                self.config.last_move_color,
                            );
                        }
                    }
                }

                if Some(sq) == self.selected_square {
                    draw_rectangle(screen_x, screen_y, size, size, self.config.selected_color);
                }

                if check_square == Some(sq) {
                    draw_rectangle(screen_x, screen_y, size, size, self.config.check_color);
                }

                if self.config.show_legal_moves && self.selected_square.is_some() {
                    if self.cached_legal_targets.contains(&sq) {
                        let cx = screen_x + size / 2.0;
                        let cy = screen_y + size / 2.0;

                        if self.board.piece_on(sq).is_some() {
                            draw_circle(
                                cx,
                                cy,
                                size * CAPTURE_RING_FRAC,
                                self.config.capture_ring_color,
                            );
                        } else {
                            draw_circle(
                                cx,
                                cy,
                                size * DOT_RADIUS_FRAC,
                                self.config.legal_move_color,
                            );
                        }
                    }
                }
            }
        }

        let board_pixels = geom.board_pixels();

        if self.config.show_grid {
            for i in 0..=BOARD_SIZE {
                let pos = i as f32 * size;

                draw_line(
                    geom.offset_x + pos,
                    geom.offset_y,
                    geom.offset_x + pos,
                    geom.offset_y + board_pixels,
                    1.0,
                    self.config.grid_color,
                );

                draw_line(
                    geom.offset_x,
                    geom.offset_y + pos,
                    geom.offset_x + board_pixels,
                    geom.offset_y + pos,
                    1.0,
                    self.config.grid_color,
                );
            }
        }

        if self.config.show_border && self.config.border_thickness > 0.0 {
            let t = self.config.border_thickness;
            let x = geom.offset_x;
            let y = geom.offset_y;

            draw_rectangle(
                x - t,
                y - t,
                board_pixels + 2.0 * t,
                t,
                self.config.border_color,
            );
            draw_rectangle(
                x - t,
                y + board_pixels,
                board_pixels + 2.0 * t,
                t,
                self.config.border_color,
            );
            draw_rectangle(x - t, y, t, board_pixels, self.config.border_color);
            draw_rectangle(
                x + board_pixels,
                y,
                t,
                board_pixels,
                self.config.border_color,
            );
        }
    }

    fn find_king_in_check(&self) -> Option<Square> {
        if *self.board.checkers() == BitBoard::default() {
            return None;
        }

        let side = self.board.side_to_move();

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
        let scaled = size * self.config.piece_scale;
        let pad = (size - scaled) / 2.0;

        for rank in 0..BOARD_SIZE {
            for file in 0..BOARD_SIZE {
                let Some(sq) = get_square(file, rank) else {
                    continue;
                };

                let Some(piece) = self.board.piece_on(sq) else {
                    continue;
                };

                if let Some((drag_sq, _, _)) = self.dragging_piece {
                    if drag_sq == sq {
                        continue;
                    }
                }

                if let Some(anim) = &self.animation {
                    if anim.to == sq {
                        continue;
                    }
                }

                let color = self.board.color_on(sq).unwrap();
                let render_piece = RenderPiece::from_chess(piece, color);
                let rect = self.piece_rects[render_piece as usize];

                let (screen_x, screen_y) = geom.square_to_screen(file, rank);

                draw_texture_ex(
                    texture,
                    screen_x + pad,
                    screen_y + pad,
                    WHITE,
                    DrawTextureParams {
                        source: Some(rect),
                        dest_size: Some(Vec2::new(scaled, scaled)),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn draw_animated_piece(&self, texture: &Texture2D, geom: &BoardGeometry) {
        let Some(anim) = &self.animation else {
            return;
        };

        let duration = anim.duration.max(0.0001);
        let t = ((get_time() - anim.start) / duration).clamp(0.0, 1.0) as f32;
        let eased = t * t * (3.0 - 2.0 * t);

        let (from_x, from_y) = geom.square_to_screen(
            anim.from.get_file().to_index() as u32,
            anim.from.get_rank().to_index() as u32,
        );

        let (to_x, to_y) = geom.square_to_screen(
            anim.to.get_file().to_index() as u32,
            anim.to.get_rank().to_index() as u32,
        );

        let x = from_x + (to_x - from_x) * eased;
        let y = from_y + (to_y - from_y) * eased;

        let size = geom.square_size;
        let scaled = size * self.config.piece_scale;
        let pad = (size - scaled) / 2.0;

        let render_piece = RenderPiece::from_chess(anim.piece, anim.color);
        let rect = self.piece_rects[render_piece as usize];

        draw_texture_ex(
            texture,
            x + pad,
            y + pad,
            WHITE,
            DrawTextureParams {
                source: Some(rect),
                dest_size: Some(Vec2::new(scaled, scaled)),
                ..Default::default()
            },
        );
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
        let size = self.current_square_size();
        let scaled = size * self.config.piece_scale;
        let pad = (size - scaled) / 2.0;

        draw_texture_ex(
            texture,
            mx - offset_x + pad,
            my - offset_y + pad,
            WHITE,
            DrawTextureParams {
                source: Some(rect),
                dest_size: Some(Vec2::new(scaled, scaled)),
                ..Default::default()
            },
        );
    }

    fn draw_coordinates(&self, geom: &BoardGeometry) {
        let size = geom.square_size;
        let font_size = size * self.config.coordinate_scale;
        let board_pixels = geom.board_pixels();

        let white = self.perspective == ChessColor::White;
        let bottom_rank = if white { 0 } else { 7 };
        let left_file = if white { 0 } else { 7 };

        for i in 0..BOARD_SIZE {
            // File labels along bottom
            let logical_file = if white { i } else { 7 - i };
            let file_label = ((b'a' + logical_file as u8) as char).to_string();

            let file_sq_color = if (logical_file + bottom_rank) % 2 == 0 {
                self.config.light_square_color
            } else {
                self.config.dark_square_color
            };

            let x_center = geom.offset_x + i as f32 * size + size / 2.0;
            let width = measure_text(&file_label, None, font_size as u16, 1.0).width;

            draw_text(
                &file_label,
                x_center - width / 2.0,
                geom.offset_y + board_pixels + font_size + 5.0,
                font_size,
                contrast_color(file_sq_color),
            );

            // Rank labels along left
            let logical_rank = if white { 7 - i } else { i };
            let rank_label = ((b'1' + logical_rank as u8) as char).to_string();

            let rank_sq_color = if (left_file + logical_rank) % 2 == 0 {
                self.config.light_square_color
            } else {
                self.config.dark_square_color
            };

            let y_center = geom.offset_y + i as f32 * size + size / 2.0 + font_size / 2.0;

            draw_text(
                &rank_label,
                geom.offset_x - font_size - 8.0,
                y_center,
                font_size,
                contrast_color(rank_sq_color),
            );
        }
    }

    // ── Public accessors ─────────────────────────────────────────────────

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn perspective(&self) -> ChessColor {
        self.perspective
    }

    pub fn fen(&self) -> String {
        self.board.to_string()
    }

    pub fn export_fen(&self) -> String {
        self.fen()
    }

    pub fn export_pgn(&self) -> String {
        let result = self
            .game_result
            .map(|r| r.pgn_result().to_string())
            .unwrap_or_else(|| "*".to_string());

        let date = utc_date_string();

        let mut headers = String::new();
        headers.push_str(&format!(
            "[Event \"{}\"]\n",
            escape_pgn(&self.config.pgn_event)
        ));
        headers.push_str(&format!(
            "[Site \"{}\"]\n",
            escape_pgn(&self.config.pgn_site)
        ));
        headers.push_str(&format!("[Date \"{}\"]\n", date));
        headers.push_str("[Round \"1\"]\n");
        headers.push_str(&format!(
            "[White \"{}\"]\n",
            escape_pgn(&self.config.pgn_white)
        ));
        headers.push_str(&format!(
            "[Black \"{}\"]\n",
            escape_pgn(&self.config.pgn_black)
        ));
        headers.push_str(&format!("[Result \"{}\"]\n\n", result));

        let tokens = self.pgn_tokens();
        let body = wrap_pgn(&tokens, &result);

        format!("{headers}{body}")
    }

    fn pgn_tokens(&self) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut number = 1u32;
        let mut expect_white = true;

        for rec in &self.move_records {
            match rec.color {
                ChessColor::White => {
                    if !expect_white {
                        tokens.push("...".to_string());
                    }

                    tokens.push(format!("{}. {}", number, rec.san));
                    expect_white = false;
                }
                ChessColor::Black => {
                    if expect_white {
                        tokens.push(format!("{}. ...", number));
                    }

                    tokens.push(rec.san.clone());
                    number += 1;
                    expect_white = true;
                }
            }
        }

        tokens
    }

    pub fn board_offset(&self) -> (f32, f32) {
        self.get_board_offset()
    }

    pub fn square_size(&self) -> f32 {
        self.current_square_size()
    }

    pub fn game_result(&self) -> Option<GameResult> {
        self.game_result
    }

    pub fn game_end_reason(&self) -> Option<GameEndReason> {
        self.game_end_reason
    }

    pub fn status_message(&self) -> &str {
        &self.status_message
    }

    pub fn last_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn legal_moves(&self) -> Vec<ChessMove> {
        chess::MoveGen::new_legal(&self.board).collect()
    }

    pub fn move_records(&self) -> &[MoveRecord] {
        &self.move_records
    }

    pub fn clock_text(&self, color: ChessColor) -> String {
        self.clock.formatted(color)
    }

    // ── Public mutators ──────────────────────────────────────────────────

    pub fn set_fen(&mut self, fen: &str) -> Result<(), ChessError> {
        let board = Board::from_str(fen).map_err(|e| ChessError::InvalidFen(e.to_string()))?;

        self.stop_uci_search();

        self.board = board;
        self.reset_transient();
        self.reset_history_and_clocks();

        self.perspective = self.board.side_to_move();

        Ok(())
    }

    pub fn set_board(&mut self, board: Board) {
        self.stop_uci_search();

        self.board = board;
        self.reset_transient();
        self.reset_history_and_clocks();

        self.perspective = self.board.side_to_move();
    }

    fn position_count_current(&self) -> usize {
        let current = position_key(&self.board);
        self.position_history
            .iter()
            .filter(|k| k.as_str() == current)
            .count()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Free helper functions
// ──────────────────────────────────────────────────────────────────────────────

#[inline]
pub(crate) fn get_square(file: u32, rank: u32) -> Option<Square> {
    if file >= BOARD_SIZE || rank >= BOARD_SIZE {
        return None;
    }

    let f = ALL_FILES[file as usize];
    let r = ALL_RANKS[rank as usize];

    Some(Square::make_square(r, f))
}

fn contrast_color(color: Color) -> Color {
    let luminance = 0.299 * color.r + 0.587 * color.g + 0.114 * color.b;

    if luminance > 0.5 {
        BLACK
    } else {
        WHITE
    }
}

fn check_board_status_result(board: &Board) -> Option<(GameResult, GameEndReason)> {
    match board.status() {
        BoardStatus::Checkmate => {
            let winner = opposite_color(board.side_to_move());
            let result = match winner {
                ChessColor::White => GameResult::WhiteWins,
                ChessColor::Black => GameResult::BlackWins,
            };

            Some((result, GameEndReason::Checkmate))
        }
        BoardStatus::Stalemate => Some((GameResult::Draw, GameEndReason::Stalemate)),
        BoardStatus::Ongoing => None,
    }
}

fn insufficient_material(board: &Board) -> bool {
    let mut minors: Vec<(ChessPiece, Square)> = Vec::new();

    for rank in ALL_RANKS {
        for file in ALL_FILES {
            let sq = Square::make_square(rank, file);

            if let Some(piece) = board.piece_on(sq) {
                match piece {
                    ChessPiece::King => {}
                    ChessPiece::Bishop | ChessPiece::Knight => minors.push((piece, sq)),
                    _ => return false,
                }
            }
        }
    }

    match minors.len() {
        0 => true,
        1 => true,
        _ => {
            let all_bishops = minors.iter().all(|(p, _)| *p == ChessPiece::Bishop);
            if !all_bishops {
                return false;
            }

            let first_light = square_is_light(minors[0].1);
            minors
                .iter()
                .all(|(_, sq)| square_is_light(*sq) == first_light)
        }
    }
}

fn square_is_light(sq: Square) -> bool {
    (sq.get_file().to_index() + sq.get_rank().to_index()) % 2 == 0
}

fn position_key(board: &Board) -> String {
    board
        .to_string()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
}

fn file_index_char(i: usize) -> char {
    (b'a' + i as u8) as char
}

fn rank_index_char(i: usize) -> char {
    (b'1' + i as u8) as char
}

fn square_name(sq: Square) -> String {
    format!(
        "{}{}",
        file_index_char(sq.get_file().to_index()),
        rank_index_char(sq.get_rank().to_index())
    )
}

fn piece_char(piece: ChessPiece) -> Option<char> {
    match piece {
        ChessPiece::King => Some('K'),
        ChessPiece::Queen => Some('Q'),
        ChessPiece::Rook => Some('R'),
        ChessPiece::Bishop => Some('B'),
        ChessPiece::Knight => Some('N'),
        ChessPiece::Pawn => None,
    }
}

fn san_for_move(board: &Board, m: ChessMove) -> String {
    let source = m.get_source();
    let dest = m.get_dest();

    let Some(piece) = board.piece_on(source) else {
        return move_to_uci(m);
    };

    let file_diff = dest.get_file().to_index() as i32 - source.get_file().to_index() as i32;

    let is_castle = piece == ChessPiece::King && file_diff.abs() == 2;

    let mut san = if is_castle {
        if dest.get_file().to_index() > source.get_file().to_index() {
            "O-O".to_string()
        } else {
            "O-O-O".to_string()
        }
    } else {
        let is_capture = board.piece_on(dest).is_some()
            || (piece == ChessPiece::Pawn && source.get_file() != dest.get_file());

        let dest_name = square_name(dest);

        match piece {
            ChessPiece::Pawn => {
                if is_capture {
                    format!(
                        "{}x{}",
                        file_index_char(source.get_file().to_index()),
                        dest_name
                    )
                } else {
                    dest_name
                }
            }
            _ => {
                let mut s = String::new();

                if let Some(c) = piece_char(piece) {
                    s.push(c);
                }

                s.push_str(&disambiguation(board, m, piece));

                if is_capture {
                    s.push('x');
                }

                s.push_str(&dest_name);
                s
            }
        }
    };

    if let Some(promo) = m.get_promotion() {
        if let Some(c) = piece_char(promo) {
            san.push('=');
            san.push(c);
        }
    }

    let next = board.make_move_new(m);

    if next.status() == BoardStatus::Checkmate {
        san.push('#');
    } else if *next.checkers() != BitBoard::default() {
        san.push('+');
    }

    san
}

fn disambiguation(board: &Board, m: ChessMove, piece: ChessPiece) -> String {
    let source = m.get_source();
    let dest = m.get_dest();

    let others: Vec<Square> = chess::MoveGen::new_legal(board)
        .filter(|other| {
            other.get_dest() == dest
                && other.get_source() != source
                && board.piece_on(other.get_source()) == Some(piece)
        })
        .map(|other| other.get_source())
        .collect();

    if others.is_empty() {
        return String::new();
    }

    let same_file = others.iter().any(|s| s.get_file() == source.get_file());
    let same_rank = others.iter().any(|s| s.get_rank() == source.get_rank());

    let file_c = file_index_char(source.get_file().to_index());
    let rank_c = rank_index_char(source.get_rank().to_index());

    if !same_file {
        file_c.to_string()
    } else if !same_rank {
        rank_c.to_string()
    } else {
        format!("{file_c}{rank_c}")
    }
}

fn move_to_uci(m: ChessMove) -> String {
    let mut s = square_name(m.get_source());
    s.push_str(&square_name(m.get_dest()));

    if let Some(promo) = m.get_promotion() {
        let c = match promo {
            ChessPiece::Queen => Some('q'),
            ChessPiece::Rook => Some('r'),
            ChessPiece::Bishop => Some('b'),
            ChessPiece::Knight => Some('n'),
            _ => None,
        };

        if let Some(c) = c {
            s.push(c);
        }
    }

    s
}

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

fn escape_pgn(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn wrap_pgn(tokens: &[String], result: &str) -> String {
    let mut out = String::new();
    let mut len = 0usize;

    for token in tokens {
        if len + token.len() + 1 > 80 {
            out.push('\n');
            len = 0;
        }

        out.push_str(token);
        out.push(' ');
        len += token.len() + 1;
    }

    out.push_str(result);
    out
}

fn utc_date_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;

    let (year, month, day) = civil_from_days(days);
    format!("{:04}.{:02}.{:02}", year, month, day)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };

    if m <= 2 {
        (y + 1, m as u32, d as u32)
    } else {
        (y, m as u32, d as u32)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_game_is_ongoing() {
        let board = Board::default();
        assert!(check_board_status_result(&board).is_none());
        assert!(!insufficient_material(&board));
    }

    #[test]
    fn config_builder_validates() {
        let cfg = ChessConfig::builder().square_size(-10.0).build();
        assert!(cfg.square_size >= 16.0);
    }

    #[test]
    fn san_e4() {
        let board = Board::default();
        let m = ChessMove::new(get_square(4, 1).unwrap(), get_square(4, 3).unwrap(), None);

        assert_eq!(san_for_move(&board, m), "e4");
    }

    #[test]
    fn position_key_is_stable() {
        let board = Board::default();
        assert_eq!(position_key(&board), position_key(&board));
    }

    #[test]
    fn clock_formats() {
        let tc = TimeControl {
            enabled: true,
            initial_secs: 90.0,
            increment_secs: 0.0,
        };

        let clock = ClockState::new(&tc);
        assert_eq!(clock.formatted(ChessColor::White), "1:30");
    }
}
