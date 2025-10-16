use broadcast_server::cli::{
    self,
    Commands::{Connect, Start},
};

use broadcast_server::server;
use smol;

fn main() {
    match cli::read_args() {
        Start => {
            println!("Starting the server..");
            // start server module
            let _ = smol::block_on(server::start());
        }
        Connect => {
            println!("Starting client session..");
            // start client module
        }
    };
}

// https://docs.rs/async-tungstenite/0.31.0/async_tungstenite/
// https://docs.rs/clap/4.5.48/clap/
// https://docs.rs/smol/latest/smol/
// https://docs.rs/async-broadcast/latest/async_broadcast/
// https://docs.rs/flume/0.11.1/flume/
