use clap::{command, Command};
use std::path::Path;
use std::fs;


fn init() -> Result<&'static str,&'static str> {

    if Path::new("nanobot/nanobot.toml").exists() {
        Err("nanobot.toml file already exists.")
    } else { 
        fs::copy("src/resources/default_config.toml", "nanobot/nanobot.toml").unwrap(); 
        Ok("Hello world")
    }
}

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

    let exit_result = match matches.subcommand() {
        Some(("init", _sub_matches)) => init() ,
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    };

    //print exit message
    match exit_result {
        Err(x) => println!("{}", x),
        Ok(x) => println!("{}", x), 
    }
}
