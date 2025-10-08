use clap::Command;

pub enum Commands {
    Start,
    Connect,
}

pub fn read_args() -> Commands {
    let matches = Command::new("broadcast-server")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("start").about("Starts the broadcast server on port 8080"))
        .subcommand(
            Command::new("connect").about("Tries to connect to the broadcast server on port 8080"),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("start", _)) => Commands::Start,
        Some(("connect", _)) => Commands::Connect,
        _ => unreachable!("subcommand_required prevents `None`"),
    }
}
