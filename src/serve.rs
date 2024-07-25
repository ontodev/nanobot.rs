use crate::{config::Config, get, ldtab, tree_view};
use ansi_to_html;
use axum::{
    extract::{Form, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect},
    routing::get,
    Router,
};
use chrono::prelude::Local;
use enquote::unquote;
use futures::executor::block_on;
use html_escape::encode_text_to_string;
use ontodev_hiccup::hiccup;
use ontodev_sqlrest::{parse, Filter, Select, SelectColumn};
use ontodev_valve::{
    ast::Expression,
    valve::{Valve, ValveColumnConfig},
};
use regex::{Captures, Regex};
use serde_json::{json, Value as SerdeValue};
use std::{
    collections::HashMap, collections::HashSet, net::SocketAddr, process::Command, sync::Arc,
};
use tokio::signal;
use tower_http::services::ServeDir;
use wiring_rs::util::signature;

#[derive(Debug, PartialEq, Eq)]
enum RequestType {
    POST,
    GET,
}

#[derive(Debug)]
pub struct AppState {
    pub config: Config,
}

pub type RequestParams = HashMap<String, String>;
/// An alias for [serde_json::Map](..//serde_json/struct.Map.html)<String, [serde_json::Value](../serde_json/enum.Value.html)>.
// Note: serde_json::Map is
// [backed by a BTreeMap by default](https://docs.serde.rs/serde_json/map/index.html) which can be
// overriden by specifying the preserve-order feature in Cargo.toml, which we have indeed specified.
pub type SerdeMap = serde_json::Map<String, SerdeValue>;

pub fn build_app(shared_state: Arc<AppState>) -> Router {
    let asset_path = shared_state.config.asset_path.clone();
    // build our application with a route
    let router = Router::new()
        .route("/", get(root))
        .route("/:table", get(get_table).post(post_table))
        .route("/:table/:subject", get(get_tree))
        .route("/:table/row/:row_number", get(get_row).post(post_row))
        .with_state(shared_state);
    if let Some(asset_path) = asset_path {
        let serve_dir = ServeDir::new(asset_path);
        tracing::debug!("Serving static assets from {:?}", serve_dir);
        Router::new()
            .nest_service("/assets", serve_dir)
            .merge(router)
    } else {
        router
    }
}

#[tokio::main]
pub async fn app(config: &Config) -> Result<String, String> {
    let shared_state = Arc::new(AppState {
        //TODO: use &config instead of config.clone()?
        config: config.clone(),
    });

    let app = build_app(shared_state);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    println!("Running Nanobot server at http://{addr}");
    println!("Press Control-C to quit.");
    if let Err(e) = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        return Err(e.to_string());
    }

    Ok("Stopping Nanobot server...".into())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

async fn root() -> impl IntoResponse {
    tracing::info!("request root");
    Redirect::permanent("table")
}

async fn post_table(
    Path(path): Path<String>,
    state: State<Arc<AppState>>,
    Query(query_params): Query<RequestParams>,
    Form(form_params): Form<RequestParams>,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!(
        "request table POST {:?}, Query Params: {:?}, Form Params: {:?}",
        path,
        query_params,
        form_params
    );
    let mut request_type = RequestType::POST;
    let valve = state
        .config
        .valve
        .as_ref()
        .ok_or("Valve is not initialized.".to_string())?;
    if form_params.contains_key("save") {
        tracing::info!("SAVE");
        valve
            .save_all_tables(&None)
            .map_err(|e| format!("{:?}", e))?;
        request_type = RequestType::GET;
    } else if form_params.contains_key("undo") {
        tracing::info!("UNDO");
        block_on(valve.undo()).expect("Undo should succeed");
        request_type = RequestType::GET;
    } else if form_params.contains_key("redo") {
        tracing::info!("REDO");
        block_on(valve.redo()).expect("Redo should succeed");
        request_type = RequestType::GET;
    }
    table(&path, &state, &query_params, &form_params, request_type).await
}

async fn get_table(
    Path(path): Path<String>,
    State(state): State<Arc<AppState>>,
    Query(query_params): Query<RequestParams>,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!("request table GET {:?} {:?}", path, query_params);
    table(
        &path,
        &state,
        &query_params,
        &RequestParams::new(),
        RequestType::GET,
    )
    .await
}

fn action(
    path: &String,
    state: &Arc<AppState>,
    query_params: &RequestParams,
) -> axum::response::Result<impl IntoResponse> {
    let action_name = match query_params.get("user.action") {
        Some(a) => a,
        None => {
            return Err((StatusCode::BAD_REQUEST, Html("No user action specified"))
                .into_response()
                .into())
        }
    };

    let action = match &state.config.actions.get(action_name) {
        Some(a) => a.to_owned(),
        None => {
            let message = format!("Undefined user action '{}'", action_name);
            return Err((StatusCode::BAD_REQUEST, Html(message))
                .into_response()
                .into());
        }
    };

    let mut values: HashMap<String, String> = HashMap::new();
    if let Ok(output) = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
    {
        let stdout = std::str::from_utf8(&output.stdout)
            .unwrap_or_default()
            .trim();
        values.insert("username".into(), stdout.into());
    }
    values.insert("today".into(), Local::now().format("%Y-%m-%d").to_string());

    let satisfied = match action.inputs {
        Some(_) => {
            let mut result = true;
            for input in action.inputs.clone().unwrap_or_default() {
                match query_params.get(&input.name) {
                    Some(v) => {
                        if !v.trim().is_empty() {
                            values.insert(input.name, v.into());
                            ()
                        } else {
                            result = false
                        }
                    }
                    None => result = false,
                }
            }
            result
        }
        None => true,
    };

    tracing::info!("VALUES {values:?}");

    let re = Regex::new(r"\{(\w+)\}").unwrap();
    let mut inputs = vec![];
    let mut results = vec![];

    if !satisfied {
        for mut input in action.inputs.clone().unwrap_or_default() {
            if let Some(v) = values.get(&input.name) {
                if !v.trim().is_empty() {
                    input.value = Some(v.to_string());
                }
            } else if let Some(default) = input.default.clone() {
                tracing::debug!("INPUT DEFAULT {:?}", input.clone());
                let subbed =
                    re.replace_all(&default, |caps: &Captures| match values.get(&caps[1]) {
                        Some(v) => v,
                        None => "",
                    });
                input.value = Some(subbed.to_string());
            };
            inputs.push(input);
        }
    } else {
        for command in action.commands.iter() {
            tracing::debug!("RUNNING {:?}", command);
            let mut run = Command::new(&command[0]);
            let mut parts = vec![command[0].to_string()];
            for arg in &command[1..] {
                let subbed = re.replace_all(arg, |caps: &Captures| match values.get(&caps[1]) {
                    Some(v) => v,
                    None => "",
                });
                run.arg(subbed.to_string());
                parts.push(subbed.to_string());
            }
            tracing::debug!("COMMAND {:?}", run);
            let output = run.output().expect("Command failed!");
            tracing::debug!("OUTPUT {:?}", output);
            let status = output
                .status
                .code()
                .ok_or("Bad exit status")
                .unwrap_or_default();
            let stdout = std::str::from_utf8(&output.stdout).unwrap_or_default();
            let stderr = std::str::from_utf8(&output.stderr).unwrap_or_default();
            let result = json!({
                "command": parts.join(" "),
                "status": status,
                "stdout": ansi_to_html::convert_escaped(stdout).unwrap(),
                "stderr": ansi_to_html::convert_escaped(stderr).unwrap(),
            });
            results.push(result);
            if status != 0 {
                break;
            }
        }
    }

    let root = if path.contains("/") { "../../" } else { "" };
    tracing::debug!("ROOT! {root} {path}");
    let valve = &state
        .config
        .valve
        .as_ref()
        .ok_or("Valve is not initialized.".to_string())?;
    let table_map = {
        let mut table_map = SerdeMap::new();
        for table in &valve.config.table_order {
            if table == "history" {
                continue;
            }
            table_map.insert(table.to_string(), json!(table.clone()));
        }
        json!(table_map)
    };
    let page = json!({
        "page": {
            "root": root,
            "project_name": "Nanobot",
            "tables": table_map,
            "undo": get::get_undo_message(&state.config),
            "redo": get::get_redo_message(&state.config),
            "actions": get::get_action_map(&state.config).unwrap_or_default(),
            "repo": get::get_repo_details().unwrap_or_default(),
        },
        "title": action.label,
        "action_name": action_name,
        "action": action,
        "inputs": inputs,
        "results": results,
    });

    if path.ends_with(".pretty.json") {
        Ok(serde_json::to_string_pretty(&page)
            .unwrap_or_default()
            .into_response())
    } else if path.ends_with(".json") {
        Ok(Json(page).into_response())
    } else {
        let page_html = match get::page_to_html(&state.config, "action", &page) {
            Ok(p) => p,
            Err(e) => return Err(e.to_string().into()),
        };
        Ok(Html(page_html).into_response())
    }
}

async fn get_tree(
    Path((table, subject)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
    Query(params): Query<RequestParams>,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!("request tree GET {:?} {:?} {:?}", table, subject, params);
    let tree = tree(&state, &table, &subject, &params).await?;
    Ok(tree.into_response())
}

async fn tree(
    state: &Arc<AppState>,
    table: &str,
    subject: &str,
    params: &RequestParams,
) -> axum::response::Result<impl IntoResponse> {
    // TODO: Just hacking!
    if params.contains_key("user.action") {
        let result = action(&table.into(), state, params)?;
        return Ok(result.into_response());
    }

    if table.contains(" ") {
        return Ok(tree2(state, table, subject, params).await?.into_response());
    };

    let pool = &state
        .config
        .pool
        .as_ref()
        .ok_or("Pool is not initialized.".to_string())?;

    // Return JSON-LD
    if subject.ends_with(".json") {
        let term_id = subject
            .trim_end_matches(".json")
            .trim_end_matches(".pretty");

        let content = ldtab::get_property_map(&term_id, "ontology", &pool)
            .await
            .unwrap_or_default();
        let mut iris = HashSet::from([term_id.into()]);
        signature::get_iris(&content, &mut iris);
        let prefixes = ldtab::get_prefix_hash_map(&iris, &pool).await.unwrap();
        let mut context = SerdeMap::new();
        for prefix in prefixes.keys() {
            context.insert(
                prefix.into(),
                json!({"@id": prefixes.get(prefix), "@prefix": true}),
            );
        }

        let specials = HashSet::from(["rdf:type", "rdfs:subClassOf", "owl:equivalentClass"]);
        let mut sorted_iris = Vec::from_iter(iris);
        sorted_iris.sort_unstable();
        for iri in sorted_iris {
            if specials.contains(&iri.as_str()) {
                context.insert(
                    iri.into(),
                    json!({
                        "@type": "@id"
                    }),
                );
            }
        }

        let mut result = SerdeMap::new();
        result.insert("@context".into(), json!(context));
        result.insert("@id".into(), json!(term_id));
        let content = content.as_object().unwrap();
        for key in content.keys() {
            let values = content.get(key).unwrap().as_array().unwrap();
            let mut list = vec![];
            for value in values {
                let value = value.as_object().unwrap();
                if let Some(object) = value.get("object") {
                    if let Some(datatype) = value.get("datatype") {
                        let datatype = datatype.as_str().unwrap();
                        match datatype {
                            "_IRI" => {
                                if specials.contains(&key.as_str()) {
                                    // predicate has @type=@id
                                    list.push(json!(object));
                                } else {
                                    list.push(json!({ "@id": object }))
                                }
                            }
                            "_JSON" => (),
                            "xsd:string" => list.push(json!(object)),
                            d => {
                                if d.starts_with("@") {
                                    list.push(json!({
                                        "@value": object,
                                        "@language": datatype.replace("@", "")
                                    }))
                                } else {
                                    list.push(json!({
                                        "@value": object,
                                        "@type": datatype
                                    }))
                                }
                            }
                        }
                    }
                }
            }
            if list.len() > 1 {
                result.insert(key.into(), json!(list));
            } else {
                result.insert(key.into(), json!(list[0]));
            }
        }
        if subject.ends_with(".pretty.json") {
            return Ok(serde_json::to_string_pretty(&result)
                .unwrap_or_default()
                .into_response());
        } else {
            return Ok(Json(result).into_response());
        }
    }

    if let Some(text) = params.get("text") {
        tracing::debug!("TEXT: {text}");
        let search = format!("\"%{text}%\"");
        let mut select = Select::new(table);
        select
            .add_explicit_select(&SelectColumn::new("subject", Some("id"), None))
            .add_explicit_select(&SelectColumn::new("object", Some("label"), None))
            // TODO: filter predicates
            .filter(vec![
                Filter::new("object", "not_eq", json!("\"\""))?,
                Filter::new("object", "ilike", json!(search))?,
                Filter::new("datatype", "eq", json!("\"xsd:string\""))?,
            ])
            .order_by(vec!["LENGTH(object)", "object"])
            .limit(20);
        tracing::debug!("SELECT {:?}", select.to_sqlite());
        let result = select.fetch_rows_as_json(&pool, &HashMap::new())?;
        return Ok(Json(result).into_response());
    }

    tracing::info!("TREE '{table}' {subject}");
    let start = std::time::Instant::now();

    let tree = tree_view::get_hiccup_term_tree(subject, table, &pool)
        .await
        .unwrap_or_default();
    let tree = hiccup::insert_href(&tree, &format!("../{table}/{{curie}}")).unwrap_or_default();
    let tree = hiccup::render(&tree).unwrap_or_default();

    let predicate_order_start: Vec<String> = vec!["rdfs:label".into()];
    let predicate_order_end: Vec<String> =
        vec!["owl:equivalentClass".into(), "rdfs:subClassOf".into()];
    let pred = ldtab::get_predicate_map_hiccup(
        subject,
        table,
        &pool,
        &predicate_order_start,
        &predicate_order_end,
    )
    .await
    .unwrap_or_default();
    let hiccup = pred.clone();
    let pred = hiccup::insert_href(&pred, &format!("../{table}/{{curie}}")).unwrap_or_default();
    let pred = match hiccup::render(&pred) {
        Ok(x) => x,
        Err(x) => format!(
            "ERROR {x} for <pre>{}</pre>",
            serde_json::to_string_pretty(&hiccup).unwrap_or_default()
        ),
    };

    let curies = HashSet::from([subject.to_string()]);
    let labels = ldtab::get_label_hash_map(&curies, table, &pool)
        .await
        .unwrap_or_default();
    let empty = String::new();
    let label = labels.get(subject).unwrap_or(&empty);

    let table_map = {
        let mut table_map = SerdeMap::new();
        let valve = &state
            .config
            .valve
            .as_ref()
            .ok_or("Valve is not initialized.".to_string())?;
        for table in &valve.config.table_order {
            if table == "history" {
                continue;
            }
            table_map.insert(table.to_string(), json!(table.clone()));
        }
        json!(table_map)
    };
    let elapsed = start.elapsed().as_millis() as usize;
    let page = json!({
        "page": {
            "root": "../",
            "project_name": "Nanobot",
            "tables": table_map,
            "undo": get::get_undo_message(&state.config),
            "redo": get::get_redo_message(&state.config),
            "actions": get::get_action_map(&state.config).unwrap_or_default(),
            "repo": get::get_repo_details().unwrap_or_default(),
            "elapsed": elapsed,
        },
        "title": "table",
        "table_name": table,
        "subject": subject,
        "label": label,
        "tree": tree,
        "predicate_map": pred,
        "formats": {
            "JSON-LD": format!("./{subject}.json"),
            "JSON-LD (pretty)": format!("./{subject}.pretty.json"),
        },
    });
    let page_html = match get::page_to_html(&state.config, "tree", &page) {
        Ok(p) => p,
        Err(e) => return Err(e.to_string().into()),
    };
    return Ok(Html(page_html).into_response());
}

async fn tree2(
    state: &Arc<AppState>,
    table: &str,
    subject: &str,
    params: &RequestParams,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!("TREE 2 '{table}' {subject}");
    let start = std::time::Instant::now();

    let (table1, table2) = table.split_once(" ").unwrap();

    let pool = &state
        .config
        .pool
        .as_ref()
        .ok_or("Pool is not initialized.".to_string())?;
    if let Some(text) = params.get("text") {
        tracing::debug!("TEXT: {text}");
        let search = format!("\"%{text}%\"");
        let mut select = Select::new(table2);
        select
            .add_explicit_select(&SelectColumn::new("subject", Some("id"), None))
            .add_explicit_select(&SelectColumn::new("object", Some("label"), None))
            // TODO: filter predicates
            .filter(vec![
                Filter::new("object", "not_eq", json!("\"\""))?,
                Filter::new("object", "ilike", json!(search))?,
                Filter::new("datatype", "eq", json!("\"xsd:string\""))?,
            ])
            .order_by(vec!["LENGTH(object)", "object"])
            .limit(20);
        tracing::debug!("SELECT {:?}", select.to_sqlite());
        let result = select.fetch_rows_as_json(&pool, &HashMap::new())?;
        return Ok(Json(result).into_response());
    }

    let tree1 = tree_view::get_hiccup_term_tree(subject, table1, &pool)
        .await
        .unwrap_or_default();
    let tree1 = hiccup::insert_href(&tree1, &format!("../{table}/{{curie}}")).unwrap_or_default();
    let tree1 = hiccup::render(&tree1).unwrap_or_default();

    let tree2 =
        tree_view::get_hiccup_term_tree(subject, table2, &state.config.pool.as_ref().unwrap())
            .await
            .unwrap_or_default();
    let tree2 = hiccup::insert_href(&tree2, &format!("../{table}/{{curie}}")).unwrap_or_default();
    let tree2 = hiccup::render(&tree2).unwrap_or_default();

    let predicate_order_start: Vec<String> = vec!["rdfs:label".into()];
    let predicate_order_end: Vec<String> =
        vec!["owl:equivalentClass".into(), "rdfs:subClassOf".into()];

    let pred1 = ldtab::get_predicate_map_hiccup(
        subject,
        table1,
        &pool,
        &predicate_order_start,
        &predicate_order_end,
    )
    .await
    .unwrap_or_default();
    let hiccup = pred1.clone();
    let pred1 = hiccup::insert_href(&pred1, &format!("../{table}/{{curie}}")).unwrap_or_default();
    let pred1 = match hiccup::render(&pred1) {
        Ok(x) => x,
        Err(x) => format!(
            "ERROR {x} for <pre>{}</pre>",
            serde_json::to_string_pretty(&hiccup).unwrap_or_default()
        ),
    };

    let pred2 = ldtab::get_predicate_map_hiccup(
        subject,
        table2,
        &pool,
        &predicate_order_start,
        &predicate_order_end,
    )
    .await
    .unwrap_or_default();
    let hiccup = pred2.clone();
    let pred2 = hiccup::insert_href(&pred2, &format!("../{table}/{{curie}}")).unwrap_or_default();
    let pred2 = match hiccup::render(&pred2) {
        Ok(x) => x,
        Err(x) => format!(
            "ERROR {x} for <pre>{}</pre>",
            serde_json::to_string_pretty(&hiccup).unwrap_or_default()
        ),
    };

    let curies = HashSet::from([subject.to_string()]);
    let labels = ldtab::get_label_hash_map(&curies, table, &pool)
        .await
        .unwrap_or_default();
    let empty = String::new();
    let label = labels.get(subject).unwrap_or(&empty);

    let table_map = {
        let mut table_map = SerdeMap::new();
        let valve = &state
            .config
            .valve
            .as_ref()
            .ok_or("Valve is not initialized.".to_string())?;
        for table in &valve.config.table_order {
            if table == "history" {
                continue;
            }
            table_map.insert(table.to_string(), json!(table.clone()));
        }
        json!(table_map)
    };
    let elapsed = start.elapsed().as_millis() as usize;
    let page = json!({
        "page": {
            "root": "../",
            "project_name": "Nanobot",
            "tables": table_map,
            "undo": get::get_undo_message(&state.config),
            "redo": get::get_redo_message(&state.config),
            "actions": get::get_action_map(&state.config).unwrap_or_default(),
            "repo": get::get_repo_details().unwrap_or_default(),
            "elapsed": elapsed,
        },
        "title": "table",
        "table_name": table,
        "table1_name": table1,
        "table2_name": table2,
        "subject": subject,
        "label": label,
        "tree1": tree1,
        "tree2": tree2,
        "predicate_map1": pred1,
        "predicate_map2": pred2,
    });
    let page_html = match get::page_to_html(&state.config, "compare_tree", &page) {
        Ok(p) => p,
        Err(e) => return Err(e.to_string().into()),
    };
    return Ok(Html(page_html).into_response());
}

async fn table(
    path: &String,
    state: &Arc<AppState>,
    query_params: &RequestParams,
    form_params: &RequestParams,
    request_type: RequestType,
) -> axum::response::Result<impl IntoResponse> {
    // TODO: Just hacking!
    if query_params.contains_key("user.action") {
        let result = action(path, state, query_params)?;
        return Ok(result.into_response());
    }

    let (table, format, shape);
    let mut sqlrest_params = query_params.clone();
    sqlrest_params.remove("shape");
    sqlrest_params.remove("view");
    sqlrest_params.remove("format");
    for key in sqlrest_params.clone().keys() {
        if key.starts_with("nb.") {
            sqlrest_params.remove(key);
        }
    }

    if path.ends_with(".pretty.json") {
        table = path.replace(".pretty.json", "");
        format = "pretty.json";
        shape = match query_params.get("shape") {
            Some(s) => s.as_str(),
            None => "page",
        };
    } else if path.ends_with(".json") {
        table = path.replace(".json", "");
        format = "json";
        shape = match query_params.get("shape") {
            Some(s) => s.as_str(),
            None => "page",
        };
    } else if path.ends_with(".tsv") {
        table = path.replace(".tsv", "");
        format = "tsv";
        shape = "value_rows";
    } else if path.ends_with(".csv") {
        table = path.replace(".csv", "");
        format = "csv";
        shape = "value_rows";
    } else if path.ends_with(".txt") {
        table = path.replace(".txt", "");
        format = "text";
        shape = "value_rows";
    } else {
        table = path.clone();
        format = "html";
        shape = "page";
    }
    let config = &state.config;
    let valve = config
        .valve
        .as_ref()
        .ok_or("Valve is not initialized.".to_string())?;
    let mut view = match query_params.get("view") {
        Some(view) => view.to_string(),
        None => "".to_string(),
    };

    // TODO: properly detect LDTab tables
    if !valve.config.table_order.contains(&table) {
        let url = format!("{table}/owl:Class");
        return Ok(Redirect::permanent(&url).into_response());
    }

    // Handle actions such as filtering.
    if query_params.contains_key("nb.action") {
        tracing::debug!("ACTION {:?}", query_params);
        let action = query_params.get("nb.action").unwrap();
        if action == "filter" {
            let column = query_params.get("nb.column").unwrap();
            let operator = query_params.get("nb.operator").unwrap();
            let constraint = query_params.get("nb.constraint").unwrap();
            tracing::debug!("FILTER {}, {}, {}", column, operator, constraint);
            sqlrest_params.insert(column.into(), format!("{}.{}", operator, constraint));
            tracing::debug!("SQLREST {:?}", sqlrest_params);

            let url = {
                let url = sqlrest_params
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>();
                if !url.is_empty() {
                    format!("{}?{}", table, url.join("&"))
                } else {
                    table.to_string()
                }
            };
            return Ok(Redirect::permanent(&url).into_response());
        }
    }

    // Handle requests related to typeahead, used for autocomplete in data forms:
    match query_params.get("format") {
        Some(format) if format == "json" => match query_params.get("column") {
            None => return Err((
                StatusCode::BAD_REQUEST,
                Html(
                    "For format=json, column is also required (e.g., /table?format=json&column=foo)"
                        .to_string(),
                ),
            )
                .into_response()
                .into()),
            Some(column_name) => match valve.get_matching_values(
                &table,
                column_name,
                query_params.get("text").and_then(|t| Some(t.as_str())),
            )
            .await
            {
                Ok(r) => return Ok(Json(r).into_response()),
                Err(e) => {
                    return Err((StatusCode::BAD_REQUEST, Html(format!("{:?}", e)))
                        .into_response()
                        .into())
                }
            },
        },
        _ => (),
    };

    // Handle a POST request to validate or submit a new row for insertion into the table:
    let mut form_map = None;
    let columns = get_columns(&table, valve)?;
    if request_type == RequestType::POST {
        if view == "" {
            view = String::from("form");
        }
        let mut new_row = SerdeMap::new();
        for column in &columns {
            if !vec!["row_number", "row_order"].contains(&column.as_str()) {
                let value = match form_params.get(column) {
                    Some(v) => v.to_string(),
                    None => {
                        let other_column = format!("{}_other", column);
                        form_params
                            .get(&other_column)
                            .unwrap_or(&String::from(""))
                            .to_string()
                    }
                };
                new_row.insert(column.to_string(), value.into());
            }
        }

        let action = match form_params.get("action") {
            None => return Err(format!("No 'action' in {:?}", form_params).into()),
            Some(v) => v,
        };

        if action == "validate" {
            let validated_row = match validate_table_row(&table, &new_row, &None, state) {
                Ok(v) => v,
                Err(e) => return Err(e.into()),
            };
            // If this is a validate action, fill in form_map which will then be handled below.
            match get_row_as_form_map(config, &table, &validated_row) {
                Ok(f) => form_map = Some(f),
                Err(e) => {
                    tracing::debug!("Rendering error 1 {}", e);
                    form_map = None
                }
            };
        } else if action == "submit" {
            // If this is a submit action, insert the row to the database and send back a page
            // containing a javascript redirect as a response which points back to the last
            // page of the table:
            let offset = {
                let row_number = match insert_table_row(&table, &new_row, state) {
                    Ok(n) => n,
                    Err(e) => return Err(e.to_string().into()),
                };
                let pages = row_number / state.config.results_per_page as u32;
                pages * state.config.results_per_page as u32
            };
            let html = format!(
                r#"<script>
                      var timer = setTimeout(function() {{
                        window.location.replace("/{table}?offset={offset}");
                      }}, 1000);
                   </script>
                   The insert operation succeeded. If you are not automatically redirected, click
                   <a href="/{table}?offset={offset}">here</a> to go back to {table}"#,
                table = table,
                offset = offset,
            );
            return Ok(Html(html).into_response());
        }
    }

    // TODO: Improve handling of custom views.
    if view != "" {
        // In this case the request is to view the "insert new row" form:
        if vec!["message", "history"].contains(&&*table) {
            return Err((
                StatusCode::BAD_REQUEST,
                Html(format!("Editing the {} table is not possible", table)),
            )
                .into_response()
                .into());
        }
        if let None = form_map {
            let mut new_row = SerdeMap::new();
            for column in &columns {
                if !vec!["row_number", "row_order"].contains(&column.as_str()) {
                    let value = query_params
                        .get(column)
                        .unwrap_or(&String::from(""))
                        .to_string();
                    // Since this is supposed to be a new row, the initial value of this cell should
                    // match the nulltype (if it exists) of its associated datatype in order to be
                    // valid. Otherwise we mark it as invalid.
                    let valid = matches_nulltype(&table, &column, &value, valve)?;
                    new_row.insert(
                        column.to_string(),
                        json!({
                            "value": value,
                            "valid": valid,
                            "messages": [],
                        }),
                    );
                }
            }
            match get_row_as_form_map(config, &table, &new_row) {
                Ok(f) => form_map = Some(f),
                Err(e) => {
                    tracing::debug!("Rendering error 2 {}", e);
                    form_map = None
                }
            };
        }

        // Used to display a drop-down or menu of some kind containing all the available tables:
        let table_map = {
            let mut table_map = SerdeMap::new();
            for table in &valve.config.table_order {
                if table == "history" {
                    continue;
                }
                table_map.insert(table.to_string(), json!(table.clone()));
            }
            json!(table_map)
        };

        // Fill in the page JSON containing all of the configuration parameters that we will be
        // passing (through page_to_html()) to the minijinja template:
        let page = json!({
            "page": {
                "root": "",
                "project_name": "Nanobot",
                "tables": table_map,
                "undo": get::get_undo_message(&state.config),
                "redo": get::get_redo_message(&state.config),
                "actions": get::get_action_map(&state.config).unwrap_or_default(),
                "repo": get::get_repo_details().unwrap_or_default(),
            },
            "title": "table",
            "table_name": table,
            "subtitle": format!(r#"<a href="/{}">Return to table</a>"#, table),
            "messages": [],
            "form_map": form_map,
        });
        let page_html = match get::page_to_html(&state.config, &view, &page) {
            Ok(p) => p,
            Err(e) => return Err(e.to_string().into()),
        };
        Ok(Html(page_html).into_response())
    } else {
        // In this case the request is to view the database contents represented by the request URL,
        // row by row.
        let url = {
            let url = sqlrest_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>();
            if !url.is_empty() {
                format!("{}?{}", table, url.join("&"))
            } else {
                table.to_string()
            }
        };
        tracing::info!("URL: {}", url);
        let select = parse(&url)?;
        tracing::info!("SELECT {:?}", select);
        match get::get_rows(&state.config, &select, &shape, &format).await {
            Ok(x) => match format {
                "tsv" => Ok(([("content-type", "text/tab-separated-values")], x).into_response()),
                "csv" => Ok(([("content-type", "text/csv")], x).into_response()),
                "text" => Ok(([("content-type", "text/plain")], x).into_response()),
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
}

async fn post_row(
    Path((table, row_number)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
    Query(query_params): Query<RequestParams>,
    Form(form_params): Form<RequestParams>,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!(
        "request row POST {:?} {:?}, Query Params: {:?}, Form Params: {:?}",
        table,
        row_number,
        query_params,
        form_params
    );
    row(
        Path((table, row_number)),
        &state,
        &query_params,
        &form_params,
        RequestType::POST,
    )
}

async fn get_row(
    Path((table, row_number)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
    Query(params): Query<RequestParams>,
) -> axum::response::Result<impl IntoResponse> {
    tracing::info!("request row GET {:?} {:?} {:?}", table, row_number, params);

    if params.contains_key("user.action") {
        let path = &format!("{table}/row/{row_number}");
        let result = action(&path, &state, &params)?;
        return Ok(result.into_response());
    }

    let row = row(
        Path((table, row_number)),
        &state,
        &params,
        &RequestParams::new(),
        RequestType::GET,
    )?;
    Ok(row.into_response())
}

fn row(
    Path((table, row_number)): Path<(String, String)>,
    state: &Arc<AppState>,
    query_params: &RequestParams,
    form_params: &RequestParams,
    request_type: RequestType,
) -> axum::response::Result<impl IntoResponse> {
    let row_number = match row_number.parse::<u32>() {
        Ok(r) => r,
        Err(e) => {
            let error = format!(
                "Unable to parse row_number '{}' due to error: {}",
                row_number, e
            );
            return Err((StatusCode::BAD_REQUEST, Html(error))
                .into_response()
                .into());
        }
    };

    render_row_from_database(
        &table,
        row_number,
        state,
        query_params,
        form_params,
        request_type,
    )
}

fn render_row_from_database(
    table: &str,
    row_number: u32,
    state: &Arc<AppState>,
    query_params: &RequestParams,
    form_params: &RequestParams,
    request_type: RequestType,
) -> axum::response::Result<impl IntoResponse> {
    let config = &state.config;
    let valve = config
        .valve
        .as_ref()
        .ok_or("Valve is not initialized.".to_string())?;
    let pool = &state
        .config
        .pool
        .as_ref()
        .ok_or("Pool is not initialized.".to_string())?;
    let view = match query_params.get("view") {
        Some(v) => v.to_string(),
        None => "form".to_string(),
    };

    // Handle requests related to typeahead, used for autocomplete in data forms:
    // TODO: There is an almost identical block of code in the table() route. We should refactor
    // so that it is in its own function.
    match query_params.get("format") {
        Some(format) if format == "json" => match query_params.get("column") {
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Html(
                        "For format=json, column is also required \
                     (e.g., /table/row/1?format=json&column=foo)"
                            .to_string(),
                    ),
                )
                    .into_response()
                    .into())
            }
            Some(column_name) => match block_on(valve.get_matching_values(
                &table,
                column_name,
                query_params.get("text").and_then(|t| Some(t.as_str())),
            )) {
                Ok(r) => return Ok(Json(r).into_response()),
                Err(e) => {
                    return Err((StatusCode::BAD_REQUEST, Html(format!("{:?}", e)))
                        .into_response()
                        .into())
                }
            },
        },
        _ => (),
    };

    // Handle POST request to validate or update the row in the table:
    let mut messages = HashMap::new();
    let mut form_map = None;
    if request_type == RequestType::POST {
        let mut new_row = SerdeMap::new();
        // Use the list of columns for the table from the db to look up their values in the form:
        for column in &get_columns(table, valve)? {
            if !vec!["row_number", "row_order"].contains(&column.as_str()) {
                let value = match form_params.get(column) {
                    Some(v) => v.to_string(),
                    None => {
                        let other_column = format!("{}_other", column);
                        form_params
                            .get(&other_column)
                            .unwrap_or(&String::from(""))
                            .to_string()
                    }
                };
                new_row.insert(column.to_string(), value.into());
            }
        }

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
            match get_row_as_form_map(config, table, &validated_row) {
                Ok(f) => form_map = Some(f),
                Err(e) => {
                    tracing::debug!("Rendering error 3 {}", e);
                    form_map = None
                }
            };
        } else if action == "submit" {
            if let Err(e) = update_table_row(table, &new_row, &row_number, state) {
                return Err(e.to_string().into());
            }

            messages = get_messages(&new_row)?;
            if let Some(error_messages) = messages.get_mut("error") {
                let extra_message = format!("Row updated with {} errors", error_messages.len());
                match messages.get_mut("debug") {
                    Some(debug_messages) => debug_messages.push(extra_message),
                    None => {
                        messages.insert("debug".to_string(), vec![extra_message]);
                    }
                };
            } else {
                messages.insert(
                    "success".to_string(),
                    vec!["Row successfully updated!".to_string()],
                );
            }
        } else if action == "delete" {
            if let Err(e) = delete_table_row(table, &row_number, state) {
                return Err(e.to_string().into());
            }
            tracing::debug!("DELETED ROW {table} {row_number}, redirecting to ../../{table}");
            return Ok(Redirect::to(&format!("../../{table}").to_string()).into_response());
        }
    }

    // Handle a request to display a form for editing and validiating the given row:
    if view != "" {
        if let None = form_map {
            if vec!["message", "history"].contains(&table) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Html(format!("Editing the {} table is not possible", table)),
                )
                    .into_response()
                    .into());
            }
            let mut select = Select::new(format!("{}_view", table));
            select.filter(vec![Filter::new(
                "row_number",
                "eq",
                json!(format!("{}", row_number)),
            )?]);
            let mut rows = select.fetch_rows_as_json(pool, &HashMap::new())?;
            if rows.len() == 0 {
                let error = format!("No such row '{row_number}' for table '{table}'");
                return Err((StatusCode::NOT_FOUND, Html(error)).into_response().into());
            }
            let mut row = &mut rows[0];
            let metafied_row = metafy_row(&mut row)?;
            match get_row_as_form_map(config, table, &metafied_row) {
                Ok(f) => form_map = Some(f),
                Err(e) => {
                    tracing::debug!("Rendering error 4 {}", e);
                    form_map = None
                }
            };
        }
    }

    let form_map = match form_map {
        Some(f) => f,
        None => {
            let error = "Something went wrong - unable to render form".to_string();
            return Err((StatusCode::BAD_REQUEST, Html(error))
                .into_response()
                .into());
        }
    };

    // Used to display a drop-down or menu containing all of the tables:
    let table_map = {
        let mut table_map = SerdeMap::new();
        for table in &valve.config.table_order {
            if table == "history" {
                continue;
            }
            table_map.insert(table.to_string(), json!(table.clone()));
        }
        json!(table_map)
    };

    // Fill in the page JSON which contains all of the parameters that we will be passing to our
    // minijinja template (through page_to_html()):
    let page = json!({
        "page": {
            "root": "../../",
            "project_name": "Nanobot",
            "tables": table_map,
            "undo": get::get_undo_message(&state.config),
            "redo": get::get_redo_message(&state.config),
            "actions": get::get_action_map(&state.config).unwrap_or_default(),
            "repo": get::get_repo_details().unwrap_or_default(),
        },
        "title": "table",
        "table_name": table,
        "row_number": row_number,
        "offset": row_number - 1,
        "subtitle": format!(r#"<a href="/{}/row/{}">Return to row</a>"#, table, row_number),
        "messages": messages,
        "form_map": form_map,
    });
    let page_html = match get::page_to_html(&state.config, &view, &page) {
        Ok(p) => p,
        Err(e) => return Err(e.to_string().into()),
    };
    Ok(Html(page_html).into_response())
}

fn matches_nulltype(table: &str, column: &str, value: &str, valve: &Valve) -> Result<bool, String> {
    let column_config = get_column_config(table, column, valve)?;
    if column_config.nulltype == "" {
        // If there is no nulltype for this column, check that the value is not an empty string.
        return Ok(value != "");
    }
    let nulltype = column_config.nulltype;

    let datatype_conditions = &valve.datatype_conditions;
    match datatype_conditions.get(&nulltype) {
        Some(datatype_condition) => {
            let compiled_cond = &datatype_condition.compiled;
            return Ok(compiled_cond(value));
        }
        // If there is no datatype condition corresponding to the nulltype (e.g., if nultype is
        // "text"), then all values will be accepted:
        None => return Ok(true),
    };
}

fn get_messages(row: &SerdeMap) -> Result<HashMap<String, Vec<String>>, String> {
    let mut messages = HashMap::new();
    for (header, details) in row {
        if vec!["row_number", "row_order"].contains(&header.as_str()) {
            continue;
        }
        if let Some(SerdeValue::Array(row_messages)) = details.get("messages") {
            for msg in row_messages {
                match msg.get("level") {
                    Some(level) if level == "error" => {
                        if !messages.contains_key("error") {
                            messages.insert("error".to_string(), vec![]);
                        }
                        let error_list = match messages.get_mut("error") {
                            Some(e) => e,
                            None => return Err("No 'error' in messages".to_string()),
                        };
                        let error_msg = match msg.get("message").and_then(|m| m.as_str()) {
                            Some(s) => s,
                            None => return Err(format!("No str called 'message' in {}", msg)),
                        };
                        error_list.push(error_msg.to_string());
                    }
                    Some(level) if level == "debug" => {
                        if !messages.contains_key("debug") {
                            messages.insert("debug".to_string(), vec![]);
                        }
                        let debug_list = match messages.get_mut("debug") {
                            Some(e) => e,
                            None => return Err("No 'debug' in messages".to_string()),
                        };
                        let debug_msg = match msg.get("message").and_then(|m| m.as_str()) {
                            Some(s) => s,
                            None => return Err(format!("No str called 'message' in {}", msg)),
                        };
                        debug_list.push(debug_msg.to_string());
                    }
                    Some(level) if level == "info" => {
                        if !messages.contains_key("info") {
                            messages.insert("info".to_string(), vec![]);
                        }
                        let info_list = match messages.get_mut("info") {
                            Some(e) => e,
                            None => return Err("No 'info' in messages".to_string()),
                        };
                        let info_msg = match msg.get("message").and_then(|m| m.as_str()) {
                            Some(s) => s,
                            None => return Err(format!("No str called 'message' in {}", msg)),
                        };
                        info_list.push(info_msg.to_string());
                    }
                    Some(level) => tracing::debug!("Unrecognized level '{}' in {}", level, msg),
                    None => tracing::debug!("Message: {} has no 'level'. Ignoring it.", msg),
                };
            }
        }
    }
    Ok(messages)
}

fn get_columns(table: &str, valve: &Valve) -> Result<Vec<String>, String> {
    match valve
        .config
        .table
        .get(table)
        .and_then(|t| Some(t.column_order.to_vec()))
    {
        None => Err(format!(
            "Unable to retrieve columns of '{}' from valve configuration.",
            table
        )),
        Some(v) => Ok(v),
    }
}

fn get_column_config(
    table: &str,
    column: &str,
    valve: &Valve,
) -> Result<ValveColumnConfig, String> {
    match valve
        .config
        .table
        .get(table)
        .and_then(|t| t.column.get(column))
    {
        Some(c) => Ok(c.clone()),
        None => Err(format!(
            "Unable to retrieve column config for '{}.{}' from Valve configuration",
            table, column
        )),
    }
}

fn get_html_type_and_values(
    config: &Config,
    datatype: &str,
    values: &Option<Vec<String>>,
) -> Result<(Option<String>, Option<Vec<String>>), String> {
    let valve = config.valve.as_ref().unwrap();
    let dt_config = match valve.config.datatype.get(datatype) {
        Some(o) => o,
        None => {
            return Err(format!(
                "Unable to retrieve datatype config for '{}'",
                datatype
            ))
        }
    };

    let mut new_values = vec![];
    match values {
        None => match valve.datatype_conditions.get(datatype) {
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
            _ => (),
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

    let rows = &config.datatype;
    for row in rows.iter() {
        let name = row.get("datatype").unwrap();
        if name == datatype {
            if let Some(html_type) = row.get("html_type") {
                if let Some(html_type) = html_type.as_str() {
                    return Ok((Some(html_type.into()), new_values));
                }
            }
            break;
        }
    }

    if dt_config.parent != "" {
        return get_html_type_and_values(config, &dt_config.parent, &new_values);
    }

    Ok((None, None))
}

fn insert_table_row(
    table_name: &str,
    row_data: &SerdeMap,
    state: &Arc<AppState>,
) -> Result<u32, String> {
    let (row_num, _) = block_on(
        state
            .config
            .valve
            .as_ref()
            .ok_or("Valve is not initialized.".to_string())?
            .insert_row(&table_name, &row_data),
    )
    .unwrap();
    Ok(row_num)
}

fn update_table_row(
    table_name: &str,
    row_data: &SerdeMap,
    row_number: &u32,
    state: &Arc<AppState>,
) -> Result<(), String> {
    block_on(
        state
            .config
            .valve
            .as_ref()
            .ok_or("Valve is not initialized.".to_string())?
            .update_row(&table_name, row_number, &row_data),
    )
    .unwrap();
    Ok(())
}

fn delete_table_row(
    table_name: &str,
    row_number: &u32,
    state: &Arc<AppState>,
) -> Result<(), String> {
    block_on(
        state
            .config
            .valve
            .as_ref()
            .ok_or("Valve is not initialized.".to_string())?
            .delete_row(&table_name, row_number),
    )
    .unwrap();
    Ok(())
}

fn validate_table_row(
    table_name: &str,
    row_data: &SerdeMap,
    row_number: &Option<u32>,
    state: &Arc<AppState>,
) -> Result<SerdeMap, String> {
    let validated_row = {
        let mut result_row = SerdeMap::new();
        for (column, value) in row_data.iter() {
            result_row.insert(column.to_string(), value.clone());
        }
        match block_on(
            state
                .config
                .valve
                .as_ref()
                .ok_or("Valve is not initialized.".to_string())?
                .validate_row(table_name, &result_row, *row_number),
        ) {
            Ok(r) => r,
            Err(e) => return Err(format!("{:?}", e)),
        }
    };
    Ok(validated_row
        .contents_to_rich_json()
        .map_err(|e| format!("{:?}", e))?)
}

fn stringify_messages(messages: &Vec<SerdeValue>) -> Result<String, String> {
    let mut msg_parts = vec![];
    for m in messages {
        match m.as_object() {
            None => return Err(format!("{:?} is not an object.", m)),
            Some(message) => {
                let level = message.get("level").unwrap_or(&serde_json::Value::Null);
                if level == "update" {
                    continue;
                }
                match message.get("message") {
                    None => return Err(format!("No 'message' in {:?}", message)),
                    Some(message) => {
                        match message.as_str() {
                            Some(message) => msg_parts.push(message.to_string()),
                            None => return Err(format!("{} is not a str", message)),
                        };
                    }
                }
            }
        };
    }
    Ok(msg_parts.join("<br>"))
}

fn metafy_row(row: &mut SerdeMap) -> Result<SerdeMap, String> {
    let mut metafied_row = SerdeMap::new();
    let mut messages = match row.get_mut("message") {
        Some(SerdeValue::Array(m)) => m.clone(),
        Some(SerdeValue::Null) => vec![],
        _ => return Err(format!("No array called 'messages' in row: {:?}", row).into()),
    };
    for (column, value) in row {
        if vec!["row_number", "row_order", "message"].contains(&column.as_str()) {
            continue;
        }
        let mut metafied_cell = SerdeMap::new();
        metafied_cell.insert("value".to_string(), value.clone());
        let mut valid = true;
        let metafied_messages = {
            let mut metafied_messages = vec![];
            for m in &mut messages {
                if let Some(SerdeValue::String(mcol)) = m.get("column") {
                    if mcol == column {
                        let m = match m.as_object_mut() {
                            Some(m) => m,
                            None => return Err(format!("{} is not an object", m)),
                        };
                        if let Some(level) = m.get("level") {
                            if level != "update" {
                                valid = false;
                            }
                        }
                        m.remove("column");
                        metafied_messages.push(m.clone());
                        // Overwrite the value in the metafied_cell:
                        metafied_cell.insert(
                            "value".to_string(),
                            match m.get("value") {
                                Some(v) => v.clone(),
                                None => return Err(format!("No 'value' in {:?}", m)),
                            },
                        );
                        m.remove("value");
                    }
                }
            }
            metafied_messages
        };
        metafied_cell.insert("messages".to_string(), json!(metafied_messages));
        metafied_cell.insert("valid".to_string(), valid.into());
        metafied_row.insert(column.to_string(), json!(metafied_cell));
    }
    Ok(metafied_row)
}

fn get_row_as_form_map(
    config: &Config,
    table_name: &str,
    row_data: &SerdeMap,
) -> Result<SerdeMap, String> {
    let valve = config.valve.as_ref().unwrap();
    let mut result = SerdeMap::new();
    let mut row_valid = None;
    let mut form_row_id = 0;
    for (cell_header, cell_value) in row_data.iter() {
        if vec!["row_number", "row_order", "history"].contains(&cell_header.as_str()) {
            continue;
        }
        let (valid, value, messages);
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

        let message = stringify_messages(&messages)?;
        let column_config = get_column_config(table_name, cell_header, valve)?;
        let description = column_config.description;
        let label = if column_config.label.trim() == "" {
            cell_header.to_string()
        } else {
            column_config.label.to_string()
        };
        let datatype = column_config.datatype;
        let structure = column_config.structure.split('(').collect::<Vec<_>>()[0];

        let mut html_type;
        let mut allowed_values = None;
        if vec!["from", "in", "tree", "under"].contains(&structure) {
            html_type = Some("search".into());
        } else {
            (html_type, allowed_values) = get_html_type_and_values(config, &datatype, &None)?;
        }

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

        let hiccup_form_row = get_hiccup_form_row(
            cell_header,
            &None,
            &allowed_values,
            &Some(description),
            &Some(label),
            &html_type,
            &Some(message),
            &Some(readonly),
            &Some(valid),
            &Some(value),
            form_row_id,
        )?;
        let html = hiccup::render(&json!(hiccup_form_row))?;
        result.insert(cell_header.into(), json!(html));
        form_row_id += 1;
    }

    // let submit_cls = match row_valid {
    //     Some(flag) => {
    //         if flag {
    //             "success"
    //         } else {
    //             "danger"
    //         }
    //     }
    //     None => "secondary", // Row has not yet been validated - display gray button.
    // };

    Ok(result)
}

fn get_hiccup_form_row(
    header: &str,
    allow_delete: &Option<bool>,
    allowed_values: &Option<Vec<String>>,
    description: &Option<String>,
    display_header: &Option<String>,
    html_type: &Option<String>,
    message: &Option<String>,
    readonly: &Option<bool>,
    valid: &Option<bool>,
    value: &Option<SerdeValue>,
    form_row_id: usize,
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
        return Err(format!(
            "A list of allowed values is required for HTML type '{}'",
            html_type
        ));
    }

    // Create the header level for this form row:
    let mut header_col = vec![
        json!("div"),
        json!({"class": "col-md-4 form-label", "id": form_row_id}),
    ];
    if allow_delete {
        header_col.push(json!([
            "a",
            { "href": format!("javascript:del({})", form_row_id) },
            ["i", {"class": "bi-x-circle", "style": "font-size: 16px; color: #dc3545;"}],
            "&nbsp",
        ]));
    }

    match display_header {
        Some(d) => header_col.push(json!(["p", {"class": "header"}, d])),
        None => header_col.push(json!(["p", {"class": "header"}, header])),
    };

    if let Some(description) = description {
        if description != "" {
            header_col.push(json!(["p", {"class": "description"}, description]));
        }
    }

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

    let mut value_col = vec![json!("div"), json!({"class": "col-md-8 form-group"})];

    if html_type == "input" {
        classes.insert(0, "form-control");
        input_attrs.insert("class".to_string(), json!(classes.join(" ")));
        match value {
            Some(SerdeValue::String(value)) => {
                let mut empty = String::new();
                let value = encode_text_to_string(value, &mut empty);
                input_attrs.insert("value".to_string(), json!(value));
            }
            Some(SerdeValue::Number(value)) => {
                input_attrs.insert("value".to_string(), json!(value));
            }
            Some(SerdeValue::Bool(value)) => {
                input_attrs.insert("value".to_string(), json!(value));
            }
            _ => (),
        };
        value_col.push(json!([html_type, input_attrs]));
    } else if html_type == "textarea" {
        classes.insert(0, "form-control");
        input_attrs.insert("class".to_string(), json!(classes.join(" ")));
        let mut element = vec![json!(html_type), json!(input_attrs)];
        match value {
            Some(SerdeValue::String(value)) => {
                let mut empty = String::new();
                let value = encode_text_to_string(value, &mut empty);
                element.push(json!(value));
            }
            Some(SerdeValue::Number(value)) => {
                element.push(json!(value));
            }
            Some(SerdeValue::Bool(value)) => {
                element.push(json!(value));
            }
            _ => (),
        };
        value_col.push(json!(element));
    } else if html_type == "select" {
        // TODO: This html type will need to be re-implemented (later).
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
                            "option",
                            {"value": av_safe, "selected": true},
                            av_safe,
                        ]));
                    }
                    _ => {
                        select_element.push(json!([
                            "option",
                            { "value": av_safe },
                            av_safe,
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
    } else if vec!["text", "number", "search"].contains(&html_type) {
        // TODO: This html type will need to be re-implemented (later).
        // TODO: Support a range restriction for 'number'
        classes.insert(0, "form-control");
        input_attrs.insert("type".to_string(), json!(html_type));
        if html_type == "search" {
            classes.append(&mut vec!["search", "typeahead"]);
            input_attrs.insert(
                "id".to_string(),
                json!(format!("{}-typeahead-form", header)),
            );
        }
        input_attrs.insert("class".to_string(), json!(classes.join(" ")));
        match value {
            Some(SerdeValue::String(value)) => {
                let mut empty = String::new();
                let value = encode_text_to_string(value, &mut empty);
                input_attrs.insert("value".to_string(), json!(value));
            }
            Some(SerdeValue::Number(value)) => {
                input_attrs.insert("value".to_string(), json!(value));
            }
            Some(SerdeValue::Bool(value)) => {
                input_attrs.insert("value".to_string(), json!(value));
            }
            _ => (),
        };
        value_col.push(json!(["input", input_attrs]));
    } else if html_type == "radio" {
        // TODO: This html type will need to be re-implemented (later).
        classes.insert(0, "form-check-input");
        input_attrs.insert("type".to_string(), json!(html_type));
        input_attrs.insert("class".to_string(), json!(classes.join(" ")));
        if let Some(allowed_values) = allowed_values {
            for av in allowed_values {
                let mut empty = String::new();
                let av_safe = encode_text_to_string(av, &mut empty);
                let mut attrs_copy = input_attrs.clone();
                attrs_copy.insert("value".to_string(), json!(av_safe));
                // TODO: Do we need to do something in particular in the case where value is None?
                if let Some(value) = value {
                    if value == av {
                        attrs_copy.insert("checked".to_string(), json!(true));
                    }
                }
                value_col.push(json!([
                    "div",
                    ["input", attrs_copy],
                    ["label", {"class": "form-check-label", "for": av_safe}, av_safe,],
                ]));
            }
        }

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

        if let Some(allowed_values) = allowed_values {
            match value {
                Some(SerdeValue::String(value)) => {
                    if !allowed_values.contains(&value) {
                        attrs_copy.insert("checked".to_string(), json!(true));
                        let mut empty = String::new();
                        let value = encode_text_to_string(value, &mut empty);
                        input_attrs.insert("value".to_string(), json!(value));
                    }
                }
                Some(SerdeValue::Number(value)) => {
                    if !allowed_values.contains(&value.to_string()) {
                        attrs_copy.insert("checked".to_string(), json!(true));
                        input_attrs.insert("value".to_string(), json!(value));
                    }
                }
                Some(SerdeValue::Bool(value)) => {
                    if !allowed_values.contains(&value.to_string()) {
                        attrs_copy.insert("checked".to_string(), json!(true));
                        input_attrs.insert("value".to_string(), json!(value));
                    }
                }
                _ => (),
            };
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
            e.push(json!([
                json!("div"),
                json!({ "class": validation_cls }),
                json!(message),
            ]));
        }
        value_col.push(json!(e));
    } else {
        return Err(format!(
            "'{}' form field is not supported for column '{}'",
            html_type, header
        ));
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

    Ok(vec![
        json!("div"),
        json!({"class": "row py-1"}),
        json!(header_col),
        json!(value_col),
    ])
}
