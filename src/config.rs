use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::fs;
use toml::map::Map;
use toml::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum Debug {
    INFO,
    WARN,
    ERROR,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub connection: String,
    pub pool: SqlitePool,
    pub debug: Debug,
}

#[derive(Clone, Debug)]
pub struct ConfigBuilder {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub connection: String,
    pub pool: SqlitePool,
    pub debug: Debug,
}

impl ConfigBuilder {
    pub async fn new() -> ConfigBuilder {
        let default_config_file = include_str!("resources/default_config.toml");
        let default_config = default_config_file.parse::<Value>().unwrap();
        let default_values = &default_config["tool"];

        let default_connection = String::from(".nanobot.db");
        let connection_string = format!("sqlite://{}?mode=rwc", &default_connection);
        let pool: SqlitePool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let mut config = ConfigBuilder {
            //set in default_config.toml
            name: String::from(default_values["name"].as_str().unwrap()),
            version: String::from(default_values["version"].as_str().unwrap()),
            edition: String::from(default_values["edition"].as_str().unwrap()),
            //not set in default_config.toml
            connection: default_connection,
            pool: pool,
            debug: Debug::INFO,
        };

        //update with user configuration (using nanobot.toml)
        let user_config_file = fs::read_to_string("nanobot.toml");

        match user_config_file {
            Ok(x) => {
                let user_config = x.parse::<Value>().unwrap();
                let user_values = &user_config["tool"]; //TODO: do we require 'tool' here?
                match user_values["name"].as_str() {
                    Some(x) => {
                        config.name = String::from(x);
                    }
                    _ => (),
                };
                match user_values["version"].as_str() {
                    Some(x) => {
                        config.version = String::from(x);
                    }
                    _ => (),
                };
                match user_values["edition"].as_str() {
                    Some(x) => {
                        config.edition = String::from(x);
                    }
                    _ => (),
                };
            }
            Err(_x) => (),
        };
        config
    }

    pub fn name<'a, S: Into<String>>(&'a mut self, name: S) -> &'a mut ConfigBuilder {
        self.name = name.into();
        self
    }

    pub fn version<'a, S: Into<String>>(&'a mut self, version: S) -> &'a mut ConfigBuilder {
        self.version = version.into();
        self
    }

    pub fn edition<'a, S: Into<String>>(&'a mut self, edition: S) -> &'a mut ConfigBuilder {
        self.edition = edition.into();
        self
    }

    pub fn connection<'a, S: Into<String>>(&'a mut self, connection: S) -> &'a mut ConfigBuilder {
        self.connection = connection.into();
        self
    }

    pub fn pool<'a>(&'a mut self, pool: SqlitePool) -> &'a mut ConfigBuilder {
        self.pool = pool;
        self
    }

    pub fn debug<'a>(&'a mut self, debug: Debug) -> &'a mut ConfigBuilder {
        self.debug = debug;
        self
    }

    pub fn build(&self) -> Config {
        Config {
            name: self.name.clone(),
            version: self.version.clone(),
            edition: self.edition.clone(),
            connection: self.connection.clone(),
            pool: self.pool.clone(),
            debug: self.debug.clone(),
        }
    }
}

impl Config {
    pub fn name<'a, S: Into<String>>(&'a mut self, name: S) -> &'a mut Config {
        self.name = name.into();
        self
    }

    pub fn version<'a, S: Into<String>>(&'a mut self, version: S) -> &'a mut Config {
        self.version = version.into();
        self
    }

    pub fn edition<'a, S: Into<String>>(&'a mut self, edition: S) -> &'a mut Config {
        self.edition = edition.into();
        self
    }

    pub fn connection<'a, S: Into<String>>(&'a mut self, connection: S) -> &'a mut Config {
        self.connection = connection.into();
        self
    }

    pub fn debug<'a>(&'a mut self, debug: Debug) -> &'a mut Config {
        self.debug = debug;
        self
    }
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
/// use nanobot::config::merge;
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
/// let merged = merge(&v1,&v2);
///
/// assert_eq!(expected, merged);
/// ```
pub fn merge(v1: &Value, v2: &Value) -> Value {
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

pub fn config(file_path: &str) -> Result<String, String> {
    let default_config = include_str!("resources/default_config.toml");

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
