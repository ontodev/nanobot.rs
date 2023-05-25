use crate::config::{Config, ValveConfig};
use crate::get;
use axum::extract::{Json, Path, Query, RawQuery, State};
use axum::http::{uri, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;
use enquote::unquote;
use futures::executor::block_on;
use html_escape::encode_text_to_string;
use minijinja::{context, Environment};
use ontodev_hiccup::hiccup;
use ontodev_sqlrest::{parse, Filter, Select};
use ontodev_valve::{ast::Expression, update_row, validate::validate_row, CompiledCondition};
use serde_json::{json, Value as SerdeValue};
use sqlx::any::AnyPool;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{env, fs};

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

    let config = match state.config.valve.as_ref() {
        Some(c) => c,
        None => {
            return Err((StatusCode::BAD_REQUEST, Html("Valve config missing"))
                .into_response()
                .into());
        }
    };

    match is_ontology(&table, &config) {
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
        &None,
        row_number,
        state,
        query_params,
        form_params,
        request_type,
    )
}

fn render_row_from_database(
    table: &str,
    term_id: &Option<String>,
    row_number: u32,
    state: &Arc<AppState>,
    query_params: &RequestParams,
    form_params: &RequestParams,
    request_type: RequestType,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!("QUERY PARAMS: {:#?}", query_params);
    tracing::info!("FORM PARAMS: {:#?}", form_params);
    let config = match state.config.valve.as_ref() {
        Some(c) => c,
        None => {
            return Err((StatusCode::BAD_REQUEST, Html("Valve config missing"))
                .into_response()
                .into());
        }
    };
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
    let mut messages = HashMap::new();
    let mut form_html = None;
    if request_type == RequestType::POST {
        let mut new_row = SerdeMap::new();
        let columns = get_sql_columns(table, config)?;
        // Use the list of columns for the table from the db to look up their values in the form:
        for column in &columns {
            if column != "row_number" {
                let value = match form_params.get(column) {
                    Some(v) => v.to_string(),
                    None => {
                        let other_column = format!("{}_other", column);
                        form_params.get(&other_column).unwrap_or(&String::from("")).to_string()
                    }
                };
                new_row.insert(column.to_string(), value.into());
            }
        }

        // Manually override view, which is not included in request.args in CGI app
        view = String::from("form");
        let action = match form_params.get("action") {
            None => return Err(format!("No 'action' in {:?}", form_params).into()),
            Some(v) => v,
        };
        if action == "validate" {
            let validated_row = match validate_table_row(table, &new_row, &Some(row_number), state)
            {
                Ok(v) => {
                    let mut tmp = SerdeMap::new();
                    tmp.insert("row_number".to_string(), json!(row_number));
                    tmp.extend(v);
                    tmp
                }
                Err(e) => return Err(e.into()),
            };
            tracing::info!("VALIDATED ROW: {:#?}", validated_row);
            form_html = Some(get_row_as_form(state, config, table, &validated_row)?);
        } else if action == "submit" {
            let validated_row = match validate_table_row(table, &new_row, &Some(row_number), state)
            {
                Ok(v) => v,
                Err(e) => return Err(e.into()),
            };
            if let Err(e) =
                block_on(update_row(&config.config, pool, table, &validated_row, row_number))
            {
                return Err(e.to_string().into());
            }

            messages = get_messages(&validated_row);
            //tracing::info!("GOT MESSAGES {:#?}", messages);
            if let Some(error_messages) = messages.get_mut("error") {
                let extra_message = format!("Row updated with {} errors", error_messages.len());
                match messages.get_mut("warn") {
                    Some(warn_messages) => warn_messages.push(extra_message),
                    None => {
                        messages.insert("warn".to_string(), vec![extra_message]);
                    }
                };
            } else {
                messages
                    .insert("success".to_string(), vec!["Row successfully updated!".to_string()]);
            }
            //tracing::info!("MESSAGES ARE NOW {:#?}", messages);
        }
    }

    if view == "form" {
        if let None = form_html {
            let mut select = Select::new(format!("{}_view", table));
            select.filter(vec![
                Filter::new("row_number", "eq", json!(format!("{}", row_number))).unwrap()
            ]);
            //tracing::info!("RUNNING SELECT: {}", select.to_sqlite().unwrap());
            let mut rows = select.fetch_rows_as_json(pool, &HashMap::new()).unwrap();
            let row = &mut rows[0];
            //tracing::info!("GOT ROW: {:#?}", row);
            let metafied_row = {
                // TODO: Make this block its own function.
                let mut tmp = SerdeMap::new();
                let mut messages = match row.get_mut("message") {
                    Some(SerdeValue::Array(m)) => m.clone(),
                    _ => return Err(format!("No array called 'messages' in row: {:?}", row).into()),
                };
                //tracing::info!("ROW MESSAGES: {:#?}", messages);
                for (column, value) in row {
                    if column == "message" || column == "row_number" {
                        continue;
                    }
                    let mut tmp_cell = SerdeMap::new();
                    tmp_cell.insert("value".to_string(), value.clone());
                    let tmp_messages = {
                        let mut tmp_messages = vec![];
                        for m in &mut messages {
                            //tracing::info!("MMMM: {:?}", m);
                            if let Some(SerdeValue::String(mcol)) = m.get("column") {
                                //tracing::info!("MCOL: {}", mcol);
                                if mcol == column {
                                    let mut m = m.as_object_mut().unwrap();
                                    m.remove("column");
                                    tmp_messages.push(m.clone());
                                    // Overwrite the value in the tmp_cell:
                                    tmp_cell.insert(
                                        "value".to_string(),
                                        m.get("value").unwrap().clone(),
                                    );
                                    m.remove("value");
                                }
                            }
                        }
                        tmp_messages
                    };
                    //tracing::info!("TMP MESSAGES: {:#?}", tmp_messages);
                    tmp_cell.insert("messages".to_string(), json!(tmp_messages));
                    tmp_cell.insert("valid".to_string(), json!(tmp_messages.is_empty()));
                    //tracing::info!("COLUMN: {}, TMP_CELL: {:#?}", column, tmp_cell);
                    tmp.insert(column.to_string(), json!(tmp_cell));
                }
                tmp
            };
            //tracing::info!("METAFIED ROW: {:#?}", metafied_row);
            form_html = Some(get_row_as_form(state, config, table, &metafied_row)?);
        }

        let table_url = match term_id {
            Some(term_id) => match uri::Builder::new()
                .path_and_query(format!("/{}?term_id={}", table, term_id))
                .build()
            {
                Ok(url) => url,
                Err(e) => return Err(e.to_string().into()),
            },
            None => match uri::Builder::new()
                .path_and_query(format!("/{}/row/{}", table, row_number))
                .build()
            {
                Ok(url) => url,
                Err(e) => return Err(e.to_string().into()),
            },
        };
        tracing::info!("TABLE_URL: {}", table_url);
        tracing::info!("MESSAGES: {:#?}", messages);
    }

    let form_html = match form_html {
        Some(f) => f,
        None => {
            let error = "Something went wrong - unable to render form".to_string();
            return Err((StatusCode::BAD_REQUEST, Html(error)).into_response().into());
        }
    };
    //tracing::info!("FORM HTML: {}", form_html);
    let page = json!({
        "page": {
            "project_name": "Nanobot",
            "tables": {
                "table": "/table",
                "column": "/column",
                "datatype": "/datatype",
                "message": "/message",
                "penguin": "/penguin"
            },
        },
        "title": "table",
        "messages": messages,
        "row_form": form_html,
    });
    tracing::info!("PAGE: {}", page);
    let page_html = get::page_to_html_form(&page).unwrap();
    Ok(Html(page_html).into_response())
}

fn get_messages(row: &SerdeMap) -> HashMap<String, Vec<String>> {
    let mut messages = HashMap::new();
    for (header, details) in row {
        if header == "row_number" {
            continue;
        }
        // TODO: Remove unwraps.
        if let Some(SerdeValue::Array(row_messages)) = details.get("messages") {
            for msg in row_messages {
                match msg.get("level") {
                    Some(level) if level == "error" => {
                        if !messages.contains_key("error") {
                            messages.insert("error".to_string(), vec![]);
                        }
                        let mut error_list = messages.get_mut("error").unwrap();
                        let error_msg = msg.get("message").unwrap().as_str().unwrap();
                        error_list.push(error_msg.to_string());
                    }
                    Some(level) if level == "warn" => {
                        if !messages.contains_key("warn") {
                            messages.insert("warn".to_string(), vec![]);
                        }
                        let mut warn_list = messages.get_mut("warn").unwrap();
                        let warn_msg = msg.get("message").unwrap().as_str().unwrap();
                        warn_list.push(warn_msg.to_string());
                    }
                    Some(level) if level == "info" => {
                        if !messages.contains_key("info") {
                            messages.insert("info".to_string(), vec![]);
                        }
                        let mut info_list = messages.get_mut("info").unwrap();
                        let info_msg = msg.get("message").unwrap().as_str().unwrap();
                        info_list.push(info_msg.to_string());
                    }
                    Some(level) => tracing::warn!("Unrecognized level '{}' in {}", level, msg),
                    None => tracing::warn!("Message: {} has no 'level'. Ignoring it.", msg),
                };
            }
        }
    }
    messages
}

fn get_sql_tables(config: &ValveConfig) -> Result<Vec<String>, String> {
    match config
        .config
        .get("table")
        .and_then(|t| t.as_object())
        .and_then(|t| Some(t.keys().cloned().collect::<Vec<_>>()))
    {
        Some(tables) => Ok(tables),
        None => Err(format!("Unable to retrieve table config from valve config: {:#?}", config)),
    }
}

fn get_sql_columns(table: &str, config: &ValveConfig) -> Result<Vec<String>, String> {
    match config
        .config
        .get("table")
        .and_then(|t| t.as_object())
        .and_then(|t| t.get(table))
        .and_then(|t| t.as_object())
        .and_then(|t| t.get("column"))
        .and_then(|c| c.as_object())
        .and_then(|c| Some(c.iter()))
        .and_then(|c| Some(c.map(|(k, v)| k.clone())))
        .and_then(|c| Some(c.collect::<Vec<_>>()))
    {
        None => Err(format!("Unable to retrieve columns of '{}' from valve configuration.", table)),
        Some(v) => Ok(v),
    }
}

fn get_column_config(table: &str, column: &str, config: &ValveConfig) -> Result<SerdeMap, String> {
    //tracing::info!("VALVE CONFIG: {:#?}", config.config);
    //tracing::info!("TABLE: {}, COLUMN: {}", table, column);
    match config
        .config
        .get("table")
        .and_then(|t| t.as_object())
        .and_then(|t| t.get(table))
        .and_then(|t| t.as_object())
        .and_then(|t| t.get("column"))
        .and_then(|c| c.as_object())
        .and_then(|c| c.get(column))
        .and_then(|c| c.as_object())
    {
        Some(c) => Ok(c.clone()),
        None => Err("Unable to retrieve column config from Valve configuration".to_string()),
    }
}

fn get_html_type_and_values(
    config: &ValveConfig,
    datatype: &str,
    values: &Option<Vec<String>>,
) -> Result<(Option<String>, Option<Vec<String>>), String> {
    let dt_config = match config
        .config
        .get("datatype")
        .and_then(|d| d.as_object())
        .and_then(|d| d.get(datatype))
        .and_then(|d| d.as_object())
    {
        Some(o) => o,
        None => return Err(format!("Unable to retrieve datatype config for '{}'", datatype)),
    };

    let mut new_values = vec![];
    match values {
        None => match config.datatype_conditions.get(datatype) {
            None => {
                return Err(format!("Could not retrieve datatype condition for '{}'", datatype))
            }
            Some(compiled_condition) => match &compiled_condition.parsed {
                Expression::Function(name, args) if name == "in" => {
                    for arg in args {
                        match &**arg {
                            Expression::Label(l) => {
                                new_values.push(unquote(l).unwrap_or(l.to_string()))
                            }
                            _ => {
                                return Err(format!(
                                    "Unsupported arg: '{:?}' in condition: {:?}",
                                    arg, compiled_condition
                                ))
                            }
                        };
                    }
                }
                _ => (),
            },
        },
        Some(values) => new_values = values.to_vec(),
    };
    let new_values = {
        if new_values.is_empty() {
            None
        } else {
            Some(new_values)
        }
    };

    //tracing::info!("Looking for html type for datatype: {}", datatype);

    if let Some(html_type) = dt_config.get("HTML type").and_then(|t| t.as_str()) {
        if !html_type.is_empty() {
            //tracing::info!("Got html type: {} and values: {:?}. Returning", html_type, new_values);
            return Ok((Some(html_type.to_string()), new_values));
        }
    }

    if let Some(parent) = dt_config.get("parent").and_then(|t| t.as_str()) {
        if !parent.is_empty() {
            //tracing::info!("Could not find html type. Trying with {} and {:?}", parent, new_values);
            return get_html_type_and_values(config, parent, &new_values);
        }
    }

    Ok((None, None))
}

fn is_ontology(table: &str, config: &ValveConfig) -> Result<bool, String> {
    let columns = get_sql_columns(table, config)?;
    Ok(columns.contains(&"subject".to_string())
        && columns.contains(&"predicate".to_string())
        && columns.contains(&"object".to_string())
        && columns.contains(&"datatype".to_string())
        && columns.contains(&"annotation".to_string()))
}

fn validate_table_row(
    table_name: &str,
    row_data: &SerdeMap,
    row_number: &Option<u32>,
    state: &Arc<AppState>,
) -> Result<SerdeMap, String> {
    let (vconfig, dt_conds, rule_conds) = match &state.config.valve {
        Some(v) => (&v.config, &v.datatype_conditions, &v.rule_conditions),
        None => return Err(format!("Valve configuration is undefined in {:?}", state.config)),
    };
    let pool = match state.config.pool.as_ref() {
        Some(p) => p,
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
                Some(*row_number),
            ))
            .unwrap() // TODO: Remove unwrap
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
        .unwrap(), // TODO: Remove unwrap
    };
    Ok(validated_row)
}

fn get_row_as_form(
    state: &Arc<AppState>,
    config: &ValveConfig,
    table_name: &str,
    row_data: &SerdeMap,
) -> Result<String, String> {
    let mut html = vec![json!("html"), json!(["form", {"method": "post"}])];
    tracing::info!("HTML: {:?}", html);
    let mut row_valid = None;
    let mut form_row_id = 0;
    for (cell_header, cell_value) in row_data.iter() {
        if cell_header == "row_number" {
            continue;
        }

        tracing::info!("GOT CELL VALUE: {:#?}", cell_value);
        let mut valid = false;
        let mut value = json!("");
        let messages;
        match cell_value.as_object() {
            None => return Err(format!("Cell value: {:?} is not an object.", cell_value)),
            Some(o) => {
                match o.get("valid") {
                    Some(SerdeValue::Bool(v)) => valid = *v,
                    _ => return Err(format!("No flag called 'valid' in {:?}", o)),
                };
                match o.get("value") {
                    Some(v) => value = v.clone(),
                    _ => return Err(format!("No 'value' in {:?}", o)),
                };
                match o.get("messages") {
                    Some(SerdeValue::Array(v)) => messages = v.to_vec(),
                    _ => return Err(format!("No array called 'messages' in {:?}", o)),
                };
            }
        };

        match row_valid {
            None if !valid => row_valid = Some(false),
            None if valid => row_valid = Some(true),
            Some(true) if !valid => row_valid = Some(false),
            _ => (),
        };

        if valid && row_valid == None {
            row_valid = Some(true)
        } else if !valid {
            row_valid = Some(false)
        }

        tracing::info!("MESSAGES FOR {}.{}: {:?}", table_name, cell_header, messages);
        let message = {
            let mut tmp = vec![];
            for m in messages {
                match m.as_object() {
                    None => return Err(format!("{:?} is not an object.", m)),
                    Some(message) => match message.get("message") {
                        None => return Err(format!("No 'message' in {:?}", message)),
                        Some(message) => {
                            let message = match message.as_str() {
                                Some(message) => tmp.push(message.to_string()),
                                None => return Err(format!("{} is not a str", message)),
                            };
                        }
                    },
                };
            }
            tmp.join("<br>")
        };
        //tracing::info!("MESSAGES FOR {}.{} (as a string): {}", table_name, cell_header, message);

        let mut html_type = Some("text".into());
        let column_config = get_column_config(table_name, cell_header, config)?;
        //tracing::info!("COLUMN CONFIG: {:#?}", column_config);
        let description = match column_config.get("description") {
            Some(d) => match d.as_str().and_then(|d| Some(d.to_string())) {
                None => return Err(format!("Could not convert '{}' to string", d)),
                Some(d) => d,
            },
            None => return Err(format!("No 'description' in {:?}", column_config)),
        };
        let datatype = match column_config.get("datatype") {
            Some(d) => match d.as_str().and_then(|d| Some(d.to_string())) {
                None => return Err(format!("Could not convert '{}' to string", d)),
                Some(d) => d,
            },
            None => return Err(format!("No 'datatype' in {:?}", column_config)),
        };
        let structure = match column_config.get("structure") {
            Some(d) => match d.as_str() {
                None => return Err(format!("{} is not a str", d)),
                Some(d) => d.split('(').collect::<Vec<_>>()[0],
            },
            None => return Err(format!("No 'structure' in {:?}", column_config)),
        };

        //tracing::info!("D,D,S: {}, {}, {}", description, datatype, structure);

        let mut allowed_values = None;
        if vec!["from", "in", "tree", "under"].contains(&structure) {
            html_type = Some("search".into());
        } else {
            (html_type, allowed_values) = get_html_type_and_values(config, &datatype, &None)?;
        }
        //tracing::info!("HTML TYPE IS {:?}", html_type);

        if allowed_values != None && html_type == None {
            html_type = Some("search".into());
        }

        let readonly;
        match html_type {
            Some(s) if s == "readonly" => {
                readonly = true;
                html_type = Some("text".into());
            }
            _ => readonly = false,
        };

        let mut hiccup_form_row = get_hiccup_form_row(
            state,
            cell_header,
            &None,
            &allowed_values,
            &None,
            &Some(description),
            &None,
            &html_type,
            &Some(message),
            &Some(readonly),
            &Some(valid),
            &Some(value),
            form_row_id,
        )?;
        html.push(json!(hiccup_form_row));
    }

    let submit_cls = match row_valid {
        Some(flag) => {
            if flag {
                "success"
            } else {
                "danger"
            }
        }
        None => "secondary", // Row has not yet been validated - display gray button.
    };

    html.push(json!([
        "div",
        {"class": "row", "style": "padding-top: 10px;"},
        [
            "div",
            {"class": "col-auto"},
            [
                "button",
                {
                    "type": "submit",
                    "name": "action",
                    "value": "validate",
                    "class": "btn btn-large btn-outline-primary",
                },
                "Validate",
            ],
        ],
        [
            "div",
            {"class": "col-auto"},
            [
                "button",
                {
                    "type": "submit",
                    "name": "action",
                    "value": "submit",
                    "class": format!("btn btn-large btn-outline-{}", submit_cls),
                },
                "Submit",
            ],
        ],
    ]));

    let page_hiccup = hiccup::render(&json!(html)).unwrap();
    //tracing::info!("PAGE HICCUP: {}", page_hiccup);
    Ok(page_hiccup)
}

fn get_hiccup_form_row(
    mut state: &Arc<AppState>,
    header: &str,
    allow_delete: &Option<bool>,
    allowed_values: &Option<Vec<String>>,
    annotations: &Option<SerdeMap>,
    description: &Option<String>,
    display_header: &Option<String>,
    html_type: &Option<String>,
    message: &Option<String>,
    readonly: &Option<bool>,
    valid: &Option<bool>,
    value: &Option<SerdeValue>,
    mut form_row_id: usize,
) -> Result<Vec<SerdeValue>, String> {
    let allow_delete = match allow_delete {
        None => false,
        Some(b) => *b,
    };
    let readonly = match readonly {
        None => false,
        Some(b) => *b,
    };
    let html_type = match html_type {
        None => "text",
        Some(t) => t,
    };
    if vec!["select", "radio", "checkbox"].contains(&html_type) && *allowed_values == None {
        return Err(format!("A list of allowed values is required for HTML type '{}'", html_type));
    }

    // Create the header lavel for this form row:
    let mut header_col = vec![json!("div"), json!({"class": "col-md-3", "id": form_row_id})];
    if allow_delete {
        header_col.push(json!([
            json!("a"),
            json!({ "href": format!("javascript:del({})", form_row_id) }),
            json!(["i", {"class": "bi-x-circle", "style": "font-size: 16px; color: #dc3545;"}]),
            json!("&nbsp"),
        ]));
    }
    form_row_id += 1;

    match display_header {
        Some(d) => header_col.push(json!([json!("b"), json!(d)])),
        None => header_col.push(json!([json!("b"), json!(header)])),
    };

    if let Some(description) = description {
        header_col.push(json!([
            json!("button"),
            json!({
                "class": "btn",
                "data-bs-toggle": "tooltip",
                "data-bs-placement": "right",
                "title": description,
            }),
            json!(["i", {"class": "bi-question-circle"}]),
        ]));
    }

    //tracing::info!("HEADER COL: {:#?}", header_col);

    // Create the value input for this form row:
    let mut classes = vec![];
    match valid {
        Some(flag) if *flag => classes.push("is-valid"),
        _ => classes.push("is-invalid"),
    };

    let mut input_attrs = SerdeMap::new();
    if readonly {
        input_attrs.insert("readonly".to_string(), json!(true));
    } else {
        input_attrs.insert("name".to_string(), json!(header));
    }

    //tracing::info!("GET HICCUP FORM ROW HTML TYPE: {}", html_type);
    let mut value_col = vec![json!("div"), json!({"class": "col-md-9 form-group"})];
    if vec!["textarea", "input"].contains(&html_type) {
        //tracing::info!("TEXTAREA OR INPUT");
        classes.insert(0, "form-control");
        input_attrs.insert("class".to_string(), json!(classes.join(" ")));
        let mut element = vec![json!(html_type), json!(input_attrs)];
        if let Some(value) = value {
            if let Some(v) = value.as_str() {
                let mut empty = String::new();
                let value = encode_text_to_string(v, &mut empty);
                element.push(json!(value));
            }
        }
        value_col.push(json!(element));
    } else if html_type == "select" {
        //tracing::info!("SELECT");
        classes.insert(0, "form-select");
        input_attrs.insert("class".to_string(), json!(classes.join(" ")));
        let mut select_element = vec![json!("select"), json!(input_attrs)];
        let mut has_selected = false;
        if let Some(allowed_values) = allowed_values {
            for av in allowed_values {
                let mut empty = String::new();
                let av_safe = encode_text_to_string(av, &mut empty);
                match value {
                    Some(value) if value == av => {
                        has_selected = true;
                        select_element.push(json!([
                            json!("option"),
                            json!({"value": av_safe, "selected": true}),
                            json!(av_safe),
                        ]));
                    }
                    _ => {
                        select_element.push(json!([
                            json!("option"),
                            json!({ "value": av_safe }),
                            json!(av_safe),
                        ]));
                    }
                };
            }
        }

        // Add an empty string for no value at the start of the options
        if has_selected {
            select_element.insert(2, json!(["option", {"value": ""}]));
        } else {
            // If there is currently no value, make sure this one is selected
            select_element.insert(2, json!(["option", {"value": "", "selected": true}]));
        }
        value_col.push(json!(select_element));
        //tracing::info!("VALUE COL FOR SELECT: {:?}", value_col);
    } else if vec!["text", "number", "search"].contains(&html_type) {
        //tracing::info!("TEXT NUMBER SEARCH");
        // TODO: Support a range restriction for 'number'
        classes.insert(0, "form-control");
        input_attrs.insert("type".to_string(), json!(html_type));
        if html_type == "search" {
            classes.append(&mut vec!["search", "typeahead"]);
            input_attrs.insert("id".to_string(), json!(format!("{}-typeahead-form", header)));
        }
        input_attrs.insert("class".to_string(), json!(classes.join(" ")));
        if let Some(value) = value {
            if let Some(v) = value.as_str() {
                let mut empty = String::new();
                let value = encode_text_to_string(v, &mut empty);
                input_attrs.insert("value".to_string(), json!(value));
            }
        }
        value_col.push(json!([json!("input"), json!(input_attrs)]));
        //tracing::info!("VALUE COL: {:#?}", value_col);
    } else if html_type == "radio" {
        //tracing::info!("RADIO");
        // TODO: what if value is not in allowed_values? Or what if there is no value?
        classes.insert(0, "form-check-input");
        input_attrs.insert("type".to_string(), json!(html_type));
        input_attrs.insert("class".to_string(), json!(classes.join(" ")));
        if let Some(allowed_values) = allowed_values {
            for av in allowed_values {
                let mut empty = String::new();
                let av_safe = encode_text_to_string(av, &mut empty);
                let mut attrs_copy = input_attrs.clone();
                attrs_copy.insert("value".to_string(), json!(av_safe));
                if let Some(value) = value {
                    if value == av {
                        attrs_copy.insert("checked".to_string(), json!(true));
                    }
                }
                value_col.push(json!([
                    json!("div"),
                    json!([json!("input"), json!(attrs_copy)]),
                    json!([
                        json!("label"),
                        json!({"class": "form-check-label", "for": av_safe}),
                        json!(av_safe),
                    ]),
                ]));
            }
        }
        //tracing::info!("VALUE COL FOR RADIO: {:#?}", value_col);

        let mut attrs_copy = input_attrs.clone();
        attrs_copy.insert("value".to_string(), json!(""));
        let mut input_attrs: SerdeMap = match serde_json::from_str(&format!(
            r#"{{
                 "type": "text",
                 "class": "form-control",
                 "name": {} + "_other",
                 "placeholder": "other",
               }}"#,
            header,
        )) {
            Ok(a) => a,
            Err(e) => return Err(e.to_string()),
        };

        if let Some(value) = value {
            if let Some(value) = value.as_str() {
                if let Some(allowed_values) = allowed_values {
                    if !allowed_values.contains(&value.to_string()) {
                        attrs_copy.insert("checked".to_string(), json!(true));
                        input_attrs.insert("value".to_string(), json!(value));
                    }
                }
            }
        }
        let mut e = vec![
            json!("div"),
            json!(["input", attrs_copy]),
            json!(["label", {"class": "form-check-label", "for": "other"}, ["input", input_attrs]]),
        ];
        if let Some(message) = message {
            let validation_cls = {
                match valid {
                    Some(flag) if *flag => "valid-feedback",
                    _ => "invalid-feedback",
                }
            };
            e.push(json!([json!("div"), json!({ "class": validation_cls }), json!(message),]));
        }
        value_col.push(json!(e));
    } else {
        //tracing::info!("ERROR");
        return Err(format!("'{}' form field is not supported for column '{}'", html_type, header));
    }

    match message {
        Some(message) if html_type != "radio" => {
            let validation_cls = {
                match valid {
                    Some(flag) if *flag => "valid-feedback",
                    _ => "invalid-feedback",
                }
            };
            value_col.push(json!([
                json!("div"),
                json!({ "class": validation_cls }),
                json!(message),
            ]));
        }
        _ => (),
    };

    ////////////////////////////////////////////
    // Remove these statements later:
    let mut annotations = SerdeMap::new();
    let my_leg = json!([
        {"object": "Pep \"Mr. Blue\" Guardiola"},
        {"object": "Kevin De Bruyne"},
    ]);
    let my_arm = json!([
        {"object": "Jürgen \"Mr. Red\" Klopp"},
        {"object": "Trent Alexander-Arnold"},
    ]);
    annotations.insert("MyFoot".to_string(), my_leg);
    annotations.insert("MyHand".to_string(), my_arm);
    let annotations = Some(annotations);
    ////////////////////////////////////////////

    if let Some(annotations) = annotations {
        // TODO: This code is weird. It seems like ann_html is assigned on every iteration,
        // but then the value is thrown away at the end of each iteration. It could be that there
        // is a bug and that the statement:
        //   `value_col.push(ann_html);`
        // should happen inside one of the for loops. Otherwise this code is extremely inefficient,
        // and the best thing to do is just to take the *last* tuple in `annotations` and
        // also the *last* element of `ann_values`.
        let mut ann_html = json!([]);
        for (ann_pred, ann_values) in annotations {
            for av in ann_values.as_array().unwrap() {
                let av = match av.as_object() {
                    Some(av) => match av.get("object") {
                        Some(o) => match o.as_str() {
                            Some(s) => s,
                            None => return Err(format!("{:?} is not a str", o)),
                        },
                        None => return Err(format!("No 'object' in {:?}", av)),
                    },
                    None => return Err(format!("{:?} is not an object.", av)),
                };
                //tracing::info!("AV FOR {}: {}", ann_pred, av);
                ann_html = json!([
                    "div",
                    {
                        "class": "row justify-content-end",
                        "style": "padding-right: 0px; padding-top: 5px;",
                    },
                    [
                        "div",
                        {"class": "col-sm-9"},
                        [
                            "div",
                            {"class": "row"},
                            [
                                "label",
                                {
                                    "class": "col-sm-2 col-form-label",
                                    "style": "padding-left: 20px !important;",
                                },
                                format!("{}", ann_pred),
                            ],
                            [
                                "div",
                                {"class": "col-sm-10", "style": "padding-right: 0px !important;"},
                                [
                                    "input",
                                    {
                                        "type": "text",
                                        "class": "form-control",
                                        "value": format!("{}", av.replace('"', "&quot;")),
                                    },
                                ],
                            ],
                        ],
                    ],
                ]);
                //tracing::info!("ANN HTML: {}", ann_html);
            }
        }
        value_col.push(ann_html);
    }

    Ok(vec![json!("div"), json!({"class": "row py-1"}), json!(header_col), json!(value_col)])
}
