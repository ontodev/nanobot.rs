use crate::config::Config;
use crate::get;
use axum::extract::{Json, Path, Query, RawQuery, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;
use enquote::unquote;
use futures::executor::block_on;
use ontodev_sqlrest::{parse, Select};
use ontodev_valve::{validate::validate_row, valve_grammar::StartParser};
use serde_json::{json, Value as SerdeValue};
use sqlx::any::AnyPool;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
enum RequestType {
    POST,
    GET,
}

#[derive(Debug)]
struct AppState {
    pub config: Config,
}

pub type RequestParams = HashMap<String, String>;
/// An alias for [serde_json::Map](..//serde_json/struct.Map.html)<String, [serde_json::Value](../serde_json/enum.Value.html)>.
// Note: serde_json::Map is
// [backed by a BTreeMap by default](https://docs.serde.rs/serde_json/map/index.html) which can be
// overriden by specifying the preserve-order feature in Cargo.toml, which we have indeed specified.
pub type SerdeMap = serde_json::Map<String, SerdeValue>;

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
        .route("/:table/row/:row_number", get(get_row).post(post_row))
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
    let select = parse(&url)?;
    tracing::info!("select {:?}", select);

    match get::get_rows(&state.config, &select, "page", &format).await {
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

async fn post_row(
    Path((table, row_number)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
    Query(query_params): Query<RequestParams>,
    Json(form_params): Json<RequestParams>,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!(
        "request row POST {:?} {:?} {:?} {:?}",
        table,
        row_number,
        query_params,
        form_params
    );
    row(Path((table, row_number)), &state, &query_params, &form_params, RequestType::POST)
}

async fn get_row(
    Path((table, row_number)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
    Query(params): Query<RequestParams>,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!("request row GET {:?} {:?} {:?}", table, row_number, params);
    row(Path((table, row_number)), &state, &params, &RequestParams::new(), RequestType::GET)
}

fn row(
    Path((table, row_number)): Path<(String, String)>,
    state: &Arc<AppState>,
    query_params: &RequestParams,
    form_params: &RequestParams,
    request_type: RequestType,
) -> axum::response::Result<impl IntoResponse> {
    let pool = match state.config.pool.as_ref() {
        Some(p) => p,
        _ => {
            let error = format!("Could not connect to database using pool {:?}", state.config.pool);
            return Err((StatusCode::BAD_REQUEST, Html(error)).into_response().into());
        }
    };

    match is_ontology(&table, &pool) {
        Err(e) => return Err((StatusCode::BAD_REQUEST, Html(e)).into_response().into()),
        Ok(flag) if flag => {
            let error = format!("'row' path is not valid for ontology table '{}'", table);
            return Err((StatusCode::BAD_REQUEST, Html(error)).into_response().into());
        }
        _ => (),
    };

    let row_number = match row_number.parse::<u32>() {
        Ok(r) => r,
        Err(e) => {
            let error = format!("Unable to parse row_number '{}' due to error: {}", row_number, e);
            return Err((StatusCode::BAD_REQUEST, Html(error)).into_response().into());
        }
    };

    render_row_from_database(
        &table,
        None,
        row_number,
        state,
        query_params,
        form_params,
        request_type,
    )
}

fn render_row_from_database(
    table: &str,
    term_id: Option<String>,
    row_number: u32,
    state: &Arc<AppState>,
    query_params: &RequestParams,
    form_params: &RequestParams,
    request_type: RequestType,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!("QUERY PARAMS: {:#?}", query_params);
    tracing::info!("FORM PARAMS: {:#?}", form_params);
    let pool = match state.config.pool.as_ref() {
        Some(p) => p,
        _ => {
            let error = format!("Could not connect to database using pool {:?}", state.config.pool);
            return Err((StatusCode::BAD_REQUEST, Html(error)).into_response().into());
        }
    };
    let mut view = match query_params.get("view") {
        None => return Err(format!("No 'view' in {:?}", query_params).into()),
        Some(v) => v.to_string(),
    };
    //let mut messages = None;
    let mut form_html = None;
    if request_type == RequestType::POST {
        let mut new_row = SerdeMap::new();
        let columns = match get_sql_columns(table, pool) {
            Err(e) => return Err(e.into()),
            Ok(v) => v,
        };
        // Use the list of columns for the table from the db to look up their values in the form:
        for column in &columns {
            if column != "row_number" {
                let value = match form_params.get(column) {
                    Some(v) => v,
                    None => {
                        let other_column = format!("{}_other", column);
                        match form_params.get(&other_column) {
                            Some(v) => v,
                            None => {
                                return Err(format!(
                                    "No '{}' or '{}' in {:?}",
                                    column, other_column, form_params
                                )
                                .into())
                            }
                        }
                    }
                };
                new_row.insert(column.to_string(), value.to_string().into());
            }
        }

        // Manually override view, which is not included in request.args in CGI app
        view = String::from("form");
        let action = match form_params.get("action") {
            None => return Err(format!("No 'action' in {:?}", form_params).into()),
            Some(v) => v,
        };
        if action == "validate" {
            let validated_row = match validate_table_row(table, &new_row, Some(row_number), state) {
                Ok(v) => {
                    let mut tmp = SerdeMap::new();
                    tmp.insert("row_number".to_string(), json!(row_number));
                    tmp.extend(v);
                    tmp
                }
                Err(e) => return Err(e.into()),
            };
            form_html = Some(get_row_as_form(table, &validated_row));
            todo!();
        }
    }

    Ok(Html(format!(
        "What can I do for you, your table '{}' and your row number {} today, sir?",
        table, row_number,
    ))
    .into_response())
}

fn get_sql_columns(table: &str, pool: &AnyPool) -> Result<Vec<String>, String> {
    let mut select = Select::new(format!("\"{}\"", table));
    if let Err(e) = select.select_all(&pool) {
        return Err(e);
    }
    Ok(select
        .select
        .iter()
        .map(|s| unquote(&s.expression).unwrap_or(s.expression.to_string()))
        .collect::<Vec<_>>())
}

fn is_ontology(table: &str, pool: &AnyPool) -> Result<bool, String> {
    let columns = match get_sql_columns(table, pool) {
        Err(e) => return Err(e),
        Ok(c) => c,
    };
    Ok(columns.contains(&"subject".to_string())
        && columns.contains(&"predicate".to_string())
        && columns.contains(&"object".to_string())
        && columns.contains(&"datatype".to_string())
        && columns.contains(&"annotation".to_string()))
}

fn validate_table_row(
    table_name: &str,
    row_data: &SerdeMap,
    row_number: Option<u32>,
    state: &Arc<AppState>,
) -> Result<SerdeMap, String> {
    let (vconfig, dt_conds, rule_conds) = match &state.config.valve {
        Some(v) => (&v.config, &v.datatype_conditions, &v.rule_conditions),
        None => return Err(format!("Valve configuration is undefined in {:?}", state.config)),
    };
    let pool = match &state.config.pool {
        Some(p) => p.clone(),
        None => return Err(format!("Pool is undefined in {:?}", state.config)),
    };

    let validated_row = match row_number {
        Some(row_number) => {
            let mut result_row = SerdeMap::new();
            for (column, value) in row_data.iter() {
                result_row.insert(
                    column.to_string(),
                    json!({
                        "value": value.clone(),
                        "valid": true,
                        "messages": Vec::<SerdeMap>::new(),
                    }),
                );
            }
            block_on(validate_row(
                &vconfig,
                &dt_conds,
                &rule_conds,
                &pool,
                table_name,
                &result_row,
                true,
                Some(row_number),
            ))
            .unwrap()
        }
        None => block_on(validate_row(
            &vconfig,
            &dt_conds,
            &rule_conds,
            &pool,
            table_name,
            row_data,
            false,
            None,
        ))
        .unwrap(),
    };
    Ok(validated_row)
}

fn get_row_as_form(table_name: &str, row_data: &SerdeMap) -> String {
    todo!();
    String::from("")
}
