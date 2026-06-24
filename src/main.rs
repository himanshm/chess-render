use chess_render::{ChessConfig, ChessGui};
use macroquad::prelude::*;

#[macroquad::main("Chess Render")]
async fn main() {
    // Use default config, but you can set the engine path here
    let mut config = ChessConfig::default();

    // If you have a UCI engine, uncomment and set the path
    // #[cfg(feature = "uci")]
    // config.uci_engine_path = Some("/usr/bin/stockfish".to_string());
    // config.uci_move_time_ms = 500; // half a second per move

    let mut gui = ChessGui::new(config);
    gui.load_pieces().await;

    loop {
        gui.update().await;
        next_frame().await;
    }
}
