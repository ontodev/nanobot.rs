use indexmap::map::IndexMap;
use ontodev_valve::{
    get_compiled_datatype_conditions, get_compiled_rule_conditions,
    get_parsed_structure_conditions, valve, valve_grammar::StartParser, ColumnRule,
    CompiledCondition, ParsedStructure, ValveCommand,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as SerdeValue;
use sqlx::{
    any::{AnyConnectOptions, AnyKind, AnyPool, AnyPoolOptions},
    query as sqlx_query,
};
use std::{collections::HashMap, fmt, fs, path::Path, str::FromStr};
use toml;

#[derive(Clone, Debug)]
pub struct Config {
    pub config_version: u16,
    pub port: u16,
    pub results_per_page: u16, 
    pub logging_level: LoggingLevel,
    pub connection: String,
    pub pool: Option<AnyPool>,
    pub valve: Option<ValveConfig>,
    pub valve_path: String,
    pub valve_create_only: bool,
    pub valve_initial_load: bool,
    pub asset_path: Option<String>,
    pub template_path: Option<String>,
    pub actions: IndexMap<String, ActionConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum LoggingLevel {
    DEBUG,
    INFO,
    WARN,
    ERROR,
}

impl Default for LoggingLevel {
    fn default() -> LoggingLevel {
        LoggingLevel::WARN
    }
}

#[derive(Clone, Debug)]
pub struct ValveConfig {
    pub config: SerdeMap,
    pub datatype_conditions: HashMap<String, CompiledCondition>,
    pub rule_conditions: HashMap<String, HashMap<String, Vec<ColumnRule>>>,
    pub structure_conditions: HashMap<String, ParsedStructure>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TomlConfig {
    pub nanobot: NanobotConfig,
    pub logging: Option<LoggingConfig>,
    pub database: Option<DatabaseConfig>,
    pub valve: Option<ValveTomlConfig>,
    pub assets: Option<AssetsConfig>,
    pub templates: Option<TemplatesConfig>,
    pub actions: Option<IndexMap<String, ActionConfig>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NanobotConfig {
    pub config_version: u16,
    pub port: Option<u16>,
    pub results_per_page: Option<u16>, 
}

impl Default for NanobotConfig {
    fn default() -> NanobotConfig {
        NanobotConfig {
            config_version: 1,
            port: Some(3000),
            results_per_page: Some(20), // TODO: 100?
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LoggingConfig {
    pub level: Option<LoggingLevel>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub connection: Option<String>,
}

impl Default for DatabaseConfig {
    fn default() -> DatabaseConfig {
        DatabaseConfig {
            connection: Some(".nanobot.db".into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ValveTomlConfig {
    pub path: Option<String>,
}

impl Default for ValveTomlConfig {
    fn default() -> ValveTomlConfig {
        ValveTomlConfig {
            path: Some("src/schema/table.tsv".into()),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AssetsConfig {
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TemplatesConfig {
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ActionConfig {
    pub label: String,
    pub inputs: Option<Vec<InputConfig>>,
    pub commands: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InputConfig {
    pub name: String,
    pub label: String,
    pub value: Option<String>,
    pub default: Option<String>,
    pub placeholder: Option<String>,
    pub test: Option<String>,
}

pub type SerdeMap = serde_json::Map<String, SerdeValue>;

pub const DEFAULT_TOML: &str = "[nanobot]
config_version = 1";

impl Config {
    pub async fn new() -> Result<Config, String> {
        let user_config_file = match fs::read_to_string("nanobot.toml") {
            Ok(x) => x,
            Err(_) => DEFAULT_TOML.into(),
        };
        let user: TomlConfig = match toml::from_str(user_config_file.as_str()) {
            Ok(d) => d,
            Err(e) => return Err(e.to_string()),
        };

        let config = Config {
            config_version: user.nanobot.config_version,
            port: user.nanobot.port.unwrap_or(3000),
            results_per_page: user.nanobot.results_per_page.unwrap_or(20), 
            logging_level: user.logging.unwrap_or_default().level.unwrap_or_default(),
            connection: user
                .database
                .unwrap_or_default()
                .connection
                .unwrap_or(".nanobot.db".into()),
            pool: None,
            valve: None,
            valve_path: user
                .valve
                .unwrap_or_default()
                .path
                .unwrap_or("src/schema/table.tsv".into()),
            valve_create_only: false,
            valve_initial_load: false,
            asset_path: {
                match user.assets.unwrap_or_default().path {
                    Some(p) => {
                        if Path::new(&p).is_dir() {
                            Some(p)
                        } else {
                            eprintln!(
                                "WARNING: Configuration specifies an assets directory \
                                '{}' but it does not exist.",
                                p
                            );
                            None
                        }
                    }
                    None => None,
                }
            },
            template_path: {
                match user.templates.unwrap_or_default().path {
                    Some(p) => {
                        if Path::new(&p).is_dir() {
                            Some(p)
                        } else {
                            eprintln!(
                                "WARNING: Configuration specifies a template directory \
                                '{}' but it does not exist. Using default templates.",
                                p
                            );
                            None
                        }
                    }
                    None => None,
                }
            },
            actions: user.actions.unwrap_or_default(),
        };

        Ok(config)
    }

    pub async fn init(&mut self) -> Result<&mut Config, String> {
        self.start_pool().await.unwrap().load_valve_config().await?;
        Ok(self)
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

        let pool = match AnyPoolOptions::new()
            .max_connections(5)
            .connect_with(connection_options)
            .await
        {
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
        let verbose = false;
        let initial_load = false;
        match valve(
            &self.valve_path,
            &self.connection,
            &ValveCommand::Config,
            verbose,
            initial_load,
            "table",
        )
        .await
        {
            Err(e) => {
                return Err(format!(
                    "VALVE error while initializing from {}: {:?}",
                    &self.valve_path, e
                ))
            }
            Ok(v) => {
                let v: SerdeMap = serde_json::from_str(&v).unwrap();
                let parser = StartParser::new();
                let d = get_compiled_datatype_conditions(&v, &parser);
                let r = get_compiled_rule_conditions(&v, d.clone(), &parser);
                let p = get_parsed_structure_conditions(&v, &parser);
                self.valve = Some(ValveConfig {
                    config: v,
                    datatype_conditions: d,
                    rule_conditions: r,
                    structure_conditions: p,
                });
            }
        };

        Ok(self)
    }

    pub fn connection<S: Into<String>>(&mut self, connection: S) -> &mut Config {
        self.connection = connection.into();
        self
    }

    pub fn create_only(&mut self, value: bool) -> &mut Config {
        self.valve_create_only = value;
        self
    }

    pub fn initial_load(&mut self, value: bool) -> &mut Config {
        self.valve_initial_load = value;
        self
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", toml::to_string(&to_toml(&self)).unwrap())
    }
}

pub fn to_toml(config: &Config) -> TomlConfig {
    TomlConfig {
        nanobot: NanobotConfig {
            config_version: config.config_version.clone(),
            port: Some(config.port.clone()),
            results_per_page: Some(config.results_per_page.clone()),
        },
        logging: Some(LoggingConfig {
            level: Some(config.logging_level.clone()),
        }),
        database: Some(DatabaseConfig {
            connection: Some(config.connection.clone()),
        }),
        valve: Some(ValveTomlConfig {
            path: Some(config.valve_path.clone()),
        }),
        assets: Some(AssetsConfig {
            path: config.asset_path.clone(),
        }),
        templates: Some(TemplatesConfig {
            path: config.template_path.clone(),
        }),
        actions: Some(config.actions.clone()),
    }
}
