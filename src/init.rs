use crate::config::{to_toml, Config, LoggingLevel};
use ontodev_valve::valve::Valve;
use std::error;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{prelude::*, BufReader};
use std::path::Path;

fn add_to_gitignore(input: &str) -> Result<String, String> {
    if Path::new(".gitignore").exists() {
        let file = match File::open(".gitignore") {
            Ok(f) => f,
            Err(e) => return Err(e.to_string()),
        };
        let reader = BufReader::new(file);

        let mut found = false; //if true, then input is already in .gitignore
        let mut last_line_empty = false;
        let mut modified = false; //if true, then nanobot has already modified .gitignore

        for line in reader.lines() {
            let string = match line {
                Ok(l) => l,
                Err(e) => return Err(e.to_string()),
            };
            if string.contains(input) {
                found = true;
            }
            if string.trim().eq("") {
                last_line_empty = true;
            } else {
                last_line_empty = false;
            }
            if string.contains("Generated by nanobot") {
                modified = true;
            }
        }

        if !found {
            if modified {
                //insert input in .gitignore
                //where nanobot has already modified the file previously
                let mut file = match OpenOptions::new().write(true).open(".gitignore") {
                    Ok(f) => f,
                    Err(e) => return Err(e.to_string()),
                };

                let file_string = match fs::read_to_string(".gitignore") {
                    Ok(f) => f,
                    Err(e) => return Err(e.to_string()),
                };
                let mut file_lines: Vec<&str> = file_string.split("\n").collect();
                for (pos, line) in file_lines.clone().iter().enumerate() {
                    if line.contains("Generated by nanobot") {
                        file_lines.insert(pos + 1, input);
                    }
                }
                let file_res = file_lines.join("\n");

                if let Err(e) = write!(file, "{}", file_res) {
                    eprintln!("Couldn't write to file: {}", e);
                }
                Ok(String::from("NotFound-Modified"))
            } else {
                let mut file = match OpenOptions::new()
                    .write(true)
                    .append(true)
                    .open(".gitignore")
                {
                    Ok(f) => f,
                    Err(e) => return Err(e.to_string()),
                };

                if !last_line_empty {
                    if let Err(e) = writeln!(file, "") {
                        eprintln!("Couldn't write to file: {}", e);
                    }
                }

                if let Err(e) = writeln!(file, "# Generated by nanobot") {
                    eprintln!("Couldn't write to file: {}", e);
                }

                if let Err(e) = writeln!(file, "{}", input) {
                    eprintln!("Couldn't write to file: {}", e);
                }
                Ok(String::from("NotFound-NotModified"))
            }
        } else {
            //input is already in .gitignore
            Ok(String::from("Found"))
        }
    } else {
        Ok(String::from("No .gitignore"))
    }
}

fn create_table_tsv(path: &Path) -> Result<(), Box<dyn error::Error>> {
    let data = include_str!("resources/table.tsv");
    fs::write(&path, data).expect("Unable to write file");
    Ok(())
}

fn create_column_tsv(path: &Path) -> Result<(), Box<dyn error::Error>> {
    let data = include_str!("resources/column.tsv");
    fs::write(&path, data).expect("Unable to write file");
    Ok(())
}

fn create_datatype_tsv(path: &Path) -> Result<(), Box<dyn error::Error>> {
    let data = include_str!("resources/datatype.tsv");
    fs::write(&path, data).expect("Unable to write file");
    Ok(())
}

pub async fn init(config: &mut Config) -> Result<String, String> {
    // Create nanobot.toml if it does not exist.
    let path = Path::new("nanobot.toml");
    if !path.exists() {
        // Create default config nanobot.toml
        let path = Path::new("nanobot.toml");
        let toml = to_toml(config);
        match toml.write_non_defaults(&path) {
            Err(_) => return Err(format!("Could not create '{}'", path.display())),
            _ => (),
        };
        tracing::info!("Created config file '{}'", path.display());
    }

    // Create files for the basic VALVE schema tables, if they don't exist
    let valve_path = &config.valve_path;
    let path = Path::new(valve_path).parent().unwrap();
    if !path.exists() {
        match fs::create_dir_all(&path) {
            Err(_x) => return Err(format!("Could not create '{}'", path.display())),
            Ok(_x) => {}
        };
        tracing::info!("Created '{}' directory", path.display());
    }

    let path = Path::new(valve_path);
    if !path.exists() {
        match create_table_tsv(&path) {
            Err(_x) => return Err(format!("Could not create '{}'", path.display())),
            Ok(_x) => {}
        };
        tracing::info!("Created '{}' file", path.display());
    }

    let path = Path::new(valve_path).parent().unwrap().join("column.tsv");
    if !path.exists() {
        match create_column_tsv(&path.as_path()) {
            Err(_x) => return Err(format!("Could not create '{}'", path.display())),
            Ok(_x) => {}
        };
        tracing::info!("Created '{}' file", path.display());
    }

    let path = Path::new(valve_path).parent().unwrap().join("datatype.tsv");
    if !path.exists() {
        match create_datatype_tsv(&path.as_path()) {
            Err(_x) => return Err(format!("Could not create '{}'", path.display())),
            Ok(_x) => {}
        };
        tracing::info!("Created '{}' file", path.display());
    }

    //create database file
    let database = config.connection.to_owned();
    let path = Path::new(&database);
    if !path.exists() {
        match File::create(&database) {
            Err(_x) => return Err(String::from("Couldn't create database")),
            Ok(_x) => {}
        }
    }

    //add database file to .gitignore
    match add_to_gitignore(format!("{}*", &database).as_str()) {
        Err(x) => return Err(x),
        Ok(_x) => {}
    }

    (config.valve, config.pool) = {
        let mut valve = Valve::build(&valve_path, &config.connection)
            .await
            .expect(&format!(
                "VALVE failed to load configuration for '{}'",
                valve_path
            ));
        if config.logging_level == LoggingLevel::DEBUG {
            valve.set_verbose(true);
        }
        let pool = valve.pool.clone();
        (Some(valve), Some(pool))
    };

    // Create and/or load tables into database
    match &config.valve {
        None => unreachable!("Valve is not initialized."),
        Some(valve) => {
            if config.create_only {
                if let Err(e) = valve.ensure_all_tables_created().await {
                    return Err(format!(
                        "VALVE error while creating from {}: {:?}",
                        valve_path, e
                    ));
                }
            } else {
                if let Err(e) = valve.load_all_tables(true).await {
                    return Err(format!(
                        "VALVE error while loading from {}: {:?}",
                        valve_path, e
                    ));
                }
            }
        }
    };

    tracing::info!("Initialized '{}' using '{}'", database, valve_path);
    Ok(String::from("Initialized a Nanobot project"))
}
