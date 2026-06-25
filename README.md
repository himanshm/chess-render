# chess-render

A configurable, embeddable chess GUI built with [Macroquad](https://macroquad.rs/) and the [chess](https://crates.io/crates/chess) crate.

This library provides a ready‑to‑use chess board widget that handles rendering, move validation, local two‑player play, optional UCI engine integration, and endgame detection—all in a single, easy‑to‑embed component.

---

## Features

- **Rendering** – draws the board and pieces from a texture atlas (includes a high‑quality default sprite sheet).
- **Move validation** – powered by the robust [`chess`](https://crates.io/crates/chess) crate.
- **Two‑player local play** – automatic board flip after each move (optional).
- **UCI engine support** – integrate external chess engines via the optional `uci` feature.
- **Endgame detection** – detects checkmate, stalemate, and draws, with a clear status message.
- **Usability** – last‑move highlighting, move‑target indicators, promotion pop‑up, and a "New Game" button.
- **Customizable** – board colors, square size, piece texture path, coordinate labels, auto‑flip behaviour, and more.
- **Built‑in assets** – default pieces are embedded in the binary – no external files required.

---

## Installation

Add `chess-render` to your `Cargo.toml`:

```toml
[dependencies]
chess-render = "0.3.3"
```

To enable UCI engine integration (required for communicating with external chess engines), add the `uci` feature:

```toml
[dependencies]
chess-render = { version = "0.3.3", features = ["uci"] }
```

---

## Quick Start

The simplest way to get a chess board on screen:

```rust
use chess_render::{ChessConfig, ChessGui};
use macroquad::prelude::*;

#[macroquad::main("Chess Render")]
async fn main() {
    let config = ChessConfig::default();
    let mut gui = ChessGui::new(config);
    gui.load_pieces().await.expect("Failed to load piece texture");

    loop {
        gui.update().await;
        next_frame().await;
    }
}
```

This will display a standard chess board with the built‑in piece set, centered in the window.

---

## Configuration

The `ChessConfig` struct allows you to customise almost every aspect of the GUI.

| Field                    | Type          | Default                            | Description |
|--------------------------|---------------|------------------------------------|-------------|
| `light_square_color`     | `Color`       | `(255, 253, 208)`                  | Colour of light squares. |
| `dark_square_color`      | `Color`       | `GRAY`                             | Colour of dark squares. |
| `square_size`            | `f32`         | `64.0`                             | Size of each square in pixels. |
| `board_offset`           | `(f32, f32)`  | `(0.0, 0.0)`                       | Manual offset from top‑left (ignored if `center_board` is true). |
| `center_board`           | `bool`        | `true`                             | Automatically centre the board in the window. |
| `auto_flip_perspective`  | `bool`        | `true`                             | Flip the board after each move so the player always sees their own side. |
| `promotion_piece`        | `ChessPiece`  | `Queen`                            | Default piece to promote to when the promotion dialog is not shown (or as a fallback). |
| `piece_texture_path`     | `Option<String>` | `None`                          | Path to a custom piece texture atlas. If `None`, the built‑in default is used. |
| `show_coordinates`       | `bool`        | `true`                             | Show file (a‑h) and rank (1‑8) labels. |
| `uci_engine_path`        | `Option<String>` | `None` (requires `uci` feature) | Path to the UCI engine executable. |
| `uci_plays_as`           | `ChessColor`  | `Black` (requires `uci` feature)   | Which side the engine plays for. |
| `uci_move_time_ms`       | `u64`         | `1000` (requires `uci` feature)    | Time in milliseconds the engine is allowed per move. |

### Customising Board Colours

```rust
use macroquad::prelude::*;

let config = ChessConfig {
    light_square_color: Color::new(0.9, 0.9, 0.8, 1.0),
    dark_square_color: Color::new(0.4, 0.4, 0.4, 1.0),
    ..Default::default()
};
```

### Using a Custom Piece Texture

Provide a path to your own 384×128 PNG sprite sheet.

**Texture layout:**
- Row 0 (y=0): White pieces (King, Queen, Bishop, Knight, Rook, Pawn)
- Row 1 (y=64): Black pieces (same order)
- Each piece is 64×64 pixels.

```rust
let config = ChessConfig {
    piece_texture_path: Some("assets/my_pieces.png".to_string()),
    ..Default::default()
};
```

---

## UCI Engine Integration

Enable the `uci` feature and configure the engine path.

```rust
#[cfg(feature = "uci")]
let config = ChessConfig {
    uci_engine_path: Some("stockfish".to_string()), // or full path
    uci_plays_as: ChessColor::Black,
    uci_move_time_ms: 2000,
    ..Default::default()
};
```

The engine will start automatically when `load_pieces()` is called. It will move when it's its turn, provided the board is not in a terminal state.

---

## Public API

### `ChessGui`

- `new(config: ChessConfig) -> Self` – create a new GUI instance.
- `async fn load_pieces(&mut self) -> Result<(), ChessError>` – load the texture (must be called before rendering).
- `async fn update(&mut self)` – call this every frame to render and handle input.
- `fn try_move(&mut self, m: ChessMove) -> bool` – attempt to make a move; returns `true` if legal.
- `fn set_fen(&mut self, fen: &str) -> Result<(), ChessError>` – set the board position from a FEN string.
- `fn set_board(&mut self, board: Board)` – set the board directly.
- `fn board(&self) -> &Board` – get a reference to the current board.
- `fn perspective(&self) -> ChessColor` – get the current player’s perspective (colour at bottom).
- `fn fen(&self) -> String` – export the current position as FEN.
- `fn legal_moves(&self) -> Vec<ChessMove>` – get all legal moves from the current position.

### `ChessConfig`

All fields are public; use `..Default::default()` to fill omitted values.

### `GameResult`

Enum returned when the game ends:
- `WhiteWins`
- `BlackWins`
- `Draw`

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `R` | Restart game (New Game) |
| `F` | Flip the board perspective |

These shortcuts work even when the egui UI does not have focus.

---

## License & Attribution

### Code
The source code of this library is licensed under the **MIT License**.

### Default Chess Pieces
The default piece sprites embedded in the binary are adapted from work by **Cburnett** and **jurgenwesterhof** and are licensed under **CC BY-SA 3.0**.

If you use the default pieces in your project, please include the following attribution:

> **Chess Pieces**  
> By jurgenwesterhof (adapted from work of Cburnett) – [Template:SVG chess pieces](http://commons.wikimedia.org/wiki/Template:SVG_chess_pieces), [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0), [Link](https://commons.wikimedia.org/w/index.php?curid=35634436)

---

## Contributing

Issues and pull requests are welcome. Please ensure that any changes are accompanied by appropriate tests and documentation.

---

## Acknowledgements

- [chess](https://crates.io/crates/chess) – move generation and validation.
- [Macroquad](https://macroquad.rs/) – simple and fast 2D rendering.
- [egui](https://www.egui.rs/) – immediate mode GUI for the promotion dialog and controls.
- [uci](https://crates.io/crates/uci) – UCI protocol implementation.
