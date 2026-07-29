# chess-render

A configurable, embeddable chess GUI built with [Macroquad](https://macroquad.rs/) and the [`chess`](https://crates.io/crates/chess) crate.

This library provides a ready-to-use chess board widget with rendering, move validation, local play, optional UCI engine integration, clocks, undo/redo, PGN export, and more.

---

## Features

- Rendering from a texture atlas
- Built-in default piece sprites
- Legal move validation via the `chess` crate
- Local two-player play
- Optional UCI engine integration
- Undo / redo
- Move history in SAN
- PGN export
- FEN export
- Chess clocks with increment
- Resign and draw buttons
- Draw detection:
  - checkmate
  - stalemate
  - insufficient material
  - fifty-move rule
  - threefold repetition
- Board themes
- Responsive board sizing
- Move animation
- Last-move highlighting
- Legal-move dots
- Capture indicators
- King-in-check highlighting
- Promotion popup
- Coordinate labels
- egui-based control panel

---

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
chess-render = "0.5.0"
```

To enable UCI support:

```toml
[dependencies]
chess-render = { version = "0.5.0", features = ["uci"] }
```

---

## Quick Start

```rust
use chess_render::{ChessConfig, ChessGui};
use macroquad::prelude::*;

#[macroquad::main("Chess Render")]
async fn main() {
    let config = ChessConfig::default();
    let mut gui = ChessGui::new(config);

    gui.load_pieces()
        .await
        .expect("Failed to load piece texture");

    loop {
        gui.update().await;
        next_frame().await;
    }
}
```

---

## Example with More Features

```rust
use chess_render::{ChessConfig, ChessGui};
use macroquad::prelude::*;

#[macroquad::main("Chess Render")]
async fn main() {
    let config = ChessConfig::builder()
        .square_size(72.0)
        .animate_moves(true)
        .show_move_list(true)
        .show_clock(true)
        .clock(300.0, 2.0)
        .build();

    let mut gui = ChessGui::new(config);

    gui.load_pieces()
        .await
        .expect("Failed to load piece texture");

    loop {
        gui.update().await;
        next_frame().await;
    }
}
```

---

## UCI Engine Example

Enable the `uci` feature and set an engine path.

```rust
use chess_render::{ChessConfig, ChessGui, EngineSide};
use macroquad::prelude::*;

#[macroquad::main("Chess Render")]
async fn main() {
    let config = ChessConfig::builder()
        .uci_engine_path("/usr/bin/stockfish")
        .engine_plays_as(EngineSide::Black)
        .uci_move_time_ms(500)
        .build();

    let mut gui = ChessGui::new(config);

    gui.load_pieces()
        .await
        .expect("Failed to initialize GUI");

    loop {
        gui.update().await;
        next_frame().await;
    }
}
```

You can also run the included demo with:

```bash
CHESS_ENGINE=/usr/bin/stockfish cargo run --example demo --features uci
```

---

## Configuration

`ChessConfig` is highly customizable.

You can use:

```rust
let config = ChessConfig::builder()
    .square_size(80.0)
    .responsive_board(true)
    .animate_moves(true)
    .show_grid(true)
    .clock(180.0, 2.0)
    .build();
```

or construct it directly and use `..Default::default()`.

---

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `R` | New game |
| `F` | Flip board |
| `U` | Undo |
| `Y` | Redo |

Shortcuts are ignored when egui keyboard focus is active.

---

## Public API Highlights

### `ChessGui`

- `new(config: ChessConfig) -> Self`
- `async load_pieces(&mut self) -> Result<(), ChessError>`
- `async update(&mut self)`
- `try_move(&mut self, m: ChessMove) -> bool`
- `undo(&mut self)`
- `redo(&mut self)`
- `resign(&mut self)`
- `offer_draw(&mut self)`
- `set_fen(&mut self, fen: &str) -> Result<(), ChessError>`
- `set_board(&mut self, board: Board)`
- `board(&self) -> &Board`
- `fen(&self) -> String`
- `export_pgn(&self) -> String`
- `legal_moves(&self) -> Vec<ChessMove>`
- `move_records(&self) -> &[MoveRecord]`
- `game_result(&self) -> Option<GameResult>`
- `game_end_reason(&self) -> Option<GameEndReason>`

---

## Texture Atlas Layout

Default piece texture size:

```text
384 × 128
```

Layout:

- Row 0: White pieces
- Row 1: Black pieces

Piece order:

```text
King, Queen, Bishop, Knight, Rook, Pawn
```

Each tile is:

```text
64 × 64
```

---

## License

Code: MIT

Default chess pieces are adapted from work by Cburnett and jurgenwesterhof, licensed under CC BY-SA 3.0.

Attribution:

> Chess Pieces  
> By jurgenwesterhof (adapted from work of Cburnett) – Template:SVG chess pieces, CC BY-SA 3.0

---

## Contributing

Pull requests and issues are welcome.

If you add features, please also:

- update documentation
- add tests where practical
- keep the API ergonomic
