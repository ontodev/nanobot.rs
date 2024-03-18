use ontodev_valve::valve::ValveError;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum NanobotError {
    GeneralError(String),
    ValveError(ValveError),
    TomlError(toml::de::Error),
    GetError(GetError),
    AnyhowError(anyhow::Error),
}

impl From<ValveError> for NanobotError {
    fn from(e: ValveError) -> Self {
        Self::ValveError(e)
    }
}

impl From<toml::de::Error> for NanobotError {
    fn from(e: toml::de::Error) -> Self {
        Self::TomlError(e)
    }
}

impl From<GetError> for NanobotError {
    fn from(e: GetError) -> Self {
        Self::GetError(e)
    }
}

impl From<anyhow::Error> for NanobotError {
    fn from(e: anyhow::Error) -> Self {
        Self::AnyhowError(e)
    }
}

#[derive(Debug)]
pub struct GetError {
    details: String,
}

impl GetError {
    pub fn new(msg: String) -> GetError {
        GetError { details: msg }
    }
}

impl fmt::Display for GetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl Error for GetError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl From<String> for GetError {
    fn from(error: String) -> GetError {
        GetError::new(error)
    }
}

impl From<std::io::Error> for GetError {
    fn from(error: std::io::Error) -> GetError {
        GetError::new(format!("{:?}", error))
    }
}

impl From<csv::Error> for GetError {
    fn from(error: csv::Error) -> GetError {
        GetError::new(format!("{:?}", error))
    }
}

impl From<serde_json::Error> for GetError {
    fn from(error: serde_json::Error) -> GetError {
        GetError::new(format!("{:?}", error))
    }
}

impl From<sqlx::Error> for GetError {
    fn from(error: sqlx::Error) -> GetError {
        GetError::new(format!("{:?}", error))
    }
}

impl From<git2::Error> for GetError {
    fn from(error: git2::Error) -> GetError {
        GetError::new(format!("{:?}", error))
    }
}

impl From<std::time::SystemTimeError> for GetError {
    fn from(error: std::time::SystemTimeError) -> GetError {
        GetError::new(format!("{:?}", error))
    }
}
