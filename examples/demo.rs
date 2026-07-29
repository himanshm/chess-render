use chess_render::{ChessConfig, ChessGui};
use macroquad::prelude::*;

#[cfg(feature = "uci")]
use chess_render::EngineSide;

#[macroquad::main("Chess Render")]
async fn main() {
    env_logger::init();

    let builder = ChessConfig::builder()
        .square_size(72.0)
        .animate_moves(true)
        .show_move_list(true)
        .show_clock(true)
        .clock(300.0, 2.0);

    // If compiled with UCI support and CHESS_ENGINE is set, use it.
    // Example:
    //   CHESS_ENGINE=/usr/bin/stockfish cargo run --example demo --features uci
    #[cfg(feature = "uci")]
    if let Ok(path) = std::env::var("CHESS_ENGINE") {
        builder = builder
            .uci_engine_path(path)
            .engine_plays_as(EngineSide::Black)
            .uci_move_time_ms(500);
    }

    let config = builder.build();
    let mut gui = ChessGui::new(config);

    if let Err(e) = gui.load_pieces().await {
        eprintln!("Failed to initialize chess GUI: {e}");
    }

    loop {
        gui.update().await;
        next_frame().await;
    }
}
