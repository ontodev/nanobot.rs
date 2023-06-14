use crate::{config::Config, serve::build_app};
use axum_test_helper::TestClient;
use clap::{arg, command, value_parser, Command};
use std::sync::Arc;
use std::{collections::HashMap, env};

pub mod config;
pub mod get;
pub mod init;
pub mod serve;
pub mod sql;

#[async_std::main]
async fn main() {
    // initialize configuration
    let mut config: config::Config = config::Config::new().await.unwrap();

    let level = match config.logging_level {
        config::LoggingLevel::DEBUG => tracing::Level::DEBUG,
        config::LoggingLevel::INFO => tracing::Level::INFO,
        config::LoggingLevel::WARN => tracing::Level::WARN,
        config::LoggingLevel::ERROR => tracing::Level::ERROR,
    };

    // initialize tracing
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(level)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let matches = command!() // requires `cargo` feature
        .propagate_version(true)
        .subcommand_required(false)
        .arg_required_else_help(false)
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

    let exit_result =
        match cgi_vars() {
            Some(vars) => handle_cgi(&vars, &mut config),
            None => match matches.subcommand() {
                Some(("init", sub_matches)) => match sub_matches.get_one::<String>("database") {
                    Some(x) => {
                        //update config
                        config.connection(x);

                        init::init(&config).await
                    }
                    _ => init::init(&config).await,
                },
                Some(("config", _sub_matches)) => Ok(config.to_string()),
                Some(("get", sub_matches)) => {
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
                    let result =
                        match get::get_table(config.init().await.unwrap(), table, shape, format)
                            .await
                        {
                            Ok(x) => x,
                            Err(x) => format!("ERROR: {:?}", x),
                        };
                    Ok(result)
                }
                Some(("serve", _sub_matches)) => serve::app(config.init().await.unwrap()),
                _ => Err(String::from(
                    "Unrecognised or missing subcommand, but CGI environment vars are \
                             undefined",
                )),
            },
        };

    //print exit message
    match exit_result {
        Err(x) => {
            tracing::error!("{}", x);
            std::process::exit(1)
        }

        Ok(x) => println!("{}", x),
    }
}

#[tokio::main]
async fn handle_cgi(vars: &HashMap<String, String>, config: &mut Config) -> Result<String, String> {
    let shared_state = Arc::new(serve::AppState {
        config: config.init().await.unwrap().clone(),
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
    let mut url = format!("/{}", path_info);
    if !query_string.is_empty() {
        url.push_str(&format!("?{}", query_string));
    }
    tracing::info!("In CGI mode, processing URL: {}", url);

    match request_method.to_lowercase().as_str() {
        "post" => {
            // TODO: This is still a little bit hacky. Maybe use a library function provided
            // by Axum (if one exists) instead of url::Url as we do below. It would also be good
            // if we didn't have to prepend "http://example.com".
            // Also: There may be a better way to read from STDIN using the clap library (which
            // we are already using) instead of what we are doing below.
            let mut query_str = String::new();
            std::io::stdin().read_line(&mut query_str);
            let query_str = format!("http://example.com?{}", query_str);
            let example_url = url::Url::try_from(query_str.as_str()).unwrap();
            let mut form = vec![];
            for (key, val) in example_url.query_pairs() {
                form.push((key.to_string(), val.to_string()));
            }
            let res = client.post(&url).form(&form).send().await;
            let html = res.text().await;
            Ok(html)
        }
        "get" => {
            let res = client.get(&url).send().await;
            let html = res.text().await;
            Ok(html)
        }
        _ => Err(format!("Unrecognized request method: {}", request_method)),
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
