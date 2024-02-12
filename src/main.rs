use crate::{config::Config, error::NanobotError, serve::build_app};
use axum_test_helper::{TestClient, TestResponse};
use clap::{arg, command, value_parser, Command};
use ontodev_valve::valve::Valve;
use std::path::Path;
use std::sync::Arc;
use std::{collections::HashMap, env, io};
use url::Url;

pub mod config;
pub mod error;
pub mod get;
pub mod init;
pub mod ldtab;
pub mod serve;
pub mod sql;
pub mod tree_view;

#[async_std::main]
async fn main() -> Result<(), NanobotError> {
    // initialize configuration
    let mut config: Config = Config::new().await?;

    let level = match config.logging_level {
        config::LoggingLevel::DEBUG => tracing::Level::DEBUG,
        config::LoggingLevel::INFO => tracing::Level::INFO,
        config::LoggingLevel::WARN => tracing::Level::WARN,
        config::LoggingLevel::ERROR => tracing::Level::ERROR,
    };

    // initialize tracing
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(level)
        .with_writer(io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    if let Some(vars) = cgi_vars() {
        return match handle_cgi(&vars, &mut config) {
            Err(x) => {
                tracing::error!("{}", x);
                std::process::exit(1)
            }
            Ok(x) => {
                println!("{}", x);
                Ok(())
            }
        };
    }

    let matches = command!() // requires `cargo` feature
        .propagate_version(true)
        .subcommand_required(false)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("init")
                .about("Initialises things")
                .arg(
                    arg!(
                        -d --database <FILE> "Specifies a custom database name"
                    )
                    .required(false)
                    .value_parser(value_parser!(String)),
                )
                .arg(arg!(--create_only "Only create VALVE tables").required(false))
                .arg(arg!(--initial_load "Use unsafe SQLite optimizations").required(false)),
        )
        .subcommand(Command::new("config").about("Configures things"))
        .subcommand(
            Command::new("get")
                .about("Gets things from a table")
                .arg(
                    arg!(<TABLE> "A database table")
                        .required(true)
                        .value_parser(value_parser!(String)),
                )
                .arg(
                    arg!(-s --shape <SHAPE> "Specifies a data 'shape', e.g. value_rows")
                        .required(false)
                        .value_parser(value_parser!(String)),
                )
                .arg(
                    arg!(-f --format <FORMAT> "Specifies an output format, e.g. json")
                        .required(false)
                        .value_parser(value_parser!(String)),
                ),
        )
        .subcommand(Command::new("serve").about("Run HTTP server"))
        .get_matches();

    let exit_result = match matches.subcommand() {
        Some(("init", sub_matches)) => {
            if let Some(d) = sub_matches.get_one::<String>("database") {
                config.connection(d);
            }
            if sub_matches.get_flag("create_only") {
                config.create_only(true);
            }
            let database = config.connection.to_owned();
            let path = Path::new(&database);
            if path.exists() {
                tracing::warn!("Initializing existing database: '{}'", path.display());
            }
            init::init(&mut config).await
        }
        Some(("config", _sub_matches)) => {
            build_valve(&mut config, false).await?;
            Ok(config.to_string())
        }
        Some(("get", sub_matches)) => {
            build_valve(&mut config, false).await?;
            let table = match sub_matches.get_one::<String>("TABLE") {
                Some(x) => x,
                _ => panic!("No table given"),
            };
            let shape = match sub_matches.get_one::<String>("shape") {
                Some(x) => x,
                _ => "value_rows",
            };
            let format = match sub_matches.get_one::<String>("format") {
                Some(x) => x,
                _ => "text",
            };
            let result = match get::get_table(&config, table, shape, format).await {
                Ok(x) => x,
                Err(x) => format!("ERROR: {:?}", x),
            };
            Ok(result)
        }
        Some(("serve", _sub_matches)) => {
            build_valve(&mut config, false).await?;
            serve::app(&config)
        }
        _ => Err(String::from(
            "Unrecognised or missing subcommand, but CGI environment vars are \
                             undefined",
        )),
    };

    //print exit message
    match exit_result {
        Err(x) => {
            tracing::error!("{}", x);
            std::process::exit(1)
        }

        Ok(x) => {
            println!("{}", x);
            Ok(())
        }
    }
}

/// Builds and assigns a Valve struct to the field `config.valve` and a copy of valve's
/// connection pool to the field `config.pool`.
async fn build_valve(config: &mut Config, initial_load: bool) -> Result<(), NanobotError> {
    (config.valve, config.pool) = {
        let valve =
            Valve::build(&config.valve_path, &config.connection, false, initial_load).await?;
        let pool = valve.pool.clone();
        (Some(valve), Some(pool))
    };
    Ok(())
}

#[tokio::main]
async fn handle_cgi(vars: &HashMap<String, String>, config: &mut Config) -> Result<String, String> {
    tracing::debug!("Processing CGI request with vars: {:?}", vars);

    let shared_state = Arc::new(serve::AppState {
        config: config.clone(),
    });
    let app = build_app(shared_state);
    let client = TestClient::new(app);

    let request_method = vars
        .get("REQUEST_METHOD")
        .ok_or("No 'REQUEST_METHOD' in CGI vars".to_string())?;
    let path_info = vars
        .get("PATH_INFO")
        .ok_or("No 'PATH_INFO' in CGI vars".to_string())?;
    let query_string = vars
        .get("QUERY_STRING")
        .ok_or("No 'QUERY_STRING' in CGI vars".to_string())?;
    let mut url = path_info.clone();
    if !url.starts_with("/") {
        url = format!("/{}", path_info);
    }
    if !query_string.is_empty() {
        url.push_str(&format!("?{}", query_string));
    }
    tracing::info!("In CGI mode, processing URL: {}", url);

    async fn generate_html(res: TestResponse) -> String {
        let mut html = format!("status: {}\n", res.status());
        for (hname, hval) in res.headers().iter() {
            html.push_str(&format!(
                "{}: {}\n",
                hname,
                hval.to_str().unwrap_or_default()
            ));
        }
        let html_body = &res.text().await;
        html.push_str(&format!("\n{}", html_body));
        html
    }

    match request_method.to_lowercase().as_str() {
        "post" => {
            let query_str = std::io::stdin()
                .lines()
                .map(|l| l.unwrap())
                .collect::<Vec<_>>()
                .join("\n");
            let query_str = format!("http://example.com?{}", query_str);
            // TODO: Check to see if this is provided by Axum so that we can remove the extra
            // library dependency (on Url).
            let example_url = Url::try_from(query_str.as_str()).map_err(|e| e.to_string())?;
            let mut form = vec![];
            for (key, val) in example_url.query_pairs() {
                form.push((key.to_string(), val.to_string()));
            }
            tracing::debug!("In CGI mode, processing form for POST: {:?}", form);
            let res = client.post(&url).form(&form).send().await;
            Ok(generate_html(res).await)
        }
        "get" => {
            let res = client.get(&url).send().await;
            Ok(generate_html(res).await)
        }
        _ => Err(format!(
            "Content-Type: text/html\nStatus: 400\nUnrecognized request method: {}",
            request_method
        )),
    }
}

fn cgi_vars() -> Option<HashMap<String, String>> {
    let mut vars = match env::var_os("GATEWAY_INTERFACE").and_then(|p| Some(p.into_string())) {
        Some(Ok(s)) if s == "CGI/1.1" => HashMap::new(),
        _ => return None,
    };

    for var in vec!["REQUEST_METHOD", "PATH_INFO", "QUERY_STRING"] {
        match env::var_os(var).and_then(|p| Some(p.into_string())) {
            Some(Ok(s)) => vars.insert(var.to_string(), s),
            _ => match var {
                "REQUEST_METHOD" => vars.insert(var.to_string(), "GET".to_string()),
                "PATH_INFO" => vars.insert(var.to_string(), "/table".to_string()),
                "QUERY_STRING" => vars.insert(var.to_string(), String::new()),
                _ => {
                    // This should never happen since all possible cases should be handled above:
                    unreachable!(
                        "CGI mode enabled but environment variable: {} is undefined. Exiting.",
                        var
                    );
                }
            },
        };
    }

    Some(vars)
}
