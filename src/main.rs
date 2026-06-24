use chess_render::{ChessConfig, ChessGui};
use macroquad::prelude::*;

#[macroquad::main("Chess Render")]
async fn main() {
    // 1. Just use the default configuration!
    // The built-in pieces will load automatically.
    let config = ChessConfig::default();

    let mut gui = ChessGui::new(config);
    gui.load_pieces().await;

    loop {
        gui.update().await;
        next_frame().await;
    }
}
