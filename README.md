# chess-render

A configurable, embeddable chess GUI built with [Macroquad](https://macroquad.rs/) and the [chess](https://crates.io/crates/chess) crate.

## Features

- **Valid move enforcement**: Powered by the robust `chess` crate.
- **Two-player local play**: Automatic board flipping after each move.
- **Built-in Default Assets**: Comes with high-quality SVG-based chess pieces embedded in the binary. No external files required to start!
- **Customizable appearance**: Easily swap out board colors or provide your own piece sprite sheet.
- **Endgame detection**: Automatically detects checkmate, stalemate, and draws.
- **Optional UCI support**: Enable the `uci` feature to integrate logic for external chess engines.

## Installation

Add `chess-render` to your `Cargo.toml`:

```toml
[dependencies]
chess-render = "0.1.0"
```

If you want to include UCI engine integration logic, enable the `uci` feature:

```toml
[dependencies]
chess-render = { version = "0.1.0", features = ["uci"] }
```

## Usage

### Quick Start (Default Pieces)

The library includes a default set of chess pieces. You can start rendering immediately without any configuration:

```rust
use chess_render::{ChessConfig, ChessGui};
use macroquad::prelude::*;

#[macroquad::main("Chess Render")]
async fn main() {
    // Uses default colors and built-in pieces
    let config = ChessConfig::default();
    
    let mut gui = ChessGui::new(config);
    gui.load_pieces().await;

    loop {
        gui.update().await;
        next_frame().await;
    }
}
```

### Customizing Pieces

If you prefer your own piece sprites, you can provide a path to a custom texture atlas. 

**Texture Specification:**
- **Format**: PNG
- **Dimensions**: 384x128 pixels
- **Layout**: 
  - **Row 0 (y=0)**: White Pieces (Left to Right: King, Queen, Bishop, Knight, Rook, Pawn)
  - **Row 1 (y=64)**: Black Pieces (Left to Right: King, Queen, Bishop, Knight, Rook, Pawn)
  - Each piece is **64x64 pixels**.

```rust
let config = ChessConfig {
    piece_texture_path: Some("assets/my_custom_pieces.png".to_string()),
    ..Default::default()
};
```

### Customizing Board Colors

You can also change the look of the board squares:

```rust
use macroquad::prelude::*;

let config = ChessConfig {
    light_square_color: Color::new(0.9, 0.9, 0.8, 1.0), // Cream
    dark_square_color: Color::new(0.4, 0.4, 0.4, 1.0),  // Dark Gray
    ..Default::default()
};
```

## License & Attribution

### Code
The source code of this library is licensed under the **MIT License**.

### Artwork
The default chess pieces embedded in this library are adapted from work by **Cburnett** and **jurgenwesterhof**. They are licensed under **CC BY-SA 3.0**.

If you use the default pieces in your project, please include the following attribution:

> **Chess Pieces**
> By jurgenwesterhof (adapted from work of Cburnett) - [Template:SVG chess pieces](http://commons.wikimedia.org/wiki/Template:SVG_chess_pieces), [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0), [Link](https://commons.wikimedia.org/w/index.php?curid=35634436)
