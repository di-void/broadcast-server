use broadcast_server::cli::{
    self,
    Commands::{Connect, Start},
};

fn main() {
    match cli::read_args() {
        Start => {
            println!("Starting the server..")
        }
        Connect => {
            println!("Starting client session..");
        }
    };
}

// https://docs.rs/async-tungstenite/0.31.0/async_tungstenite/
// https://docs.rs/clap/4.5.48/clap/
// https://docs.rs/smol/latest/smol/
