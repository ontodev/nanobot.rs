use crate::error::NanobotError;
use indexmap::map::IndexMap;
use lazy_static::lazy_static;
use ontodev_valve::valve::Valve;
use serde::{Deserialize, Serialize};
use serde_json::Value as SerdeValue;
use sqlx::any::AnyPool;
use std::{error, fmt, fs, path::Path};
use toml;

#[derive(Clone, Debug)]
pub struct Config {
    pub config_version: u16,
    pub port: u16,
    pub results_per_page: u16,
    pub logging_level: LoggingLevel,
    pub connection: String,
    pub pool: Option<AnyPool>,
    pub valve: Option<Valve>,
    pub valve_path: String,
    pub create_only: bool,
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

impl fmt::Display for LoggingLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TomlConfig {
    pub nanobot: NanobotConfig,
    pub logging: Option<LoggingConfig>,
    pub database: Option<DatabaseConfig>,
    pub valve: Option<ValveTomlConfig>,
    pub assets: Option<AssetsConfig>,
    pub templates: Option<TemplatesConfig>,
    pub actions: Option<IndexMap<String, ActionConfig>>,
}

impl Default for TomlConfig {
    fn default() -> TomlConfig {
        TomlConfig {
            nanobot: NanobotConfig::default(),
            logging: Some(LoggingConfig {
                level: Some(LoggingLevel::default()),
            }),
            database: Some(DatabaseConfig::default()),
            valve: Some(ValveTomlConfig::default()),
            assets: Some(AssetsConfig::default()),
            templates: Some(TemplatesConfig::default()),
            actions: Some(IndexMap::default()),
        }
    }
}

impl TomlConfig {
    pub fn write_non_defaults(&self, path: &Path) -> Result<(), Box<dyn error::Error>> {
        let default_toml = Self::default();
        let mut toml_contents = String::new();

        // We always write the nanobot section of the toml config even when it matches the default.
        // But for the other sections of the toml config, we only write them when they differ from
        // the default.
        toml_contents.push_str(&self.nanobot.to_string());

        if let Some(logging) = &self.logging {
            if &default_toml.logging.unwrap() != logging {
                toml_contents.push_str(&format!("\n{}", logging.to_string()));
            }
        }
        if let Some(database) = &self.database {
            if &default_toml.database.unwrap() != database {
                toml_contents.push_str(&format!("\n{}", database.to_string()));
            }
        }
        if let Some(valve) = &self.valve {
            if &default_toml.valve.unwrap() != valve {
                toml_contents.push_str(&format!("\n{}", valve.to_string()));
            }
        }
        if let Some(assets) = &self.assets {
            if &default_toml.assets.unwrap() != assets {
                toml_contents.push_str(&format!("\n{}", assets.to_string()));
            }
        }
        if let Some(templates) = &self.templates {
            if &default_toml.templates.unwrap() != templates {
                toml_contents.push_str(&format!("\n{}", templates.to_string()));
            }
        }
        if let Some(actions) = &self.actions {
            if &default_toml.actions.unwrap() != actions {
                for (name, details) in actions.iter() {
                    toml_contents.push_str(&format!("[actions.{}]\n{}\n", name, details));
                }
            }
        }

        fs::write(path, toml_contents).expect("Unable to write file");
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct NanobotConfig {
    pub config_version: u16,
    pub port: Option<u16>,
    pub results_per_page: Option<u16>,
}

impl Default for NanobotConfig {
    fn default() -> NanobotConfig {
        NanobotConfig {
            config_version: DEFAULT_CONFIG_VERSION,
            port: Some(DEFAULT_PORT),
            results_per_page: Some(DEFAULT_RESULTS_PER_PAGE),
        }
    }
}

impl fmt::Display for NanobotConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[nanobot]\n{}", toml::to_string(self).unwrap()).unwrap();
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct LoggingConfig {
    pub level: Option<LoggingLevel>,
}

impl fmt::Display for LoggingConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(level) = &self.level {
            write!(f, "[logging]\nlevel = \"{}\"\n", level).unwrap();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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

impl fmt::Display for DatabaseConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(connection) = &self.connection {
            write!(f, "[database]\nconnection = \"{}\"\n", connection).unwrap();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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

impl fmt::Display for ValveTomlConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "[valve]\npath = \"{}\"\n", path).unwrap();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AssetsConfig {
    pub path: Option<String>,
}

impl fmt::Display for AssetsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "[assets]\npath = \"{}\"\n", path).unwrap();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TemplatesConfig {
    pub path: Option<String>,
}

impl fmt::Display for TemplatesConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "[templates]\npath = \"{}\"\n", path).unwrap();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ActionConfig {
    pub label: String,
    pub inputs: Option<Vec<InputConfig>>,
    pub commands: Vec<Vec<String>>,
}

impl fmt::Display for ActionConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "label = \"{}\"\n", self.label).unwrap();
        if let Some(inputs) = &self.inputs {
            write!(f, "inputs = [\n").unwrap();
            for input in inputs {
                write!(f, "  {}", input).unwrap();
            }
            write!(f, "]\n").unwrap();
        }
        if !self.commands.is_empty() {
            write!(f, "commands = [\n").unwrap();
            for command in self.commands.iter() {
                write!(f, "  {:?},\n", command).unwrap();
            }
            write!(f, "]\n").unwrap();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InputConfig {
    pub name: String,
    pub label: String,
    pub value: Option<String>,
    pub default: Option<String>,
    pub placeholder: Option<String>,
    pub test: Option<String>,
}

impl fmt::Display for InputConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entry = format!("{}", toml::to_string(self).unwrap()).replace("\n", ", ");
        let entry = match entry.strip_suffix(", ") {
            None => entry,
            Some(e) => e.to_string(),
        };
        write!(f, "{{ {} }},\n", entry).unwrap();
        Ok(())
    }
}

pub type SerdeMap = serde_json::Map<String, SerdeValue>;

pub const DEFAULT_CONFIG_VERSION: u16 = 1;
pub const DEFAULT_PORT: u16 = 3000;
pub const DEFAULT_RESULTS_PER_PAGE: u16 = 20;
lazy_static! {
    pub static ref DEFAULT_TOML: String =
        format!("[nanobot]\nconfig_version = {}", DEFAULT_CONFIG_VERSION);
}

impl Config {
    pub async fn new() -> Result<Config, NanobotError> {
        let user_config_file = match fs::read_to_string("nanobot.toml") {
            Ok(x) => x,
            Err(_) => DEFAULT_TOML.to_string(),
        };
        let user: TomlConfig = toml::from_str(user_config_file.as_str())?;

        let config = Config {
            config_version: user.nanobot.config_version,
            port: user.nanobot.port.unwrap_or(DEFAULT_PORT),
            results_per_page: user
                .nanobot
                .results_per_page
                .unwrap_or(DEFAULT_RESULTS_PER_PAGE),
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
            create_only: false,
            asset_path: {
                match user.assets.unwrap_or_default().path {
                    Some(p) => {
                        if Path::new(&p).is_dir() {
                            Some(p)
                        } else {
                            tracing::warn!(
                                "Configuration specifies an assets directory \
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
                            tracing::warn!(
                                "Configuration specifies a template directory \
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

    pub fn connection<S: Into<String>>(&mut self, connection: S) -> &mut Config {
        let connection = connection.into();
        if let Some(_) = self.valve {
            tracing::warn!(
                "Valve has already been initialized. Changing the connection \
                 string from '{}' to '{}' will have no effect on the running Valve instance.",
                self.connection,
                connection
            );
        }
        self.connection = connection;
        self
    }

    pub fn create_only(&mut self, value: bool) -> &mut Config {
        self.create_only = value;
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
