use crate::config::Config;
use crate::get;
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;
use enquote::unquote;
use ontodev_sqlrest::parse;
use std::net::SocketAddr;
use std::sync::Arc;

struct AppState {
    pub config: Config,
}

#[tokio::main]
pub async fn app(config: &Config) -> Result<String, String> {
    let shared_state = Arc::new(AppState {
        //TODO: use &config instead of config.clone()?
        config: config.clone(),
    });

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        .route("/:table", get(table))
        .with_state(shared_state);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("listening on {}", addr);
    if let Err(e) = axum::Server::bind(&addr).serve(app.into_make_service()).await {
        return Err(e.to_string());
    }

    let hello = String::from("Hello, world!");
    Ok(hello)
}

async fn root() -> impl IntoResponse {
    tracing::info!("request root");
    Redirect::permanent("/table")
}

async fn table(
    Path(path): Path<String>,
    RawQuery(query): RawQuery,
    State(state): State<Arc<AppState>>,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!("request table {:?} {:?}", path, query);
    let mut table = path.clone();
    let mut format = "html";
    if path.ends_with(".pretty.json") {
        table = path.replace(".pretty.json", "");
        format = "pretty.json";
    } else if path.ends_with(".json") {
        table = path.replace(".json", "");
        format = "json";
    }
    let url = match query {
        Some(q) => format!("{}?{}", table, q),
        None => table.clone(),
    };

    tracing::info!("URL: {}", url);
    let mut select = parse(&url)?;
    tracing::info!("select {:?}", select);
    let mut msg_col_index = None;
    for (i, scol) in select.select.iter().enumerate() {
        let scol_name = scol.expression.to_lowercase();
        let scol_name = unquote(&scol_name).unwrap_or(scol_name);
        if scol_name == "message" {
            msg_col_index = Some(i);
        }
    }

    // If the request includes the message column, set the `show_messages` flag to true, which
    // we will then pass to `get_rows()`, and delete the message column from the select list:
    let mut show_messages = false;
    if let Some(index) = msg_col_index {
        select.select.remove(index);
        show_messages = true;
    }

    match get::get_rows(&state.config, &select, "page", &format, show_messages).await {
        Ok(x) => match format {
            "html" => Ok(Html(x).into_response()),
            "json" => {
                Ok(([("content-type", "application/json; charset=utf-8")], x).into_response())
            }
            "pretty.json" => Ok(x.into_response()),
            _ => unreachable!("Unsupported format"),
        },
        Err(x) => {
            tracing::info!("Get Error: {:?}", x);
            Ok((StatusCode::NOT_FOUND, Html("404 Not Found".to_string())).into_response())
        }
    }
}
