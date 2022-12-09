use clap::{arg, command, value_parser, Command};
pub mod config;
pub mod init;

#[async_std::main]
async fn main() {
    let matches = command!() // requires `cargo` feature
        .propagate_version(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("init").about("Initialises things").arg(
                arg!(
                    -d --database <FILE> "Specifies a custom database name"
                )
                .required(false)
                .value_parser(value_parser!(String)),
            ),
        )
        .subcommand(Command::new("config").about("Configures things"))
        .get_matches();

    let exit_result = match matches.subcommand() {
        Some(("init", sub_matches)) => match sub_matches.get_one::<String>("database") {
            Some(x) => init::init(x).await,
            _ => init::init(".nanobot.db").await,
        },
        Some(("config", _sub_matches)) => config::config("nanobot.toml"),
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    };

    //print exit message
    match exit_result {
        Err(x) => {
            println!("{}", x);
            std::process::exit(1)
        }

        Ok(x) => println!("{}", x),
    }
}
