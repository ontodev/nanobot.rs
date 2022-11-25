use clap::{command, Command};

fn main() {
    let matches = command!() // requires `cargo` feature
        .propagate_version(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("init")
                .about("Initialises things")
        )
        .get_matches();

    match matches.subcommand() {
        Some(("init", _sub_matches)) => println!("Hello world"),
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    }
}
