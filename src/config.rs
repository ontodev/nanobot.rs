use ontodev_valve::{
    get_compiled_datatype_conditions, get_compiled_rule_conditions, valve,
    valve_grammar::StartParser, ColumnRule, CompiledCondition, ValveCommand,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as SerdeValue;
use sqlx::{
    any::{AnyConnectOptions, AnyKind, AnyPool, AnyPoolOptions},
    query as sqlx_query,
};
use std::{collections::HashMap, fs, str::FromStr};
use toml::map::Map;
use toml::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum Debug {
    INFO,
    WARN,
    ERROR,
}

#[derive(Clone, Debug)]
pub struct ValveConfig {
    pub config: SerdeMap,
    pub datatype_conditions: HashMap<String, CompiledCondition>,
    pub rule_conditions: HashMap<String, HashMap<String, Vec<ColumnRule>>>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub connection: String,
    pub pool: Option<AnyPool>,
    pub valve: Option<ValveConfig>,
    pub debug: Debug,
}

pub type SerdeMap = serde_json::Map<String, SerdeValue>;

impl Config {
    pub async fn new() -> Result<Config, String> {
        let default_config_file = include_str!("resources/default_config.toml");
        let default_config = match default_config_file.parse::<Value>() {
            Ok(d) => d,
            Err(e) => return Err(e.to_string()),
        };
        let default_values = &default_config["tool"];

        let default_connection = String::from(".nanobot.db");
        //let default_connection = String::from("postgresql:///valve_postgres");

        let mut config = Config {
            //set in default_config.toml
            name: String::from(match default_values["name"].as_str() {
                Some(s) => s,
                None => {
                    return Err(format!("Could not convert '{}' to str", default_values["name"]))
                }
            }),
            version: String::from(match default_values["version"].as_str() {
                Some(s) => s,
                None => {
                    return Err(format!("Could not convert '{}' to str", default_values["version"]))
                }
            }),
            edition: String::from(match default_values["edition"].as_str() {
                Some(s) => s,
                None => {
                    return Err(format!("Could not convert '{}' to str", default_values["edition"]))
                }
            }),
            //not set in default_config.toml
            connection: default_connection,
            pool: None,
            valve: None,
            debug: Debug::INFO,
        };

        //update with user configuration (using nanobot.toml)
        let user_config_file = fs::read_to_string("nanobot.toml");

        match user_config_file {
            Ok(x) => {
                let user_config = match x.parse::<Value>() {
                    Ok(u) => u,
                    Err(e) => return Err(e.to_string()),
                };
                let user_values = &user_config["tool"]; //TODO: do we require 'tool' here?
                if let Some(x) = user_values["name"].as_str() {
                    config.name = String::from(x);
                }
                if let Some(x) = user_values["version"].as_str() {
                    config.version = String::from(x);
                }
                if let Some(x) = user_values["edition"].as_str() {
                    config.edition = String::from(x);
                };
            }
            Err(_x) => (),
        };
        Ok(config)
    }

    pub async fn start_pool(&mut self) -> Result<&mut Config, String> {
        let connection_options;
        if self.connection.starts_with("postgresql://") {
            connection_options = match AnyConnectOptions::from_str(&self.connection) {
                Ok(o) => o,
                Err(e) => return Err(e.to_string()),
            };
        } else {
            let connection_string;
            if !self.connection.starts_with("sqlite://") {
                connection_string = format!("sqlite://{}?mode=rwc", self.connection);
            } else {
                connection_string = self.connection.to_string();
            }
            connection_options = match AnyConnectOptions::from_str(connection_string.as_str()) {
                Ok(o) => o,
                Err(e) => return Err(e.to_string()),
            };
        }

        let pool =
            match AnyPoolOptions::new().max_connections(5).connect_with(connection_options).await {
                Ok(o) => o,
                Err(e) => return Err(e.to_string()),
            };
        if pool.any_kind() == AnyKind::Sqlite {
            if let Err(e) = sqlx_query("PRAGMA foreign_keys = ON").execute(&pool).await {
                return Err(e.to_string());
            }
        }
        self.pool = Some(pool);
        Ok(self)
    }

    pub async fn load_valve_config(&mut self) -> Result<&mut Config, String> {
        // TODO: Make the path configurable:
        let path = "src/schema/table.tsv";
        match valve(path, &self.connection, &ValveCommand::Config, false, "table").await {
            Err(_) => return Err(format!("Could not load from '{}'", path)),
            Ok(v) => {
                let v: SerdeMap = serde_json::from_str(&v).unwrap();
                let parser = StartParser::new();
                let d = get_compiled_datatype_conditions(&v, &parser);
                let r = get_compiled_rule_conditions(&v, d.clone(), &parser);
                self.valve =
                    Some(ValveConfig { config: v, datatype_conditions: d, rule_conditions: r });
            }
        };

        Ok(self)
    }

    pub fn name<S: Into<String>>(&mut self, name: S) -> &mut Config {
        self.name = name.into();
        self
    }

    pub fn version<S: Into<String>>(&mut self, version: S) -> &mut Config {
        self.version = version.into();
        self
    }

    pub fn edition<S: Into<String>>(&mut self, edition: S) -> &mut Config {
        self.edition = edition.into();
        self
    }

    pub fn connection<S: Into<String>>(&mut self, connection: S) -> &mut Config {
        self.connection = connection.into();
        self
    }

    pub fn debug(&mut self, debug: Debug) -> &mut Config {
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

    let input_value = match input_config.parse::<Value>() {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };
    let default_value = match default_config.parse::<Value>() {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };

    let value_table = match input_value.as_table() {
        Some(v) => v,
        None => return Err(format!("'{}' is not a table", input_value)),
    };
    let default_table = match default_value.as_table() {
        Some(v) => v,
        None => return Err(format!("'{}' is not a table", default_value)),
    };

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
    let toml = match toml::to_string(&merge_value) {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };

    Ok(toml)
}
