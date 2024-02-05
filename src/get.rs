// TODO: Have a general look through all of this code (and in the other modules) and see if
// we can use the valve config (which is now available) instead of running db requests. But do
// this later.

use crate::config::Config;
use crate::sql::{
    get_count_from_pool, get_message_counts_from_pool, get_table_from_pool, get_total_from_pool, 
    LIMIT_MAX,
};
use chrono::prelude::{DateTime, Utc};
use csv::WriterBuilder;
use enquote::unquote;
use futures::executor::block_on;
use git2::Repository;
use minijinja::{Environment, Source};
use ontodev_sqlrest::{Direction, OrderByColumn, Select};
use ontodev_valve as valve;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_string_pretty, Map, Value};
use sqlx::any::AnyRow;
use sqlx::Row;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;
use tabwriter::TabWriter;
use urlencoding::decode;

pub type SerdeMap = serde_json::Map<String, serde_json::Value>;

#[derive(Debug)]
pub struct GetError {
    details: String,
}

impl GetError {
    fn new(msg: String) -> GetError {
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ValveMessage {
    column: String,
    value: String,
    rule: String,
    level: String,
    message: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ValveChange {
    column: String,
    level: String,
    old_value: String,
    value: String,
    message: String,
}

pub async fn get_table(
    config: &Config,
    table: &str,
    shape: &str,
    format: &str,
) -> Result<String, GetError> {
    let table = unquote(table).unwrap_or(table.to_string());
    let mut select = Select::new(format!("\"{}\"", table));
    select.limit(usize::from(config.results_per_page));
    get_rows(config, &select, shape, format).await
}

pub async fn get_rows(
    config: &Config,
    base_select: &Select,
    shape: &str,
    format: &str,
) -> Result<String, GetError> {
    // Get all the tables
    let table_map = match config
        .valve
        .as_ref()
        .and_then(|v| v.config.get("table"))
        .and_then(|t| t.as_object())
    {
        Some(table_map) => table_map,
        None => {
            return Err(GetError::new(format!(
                "No object named 'table' in valve config"
            )))
        }
    };

    let unquoted_table = unquote(&base_select.table).unwrap_or(base_select.table.to_string());
    if !table_map.contains_key(&unquoted_table) {
        return Err(GetError::new(format!(
            "Invalid table '{}'",
            &base_select.table
        )));
    }

    // Get the columns for the selected table
    let column_config = match config
        .valve
        .as_ref()
        .and_then(|v| v.config.get("table"))
        .and_then(|t| t.as_object())
        .and_then(|t| t.get(&unquoted_table))
        .and_then(|t| t.as_object())
        .and_then(|t| t.get("column"))
        .and_then(|c| c.as_object())
    {
        None => {
            return Err(GetError::new(format!(
                "Unable to retrieve columns of '{}' from valve configuration.",
                unquoted_table
            )))
        }
        Some(v) => v,
    };

    let mut columns: Vec<String> = vec![];
    let mut column_rows = vec![];
    for (column, row) in column_config {
        let unquoted_column = unquote(&column).unwrap_or(column.to_string());
        columns.push(format!("\"{}\"", unquoted_column));
        let row = match row.as_object() {
            Some(row) => row.clone(),
            None => return Err(GetError::new(format!("{:?} is not an object", row))),
        };
        column_rows.push(row);
    }

    let mut select = Select::clone(&base_select);
    select.select(columns);
    match select.limit {
        Some(l) if l > LIMIT_MAX => select.limit(LIMIT_MAX),
        Some(l) if l > 0 => select.limit(l),
        _ => select.limit(usize::from(config.results_per_page)),
    };

    let pool = match config.pool.as_ref() {
        Some(p) => p,
        _ => {
            return Err(GetError::new(format!(
                "Could not connect to database using pool {:?}",
                config.pool
            )))
        }
    };

    match shape {
        "value_rows" => {
            if unquoted_table != "message" {
                // use the *_view table
                select.table(format!("\"{unquoted_table}_view\""));
            }
            tracing::debug!("VALUE SELECT {select:?}");
            let value_rows = match get_table_from_pool(&pool, &select).await {
                Ok(value_rows) => value_rows,
                Err(e) => return Err(GetError::new(e.to_string())),
            };
            match format {
                "tsv" => value_rows_to_tsv(&value_rows),
                "csv" => value_rows_to_csv(&value_rows),
                "text" => value_rows_to_text(&value_rows),
                "json" => Ok(json!(value_rows).to_string()),
                "pretty.json" => match to_string_pretty(&json!(value_rows)) {
                    Ok(pretty_json) => Ok(pretty_json),
                    Err(e) => return Err(GetError::new(e.to_string())),
                },
                &_ => Err(GetError::new(format!(
                    "Shape '{}' does not support format '{}'",
                    shape, format
                ))),
            }
        }
        "page" => {
            let page = match get_page(&config, &select, &table_map, &column_rows).await {
                Ok(page) => page,
                Err(e) => return Err(GetError::new(e.to_string())),
            };
            match format {
                "json" => Ok(page.to_string()),
                "pretty.json" => match to_string_pretty(&page) {
                    Ok(pretty_json) => Ok(pretty_json),
                    Err(e) => return Err(GetError::new(e.to_string())),
                },
                "html" => page_to_html(&config, "table", &page),
                &_ => Err(GetError::new(format!(
                    "Shape '{}' does not support format '{}'",
                    shape, format
                ))),
            }
        }
        _ => Err(GetError::new(format!("Invalid shape '{}'", shape))),
    }
}

async fn get_page(
    config: &Config,
    select: &Select,
    table_map: &Map<String, Value>,
    column_rows: &Vec<Map<String, Value>>,
) -> Result<Value, GetError> {
    let pool = &config.pool.as_ref().unwrap();
    let filter_messages = {
        let m = select
            .select
            .iter()
            .filter(|s| {
                let s_column = unquote(&s.expression).unwrap_or(s.expression.to_string());
                s_column == "message"
            })
            .collect::<Vec<_>>();
        !m.is_empty()
    };

    // Annotate columns with filters and sorting
    let mut column_map = Map::new();
    for row in column_rows.iter() {
        let mut r = Map::new();
        let mut key = String::from("");
        for (k, v) in row.iter() {
            if k == "column" {
                key = match v.as_str() {
                    Some(key) => key.to_string(),
                    None => return Err(GetError::new(format!("Could not convert '{}' to str", v))),
                };
            } else {
                r.insert(k.to_string(), v.clone());
            }
        }

        let sql_type = valve::get_sql_type_from_global_config(
            &config.valve.as_ref().unwrap().config,
            &unquote(&select.table).unwrap(),
            &key,
            &config.pool.as_ref().unwrap(),
        )
        .unwrap_or_default();
        r.insert("sql_type".into(), json!(sql_type));
        let numeric_types = ["integer", "numeric", "real", "decimal"];
        r.insert(
            "numeric".into(),
            json!(numeric_types.contains(&sql_type.to_lowercase().as_str())),
        );
        let mut filter_others = vec![];
        for filter in &select.filter {
            if filter.lhs.replace("\"", "") == key {
                r.insert(
                    "filtered_operator".into(),
                    json!(filter.operator.to_string()),
                );
                r.insert(
                    "filtered_constraint".into(),
                    match filter.rhs.clone() {
                        serde_json::Value::String(s) => json!(s
                            .clone()
                            .replace("\"", "")
                            .replace("\u{0027}", "")
                            .replace("%", "*")),
                        _ => json!(filter.rhs),
                    },
                );
            } else {
                filter_others.push(filter.clone());
            }
        }
        for order_by in &select.order_by {
            if order_by.column.replace("\"", "") == key {
                r.insert("sorted".to_string(), json!(order_by.direction.to_url()));
                break;
            }
        }

        let mut sorted = select.clone();
        let empty: Vec<String> = Vec::new();
        sorted.select(empty);

        sorted.order_by(vec![&key]);
        let href = match sorted.to_url() {
            Ok(url) => url,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        let href = match decode(&href) {
            Ok(href) => href,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        r.insert("sort_ascending".into(), json!(href));

        sorted.explicit_order_by(vec![&OrderByColumn::new(&key, &Direction::Descending)]);
        let href = match sorted.to_url() {
            Ok(url) => url,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        let href = match decode(&href) {
            Ok(href) => href,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        r.insert("sort_descending".into(), json!(href));

        let empty: Vec<String> = Vec::new();
        sorted.order_by(empty);
        let href = match sorted.to_url() {
            Ok(url) => url,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        let href = match decode(&href) {
            Ok(href) => href,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        r.insert("sort_none".into(), json!(href));

        let mut sorted = select.clone();
        let empty: Vec<String> = Vec::new();
        sorted.select(empty);

        if r.contains_key(&"sorted".to_string()) {
            let empty: Vec<String> = Vec::new();
            sorted.order_by(empty);
        }
        sorted.filter(filter_others);
        let href = match sorted.to_url() {
            Ok(url) => url,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        let href = match decode(&href) {
            Ok(href) => href,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        r.insert("reset".into(), json!(href));

        // TODO: Hide

        column_map.insert(key, Value::Object(r));
    }

    // We will need the table name without quotes for lookup purposes:
    let unquoted_table = unquote(&select.table).unwrap_or(select.table.to_string());
    // For calculating processing time:
    let start = std::time::Instant::now();

    // If the table is anything other than the message table, query its corresponding view instead
    // of the table itself. The view includes conflict rows and the message column:
    let db_object;
    if unquoted_table == "message" {
        db_object = unquoted_table.to_string();
    } else {
        db_object = format!("{}_view", unquoted_table);
    }
    let mut view_select = Select {
        table: db_object,
        ..select.clone()
    };
    let curr_cols = view_select.select.to_vec();
    // Explicitly include the row_number / message_id column:
    if unquoted_table == "message" {
        view_select.select(vec!["message_id"]);
    } else {
        view_select.select(vec!["row_number"]);
    }
    for col in &curr_cols {
        view_select.add_explicit_select(col);
    }
    // If this isn't the message table, explicitly include the message and history columns from the table's view:
    if unquoted_table != "message" {
        view_select.add_select("message");
        view_select.add_select("history");
    }

    // Only apply the limit to the view query if we're filtering for rows with messages:
    if filter_messages {
        if let Some(limit) = select.limit {
            view_select.limit(limit);
        }
    }

    // Use the view to select the data
    tracing::debug!("VIEW SELECT {view_select:?}");
    let value_rows = match get_table_from_pool(&pool, &view_select).await {
        Ok(value_rows) => value_rows,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    // Get the number of messages of each type:
    let message_counts = match get_message_counts_from_pool(&pool, &unquoted_table).await {
        Ok(message_counts) => message_counts,
        Err(e) => return Err(GetError::new(e.to_string())),
    };

    // convert value_rows to cell_rows
    let table_type = config
        .valve
        .as_ref()
        .and_then(|v| v.config.get("table"))
        .and_then(|v| v.as_object())
        .and_then(|o| o.get(&unquoted_table))
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let cell_rows: Vec<Map<String, Value>> = value_rows
        .iter()
        .map(|r| decorate_row(&unquoted_table, &table_type, &column_map, r))
        .collect();

    let mut counts = Map::new();
    let count = {
        if unquoted_table != "message" && filter_messages {
            match message_counts.get("message_row").and_then(|m| m.as_u64()) {
                Some(m) => m as usize,
                None => {
                    return Err(GetError::new(format!(
                        "No 'nessage_row' in {:?}",
                        message_counts
                    )))
                }
            }
        } else {
            match get_count_from_pool(&pool, &select).await {
                Ok(count) => count,
                Err(e) => return Err(GetError::new(e.to_string())),
            }
        }
    };
    counts.insert("count".to_string(), json!(count));

    let total = match get_total_from_pool(&pool, &unquoted_table).await {
        Ok(total) => total,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    counts.insert("total".to_string(), json!(total));
    for (k, v) in message_counts {
        counts.insert(k, v.into());
    }

    let end = select.offset.unwrap_or(0) + cell_rows.len();

    let mut this_table = match table_map.get(&unquoted_table).and_then(|t| t.as_object()) {
        Some(t) => t.clone(),
        None => {
            return Err(GetError::new(format!(
                "No '{}' in {:?}",
                unquoted_table, table_map
            )))
        }
    };
    this_table.insert("table".to_string(), json!(unquoted_table.clone()));
    this_table.insert("href".to_string(), json!(unquoted_table.clone()));
    this_table.insert("start".to_string(), json!(select.offset.unwrap_or(0) + 1));
    this_table.insert("end".to_string(), json!(end));
    this_table.insert("counts".to_string(), json!(counts));

    let mut formats = Map::new();

    let mut select_format = Select::clone(select);
    let empty: Vec<String> = Vec::new();
    select_format.select(empty);

    select_format.table(format!("\"{}.tsv\"", unquoted_table));
    let href = match select_format.to_url() {
        Ok(url) => url,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let href = match decode(&href) {
        Ok(href) => href,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    formats.insert("TSV".to_string(), json!(href));

    select_format.table(format!("\"{}.csv\"", unquoted_table));
    let href = match select_format.to_url() {
        Ok(url) => url,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let href = match decode(&href) {
        Ok(href) => href,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    formats.insert("CSV".to_string(), json!(href));

    select_format.table(format!("\"{}.txt\"", unquoted_table));
    let href = match select_format.to_url() {
        Ok(url) => url,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let href = match decode(&href) {
        Ok(href) => href,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    formats.insert("Plain Text".to_string(), json!(href));

    select_format.table(format!("\"{}.json\"", unquoted_table));
    let href = match select_format.to_url() {
        Ok(url) => url,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let href = match decode(&href) {
        Ok(href) => {
            if href.contains("?") {
                format!("{}&shape=value_rows", href)
            } else {
                format!("{}?shape=value_rows", href)
            }
        }
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    formats.insert("JSON (raw)".to_string(), json!(href));

    select_format.table(format!("\"{}.pretty.json\"", unquoted_table));
    let href = match select_format.to_url() {
        Ok(url) => url,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let href = match decode(&href) {
        Ok(href) => {
            if href.contains("?") {
                format!("{}&shape=value_rows", href)
            } else {
                format!("{}?shape=value_rows", href)
            }
        }
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    formats.insert("JSON (raw, pretty)".to_string(), json!(href));

    select_format.table(format!("\"{}.json\"", unquoted_table));
    let href = match select_format.to_url() {
        Ok(url) => url,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let href = match decode(&href) {
        Ok(href) => href,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    formats.insert("JSON (page)".to_string(), json!(href));

    select_format.table(format!("\"{}.pretty.json\"", unquoted_table));
    let href = match select_format.to_url() {
        Ok(url) => url,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let href = match decode(&href) {
        Ok(href) => href,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    formats.insert("JSON (page, pretty)".to_string(), json!(href));

    this_table.insert("formats".to_string(), json!(formats));

    // Pagination
    let mut select_offset = Select::clone(select);
    let empty: Vec<String> = Vec::new();
    select_offset.select(empty);
    match select.offset {
        Some(offset) if offset > 0 => {
            let href = match select_offset.offset(0).to_url() {
                Ok(url) => url,
                Err(e) => return Err(GetError::new(e.to_string())),
            };
            let href = match decode(&href) {
                Ok(href) => href,
                Err(e) => return Err(GetError::new(e.to_string())),
            };
            this_table.insert("first".to_string(), json!(href));
            if offset > select.limit.unwrap_or(0) {
                let href = match select_offset
                    .offset(offset - select.limit.unwrap_or(0))
                    .to_url()
                {
                    Ok(url) => url,
                    Err(e) => return Err(GetError::new(e.to_string())),
                };
                let href = match decode(&href) {
                    Ok(href) => href,
                    Err(e) => return Err(GetError::new(e.to_string())),
                };
                this_table.insert("previous".to_string(), json!(href));
            } else {
                this_table.insert("previous".to_string(), json!(href));
            }
        }
        _ => (),
    };
    if end < count {
        let href = match select_offset
            .offset(select.offset.unwrap_or(0) + select.limit.unwrap_or(0))
            .to_url()
        {
            Ok(url) => url,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        let href = match decode(&href) {
            Ok(href) => href,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        this_table.insert("next".to_string(), json!(href));
        let remainder = count % select.limit.unwrap_or(0);
        let last = if remainder == 0 {
            count - select.limit.unwrap_or(0)
        } else {
            count - (count % select.limit.unwrap_or(0))
        };
        let href = match select_offset.offset(last).to_url() {
            Ok(url) => url,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        let href = match decode(&href) {
            Ok(href) => href,
            Err(e) => return Err(GetError::new(e.to_string())),
        };
        this_table.insert("last".to_string(), json!(href));
    }

    let mut tables = Map::new();
    for key in table_map.keys() {
        if key == "history" {
            continue;
        }
        tables.insert(key.clone(), Value::String(key.clone()));
    }

    let mut select2 = select.clone();
    let empty: Vec<String> = Vec::new();
    select2.select(empty);

    let elapsed = start.elapsed().as_millis() as usize;
    let result: Value = json!({
        "page": {
            "project_name": "Nanobot",
            "tables": tables,
            "title": unquoted_table,
            "url": select2.to_url().unwrap_or_default(),
            "select": select,
            "select_params": select2.to_params().unwrap_or_default(),
            "elapsed": elapsed,
            "undo": get_undo_message(&config),
            "redo": get_redo_message(&config),
            "actions": get_action_map(&config).unwrap_or_default(),
            "repo": get_repo_details().unwrap_or_default(),
        },
        "table": this_table,
        "column": column_map,
        "row": cell_rows,
    });
    tracing::debug!("Elapsed time for get_page(): {}", elapsed);
    Ok(result)
}

// Given a table type, a column map, a cell value, and message list,
// return a JSON value representing this cell.
fn decorate_cell(
    table_type: &str,
    column_name: &str,
    column: &Value,
    value: &Value,
    messages: &Vec<ValveMessage>,
    history: &Vec<Vec<ValveChange>>,
) -> Map<String, Value> {
    let mut cell: Map<String, Value> = Map::new();
    cell.insert("value".to_string(), value.clone());

    let mut classes: Vec<String> = vec![];

    // Handle null and nulltype
    if value.is_null() {
        if let Some(nulltype) = column.get("nulltype") {
            if nulltype.is_string() {
                cell.insert("nulltype".to_string(), nulltype.clone());
            }
        }
    } else {
        let datatype = column
            .get("datatype")
            .expect("Column {k} must have a datatype in column_map {column_map:?}");
        cell.insert("datatype".to_string(), datatype.clone());
    }

    // Add links to other tables
    if ["table", "column"].contains(&table_type) && column_name == "table" {
        cell.insert("href".to_string(), json!(value));
    }

    // Handle messages associated with the row:
    let mut output_messages = vec![];
    let mut max_level = 0;
    let mut message_level = "none";
    for message in messages.iter().filter(|m| m.column == column_name) {
        // Override null values
        if value.is_null() {
            cell.insert("value".to_string(), json!(message.value));
        }
        output_messages.push(json!({
            "level": message.level,
            "rule": message.rule,
            "message": message.message,
        }));
        let level = level_to_int(&message.level);
        if level > max_level {
            max_level = level;
            message_level = message.level.as_str();
        }
    }

    if output_messages.len() > 0 {
        cell.insert("message_level".to_string(), json!(message_level));
        cell.insert("messages".to_string(), json!(output_messages));
    }

    let mut changes = vec![];
    for record in history.iter() {
        for change in record.iter().filter(|c| c.column == column_name) {
            changes.push(change);
        }
    }

    if changes.len() > 0 {
        cell.insert("history".to_string(), json!(changes));
    }

    if cell.get("value").unwrap().is_null() {
        classes.push("null".to_string());
    }

    if classes.len() > 0 {
        cell.insert("classes".to_string(), json!(classes));
    }

    cell
}

fn decorate_row(
    table: &str,
    table_type: &str,
    column_map: &Map<String, Value>,
    row: &Map<String, Value>,
) -> Map<String, Value> {
    // tracing::debug!("Decorate Row: table {table}");
    let messages: Vec<ValveMessage> = match row.get("message") {
        Some(serde_json::Value::Null) => vec![],
        Some(json_value) => match serde_json::from_value(json_value.clone()) {
            Ok(ms) => ms,
            Err(x) => {
                tracing::warn!("Could not parse message '{json_value:?}': {x:?}");
                vec![]
            }
        },
        None => vec![],
    };
    let history: Vec<Vec<ValveChange>> = match row.get("history") {
        Some(serde_json::Value::Null) => vec![],
        Some(json_value) => match serde_json::from_str(&json_value.as_str().unwrap_or_default()) {
            Ok(ms) => ms,
            Err(x) => {
                tracing::warn!("Could not parse history '{json_value:?}': {x:?}");
                vec![]
            }
        },
        None => vec![],
    };
    let mut cell_row: Map<String, Value> = SerdeMap::new();
    for (column_name, value) in row.iter() {
        // tracing::debug!("Decorate Row: column {column_name}");
        if table != "message" && ["message", "history"].contains(&column_name.as_str()) {
            continue;
        }
        let default_column = json!({
            "table": table.to_string(),
            "column": column_name.to_string(),
            "datatype": "integer",
        });
        let column = column_map.get(column_name).unwrap_or(&default_column);
        let cell = decorate_cell(table_type, column_name, column, value, &messages, &history);
        cell_row.insert(column_name.to_string(), serde_json::Value::Object(cell));
    }
    cell_row
}

pub fn get_change_message(record: &AnyRow) -> Option<String> {
    let table = record.try_get::<&str, &str>("table").ok()?;
    let row_number = record.try_get::<i64, &str>("row").ok()? + 1;
    let from = record.try_get::<&str, &str>("from").ok()?;
    let to = record.try_get::<&str, &str>("to").ok()?;
    let message = match (from, to) {
        ("", _) => format!("add row {row_number} to '{table}'"),
        (_, "") => format!("delete row {row_number} from '{table}'"),
        (_, _) => format!("update row {row_number} of '{table}'"),
    };
    Some(String::from(message))
}

// Get the undo message, or None.
pub fn get_undo_message(config: &Config) -> Option<String> {
    let pool = config.pool.as_ref()?;
    let record = block_on(valve::get_record_to_undo(pool)).ok()??;
    let message = get_change_message(&record)?;
    Some(String::from(format!("Undo {message}")))
}

// Get the redo message, or None.
pub fn get_redo_message(config: &Config) -> Option<String> {
    let pool = config.pool.as_ref()?;
    let record = block_on(valve::get_record_to_redo(pool)).ok()??;
    let message = get_change_message(&record)?;
    Some(String::from(format!("Redo {message}")))
}

pub fn get_action_map(config: &Config) -> Result<SerdeMap, GetError> {
    let action_map: SerdeMap = config
        .actions
        .iter()
        .map(|(k, v)| (k.into(), v.clone().label.into()))
        .collect();
    Ok(action_map)
}

pub fn get_repo_details() -> Result<SerdeMap, GetError> {
    let mut result = SerdeMap::new();

    let repo = match Repository::open_from_env() {
        Ok(repo) => repo,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let head = match repo.head() {
        Ok(head) => Some(head),
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    let head = head
        .as_ref()
        .and_then(|h| h.shorthand())
        .unwrap_or_default();
    let local = repo.find_branch(&head, git2::BranchType::Local)?;
    tracing::debug!("GIT got local: {head}, {:?}", local.name()?);
    result.insert("head".into(), head.into());
    result.insert("local".into(), local.name()?.into());

    let upstream = local.upstream();
    if let Ok(upstream) = upstream {
        let (ahead, behind) = repo.graph_ahead_behind(
            local.get().target().unwrap(),
            upstream.get().target().unwrap(),
        )?;
        let remote = repo.find_remote("origin")?;
        let remote_url = format!(
            "{}/tree/{}",
            remote
                .url()
                .ok_or("No URL?")
                .unwrap_or_default()
                .trim_end_matches(".git"),
            upstream
                .name()?
                .unwrap_or_default()
                .trim_start_matches("origin/")
        );
        tracing::debug!(
            "GIT got remote: {ahead} ahead {behind} behind {:?}, {remote_url}",
            upstream.name()?
        );
        result.insert("upstream".into(), upstream.name()?.into());
        result.insert("remote_url".into(), remote_url.into());
        result.insert("ahead".into(), ahead.into());
        result.insert("behind".into(), behind.into());
    } else {
        tracing::debug!("GIT no upstream branch");
    }

    // https://github.com/ontodev/nanobot.rs/tree/refine-ui
    let mut opts = git2::StatusOptions::new();
    opts.include_ignored(false);
    opts.include_untracked(false);
    opts.exclude_submodules(true);
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        let uncommitted = statuses.len() > 0;
        tracing::debug!("GIT got status: {uncommitted}");
        result.insert("uncommitted".into(), uncommitted.into());
    }
    let path = repo.path().join("FETCH_HEAD");
    tracing::debug!("GIT repo path: {path:?} {}", path.is_file());
    if path.is_file() {
        let dt: DateTime<Utc> = fs::metadata(path)?.modified()?.clone().into();
        let fetched = format!("{}", dt.to_rfc3339());
        result.insert("fetched".into(), fetched.into());
    }

    Ok(result)
}

fn value_rows_to_strings(rows: &Vec<Map<String, Value>>) -> Result<Vec<Vec<String>>, GetError> {
    let mut lines = vec![];
    let mut row: Vec<String> = vec![];
    match rows.first().and_then(|f| Some(f.keys())) {
        Some(first_keys) => {
            for key in first_keys {
                row.push(key.clone());
            }
        }
        None => return Ok(lines),
    };
    lines.push(row);

    for row in rows {
        let mut cells = vec![];
        for cell in row.values() {
            let mut value = cell.clone().to_string();
            if cell.is_string() {
                value = match cell.as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        return Err(GetError::new(format!(
                            "Could not convert '{}' to str",
                            cell
                        )))
                    }
                };
            } else if cell.is_null() {
                // TODO: better null handling
                value = "".to_string();
            }
            cells.push(value);
        }
        lines.push(cells);
    }
    Ok(lines)
}

fn value_rows_to_xsv(rows: &Vec<Map<String, Value>>, delimiter: u8) -> Result<String, GetError> {
    let lines = value_rows_to_strings(rows)?;
    let mut writer = WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(vec![]);
    for line in lines {
        writer.write_record(line)?;
    }
    let writer = match writer.into_inner() {
        Ok(w) => w,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    match String::from_utf8(writer) {
        Ok(text) => Ok(text),
        Err(e) => Err(GetError::new(e.to_string())),
    }
}

fn value_rows_to_csv(rows: &Vec<Map<String, Value>>) -> Result<String, GetError> {
    value_rows_to_xsv(rows, b',')
}

fn value_rows_to_tsv(rows: &Vec<Map<String, Value>>) -> Result<String, GetError> {
    value_rows_to_xsv(rows, b'\t')
}

fn value_rows_to_text(rows: &Vec<Map<String, Value>>) -> Result<String, GetError> {
    let tsv = value_rows_to_tsv(rows).unwrap_or_default();

    // Format using elastic tabstops
    let mut tw = TabWriter::new(vec![]);
    if let Err(e) = write!(&mut tw, "{}", tsv) {
        return Err(GetError::new(e.to_string()));
    }
    if let Err(e) = tw.flush() {
        return Err(GetError::new(e.to_string()));
    }

    match tw.into_inner() {
        Ok(tw) => match String::from_utf8(tw) {
            Ok(s) => Ok(s),
            Err(e) => Err(GetError::new(e.to_string())),
        },
        Err(e) => Err(GetError::new(e.to_string())),
    }
}

fn level_to_int(level: &String) -> usize {
    match level.to_lowercase().as_str() {
        "error" => 4,
        "warn" => 3,
        "info" => 2,
        "update" => 1,
        _ => 0,
    }
}

fn level_to_bootstrap(level: String) -> String {
    match level.to_lowercase().as_str() {
        "error" => "danger",
        "warn" => "warning",
        "update" => "success",
        x => x,
    }
    .to_string()
}

fn name_to_id(name: String) -> String {
    let re: Regex = Regex::new(r"\W").unwrap();
    re.replace_all(&name, "-").to_string()
}

// TODO: Don't rebuild the Minijinja environment on every call!
pub fn page_to_html(config: &Config, template: &str, page: &Value) -> Result<String, GetError> {
    tracing::info!("page_to_html {:?} {}", config.template_path, template);
    let page_html = include_str!("resources/page.html");
    let table_html = include_str!("resources/table.html");
    let form_html = include_str!("resources/form.html");
    let tree_html = include_str!("resources/tree.html");
    let action_html = include_str!("resources/action.html");

    let mut env = Environment::new();
    env.add_filter("level_to_bootstrap", level_to_bootstrap);
    env.add_filter("id", name_to_id);

    if let Some(t) = &config.template_path {
        tracing::info!("Adding template source {}", t);
        env.set_source(Source::from_path(t));
        let path = Path::new(t).join("page.html");
        if !path.is_file() {
            env.add_template("page.html", page_html).unwrap();
        }
        let path = Path::new(t).join("table.html");
        if !path.is_file() {
            env.add_template("table.html", table_html).unwrap();
        }
        let path = Path::new(t).join("form.html");
        if !path.is_file() {
            env.add_template("form.html", form_html).unwrap();
        }
        let path = Path::new(t).join("tree.html");
        if !path.is_file() {
            env.add_template("tree.html", tree_html).unwrap();
        }
        let path = Path::new(t).join("action.html");
        if !path.is_file() {
            env.add_template("action.html", action_html).unwrap();
        }
    } else {
        tracing::info!("Adding default templates");
        env.add_template("page.html", page_html).unwrap();
        env.add_template("table.html", table_html).unwrap();
        env.add_template("form.html", form_html).unwrap();
        env.add_template("tree.html", tree_html).unwrap();
        env.add_template("action.html", action_html).unwrap();
    }

    let template = match env.get_template(format!("{}.html", template).as_str()) {
        Ok(t) => t,
        Err(e) => return Err(GetError::new(e.to_string())),
    };
    match template.render(page) {
        Ok(p) => Ok(p),
        Err(e) => return Err(GetError::new(e.to_string())),
    }
}
