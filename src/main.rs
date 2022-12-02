use clap::{arg, command, value_parser, Command};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use std::fs;
use std::io::Write;
use std::path::Path;
use tabwriter::TabWriter;
use toml::map::Map;
use toml::Value;

fn init() -> Result<String, String> {
    if Path::new("nanobot.toml").exists() {
        Err(String::from("nanobot.toml file already exists."))
    } else {
        let toml = r#"[tool]
name = "nanobot"
version = "0.1.0"
edition = "2021"
"#;
        fs::write("nanobot.toml", toml).expect("Unable to write file");

        Ok(String::from("Initialized a Nanobot project"))
    }
}

fn format_table_stdout(rows: &Vec<SqliteRow>) -> String {
    //transform rows into a single string
    let rows_string = rows
        .iter()
        .map(|r| {
            format!(
                "{}\t{}\t{}\t{}",
                r.get::<String, _>("table"),
                r.get::<String, _>("path"),
                r.get::<String, _>("description"),
                r.get::<String, _>("type")
            )
        })
        .collect::<Vec<String>>()
        .join("\n");

    //add header information
    let mut rows_with_header: String = "table\tpath\tdescription\ttype\n".to_owned();
    rows_with_header.push_str(&rows_string);

    //format using elastic tabstops
    let mut tw = TabWriter::new(vec![]);
    write!(&mut tw, "{}", rows_with_header).unwrap();
    tw.flush().unwrap();

    //return result
    let result = String::from_utf8(tw.into_inner().unwrap()).unwrap();
    result
}

async fn get_table_from_database(database: &str, table: &str) -> Result<String, String> {
    let connection_string = format!("sqlite://{}?mode=rwc", database);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let table_checked = match table {
        "table" => "table",
        _ => panic!("Unsupported table"),
    };

    let query_string = format!("SELECT * FROM '{}'", &table_checked);
    let rows: Vec<SqliteRow> = sqlx::query(&query_string).fetch_all(&pool).await.unwrap();

    let result = format_table_stdout(&rows);
    Ok(result)
}

/// Merge two toml::Values.
/// The second argument is given priority in case of conflicts.
/// So, given two toml::Values d and c,
/// where d is considered a default configuartion,
/// and a is a custom configuration deviating from d.
/// Then, merge(d,c) keeps the custom values specified in c
/// and includes default values from d not specified in c.
///
/// #Examples
///
/// ```
/// use toml::Value;
///
/// let s1 = r#"
/// [package]
/// name = "macrobot"
/// version = "0.1.0"
/// edition = "2021"
/// "#;
///
/// let s2 = r#"
/// [package]
/// name = "nanobot"
/// version = "0.1.0"
/// "#;
///
/// let s3 = r#"
/// [package]
/// name = "nanobot"
/// version = "0.1.0"
/// edition = "2021"
/// "#;
///
/// let v1 = s1.parse::<Value>().unwrap();
/// let v2 = s2.parse::<Value>().unwrap();
/// let expected = s3.parse::<Value>().unwrap();
///
/// let merged = merge(v1,v2);
///
/// assert_eq!(expected, merged);
/// ```
fn merge(v1: &Value, v2: &Value) -> Value {
    match v1 {
        Value::Table(x) => match v2 {
            Value::Table(y) => {
                let mut merge_table = Map::new();
                for (k, v) in x {
                    if y.contains_key(k) {
                        let yv = y.get(k).unwrap();
                        let m = merge(v, yv);
                        merge_table.insert(k.clone(), m);
                    } else {
                        merge_table.insert(k.clone(), v.clone());
                    }
                }
                for (k, v) in y {
                    if !merge_table.contains_key(k) {
                        merge_table.insert(k.clone(), v.clone());
                    }
                }
                Value::Table(merge_table)
            }
            _ => panic!("Cannot merge inconsistent types."),
        },
        Value::Array(x) => match v2 {
            Value::Array(y) => {
                let mut merged = [&x[..], &y[..]].concat();
                merged.dedup();
                Value::Array(merged)
            }
            _ => panic!("Cannot merge inconsistent types."),
        },
        Value::String(_x) => match v2 {
            Value::String(y) => Value::String(y.clone()),
            _ => panic!("Cannot merge inconsistent types."),
        },
        Value::Integer(_x) => match v2 {
            Value::Integer(y) => Value::Integer(y.clone()),
            _ => panic!("Cannot merge inconsistent types."),
        },
        Value::Float(_x) => match v2 {
            Value::Float(y) => Value::Float(y.clone()),
            _ => panic!("Cannot merge inconsistent types."),
        },
        Value::Boolean(_x) => match v2 {
            Value::Boolean(y) => Value::Boolean(y.clone()),
            _ => panic!("Cannot merge inconsistent types."),
        },
        Value::Datetime(_x) => match v2 {
            Value::Datetime(y) => Value::Datetime(y.clone()),
            _ => panic!("Cannot merge inconsistent types."),
        },
    }
}

fn config(file_path: &str) -> Result<String, String> {
    let default_config = fs::read_to_string("src/resources/default_config.toml")
        .expect("Should have been able to read the file");

    let input_config =
        fs::read_to_string(file_path).expect("Should have been able to read the file");

    let input_value = input_config.parse::<Value>().unwrap();
    let default_value = default_config.parse::<Value>().unwrap();

    let value_table = input_value.as_table().unwrap();
    let default_table = default_value.as_table().unwrap();

    let mut merge_table = Map::new();
    merge_table.clone_from(&value_table);

    for (k, v) in default_table.iter() {
        if !value_table.contains_key(k) {
            merge_table.insert(k.clone(), v.clone());
        } else {
            let v2 = value_table.get(k).unwrap();
            let merged = merge(v, v2);
            merge_table.insert(k.clone(), merged);
        }
    }

    let merge_value = Value::Table(merge_table);
    let toml = toml::to_string(&merge_value).unwrap();

    Ok(toml)
}

#[async_std::main]
async fn main() {
    let matches = command!() // requires `cargo` feature
        .propagate_version(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("init").about("Initialises things"))
        .subcommand(Command::new("config").about("Configures things"))
        .subcommand(
            Command::new("get").about("Gets things from a table").arg(
                arg!(<TABLE> "A database table")
                    .required(true)
                    .value_parser(value_parser!(String)),
            ),
        )
        .get_matches();

    let exit_result = match matches.subcommand() {
        Some(("init", _sub_matches)) => init(),
        Some(("config", _sub_matches)) => config("nanobot.toml"),
        Some(("get", sub_matches)) => {
            match sub_matches.get_one::<String>("TABLE") {
                Some(x) => get_table_from_database(".nanobot.db", x),
                _ => panic!("No table given"),
            }
            .await
        }
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
