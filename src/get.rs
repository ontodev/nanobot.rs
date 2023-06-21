// TODO: Have a general look through all of this code (and in the other modules) and see if
// we can use the valve config (which is now available) instead of running db requests. But do
// this later.

use crate::config::Config;
use crate::sql::{
    get_count_from_pool, get_message_counts_from_pool, get_table_from_pool, get_total_from_pool,
    LIMIT_DEFAULT, LIMIT_MAX,
};
use enquote::unquote;
use minijinja::{Environment, Source};
use ontodev_sqlrest::Select;
use regex::Regex;
use serde_json::{json, to_string_pretty, Map, Value};
use sqlx::any::AnyPool;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::Write;
use std::path::Path;
use tabwriter::TabWriter;
use urlencoding::decode;

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

impl From<sqlx::Error> for GetError {
    fn from(error: sqlx::Error) -> GetError {
        GetError::new(format!("{:?}", error))
    }
}

impl From<String> for GetError {
    fn from(error: String) -> GetError {
        GetError::new(error)
    }
}

pub async fn get_table(
    config: &Config,
    table: &str,
    shape: &str,
    format: &str,
) -> Result<String, GetError> {
    let table = unquote(table).unwrap_or(table.to_string());
    let mut select = Select::new(format!("\"{}\"", table));
    select.limit(LIMIT_DEFAULT);
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
        _ => select.limit(LIMIT_DEFAULT),
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
            let value_rows = match get_table_from_pool(&pool, &select).await {
                Ok(value_rows) => value_rows,
                Err(e) => return Err(GetError::new(e.to_string())),
            };
            match format {
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
            let page = match get_page(&pool, &select, &table_map, &column_rows).await {
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
    pool: &AnyPool,
    select: &Select,
    table_map: &Map<String, Value>,
    column_rows: &Vec<Map<String, Value>>,
) -> Result<Value, GetError> {
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
        let mut filters: Vec<Value> = vec![];
        for filter in &select.filter {
            if filter.lhs == key {
                filters.push(json!([
                    filter.lhs.clone(),
                    filter.operator.clone(),
                    filter.rhs.clone(),
                ]));
            }
        }
        if filters.len() > 0 {
            r.insert("filters".to_string(), Value::Array(filters));
        }
        // TODO: order
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
    // If this isn't the message table, explicitly include the message column from the table's view:
    if unquoted_table != "message" {
        view_select.add_select("message");
    }

    // Only apply the limit to the view query if we're filtering for rows with messages:
    if filter_messages {
        if let Some(limit) = select.limit {
            view_select.limit(limit);
        }
    }

    // Use the view to select the data
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
    let mut cell_rows: Vec<Map<String, Value>> = vec![];
    for row in &value_rows {
        let mut crow: Map<String, Value> = Map::new();
        for (k, v) in row.iter() {
            if unquoted_table != "message" && k == "message" {
                continue;
            }
            let mut cell: Map<String, Value> = Map::new();
            let mut classes: Vec<String> = vec![];

            // Add the value to the cell
            cell.insert("value".to_string(), v.clone());

            // Row numbers and message ids have an integer datatype but otherwise do not need to be
            // processed, so we continue:
            if (unquoted_table != "message" && k == "row_number")
                || (unquoted_table == "message" && k == "message_id")
            {
                cell.insert("datatype".to_string(), Value::String("integer".to_string()));
                crow.insert(k.to_string(), Value::Object(cell));
                continue;
            }

            // Handle null and nulltype
            if v.is_null() {
                classes.push("bg-null".to_string());
                match column_map.get(k) {
                    Some(column) => {
                        if let Some(nulltype) = column.get("nulltype") {
                            if nulltype.is_string() {
                                cell.insert("nulltype".to_string(), nulltype.clone());
                            }
                        }
                    }
                    None => {
                        return Err(GetError::new(format!(
                            "While handling nulltype: No key '{}' in column_map {:?}",
                            k, column_map
                        )))
                    }
                };
            }

            // Handle the datatype:
            if !cell.contains_key("nulltype") {
                let datatype = match column_map.get(k) {
                    Some(column) => match column.get("datatype") {
                        Some(datatype) => datatype,
                        None => {
                            return Err(GetError::new(format!(
                                "While handling datatype: No 'datatype' entry in {:?}",
                                column
                            )))
                        }
                    },
                    None => {
                        return Err(GetError::new(format!(
                            "No key '{}' in column_map {:?}",
                            k, column_map
                        )))
                    }
                };
                cell.insert("datatype".to_string(), datatype.clone());
            }
            // Handle structure
            match column_map.get(k) {
                Some(column) => {
                    let default_structure = json!("");
                    let structure = column.get("structure").unwrap_or(&default_structure);
                    if structure == "from(table.table)" {
                        let href = format!("table?table=eq.{}", {
                            match v.as_str() {
                                Some(s) => s.to_string(),
                                None => {
                                    return Err(GetError::new(format!(
                                        "Could not convert '{}' to str",
                                        v
                                    )))
                                }
                            }
                        });
                        cell.insert("href".to_string(), Value::String(href));
                    } else if k == "table" && unquoted_table == "table" {
                        // In the 'table' table, link to the other tables
                        let href = match v.as_str() {
                            Some(s) => s.to_string(),
                            None => {
                                return Err(GetError::new(format!(
                                    "Could not convert '{}' to str",
                                    v
                                )))
                            }
                        };
                        cell.insert("href".to_string(), Value::String(href));
                    }
                }
                None => {
                    return Err(GetError::new(format!(
                        "No key '{}' in column_map {:?}",
                        k, column_map
                    )))
                }
            };

            if classes.len() > 0 {
                cell.insert("classes".to_string(), json!(classes));
            }

            crow.insert(k.to_string(), Value::Object(cell));
        }

        // Handle messages associated with the row:
        let mut error_values = HashMap::new();
        if unquoted_table != "message" {
            if let Some(input_messages) = row.get("message") {
                let input_messages = match input_messages {
                    Value::Array(value) => value.clone(),
                    Value::String(value) => {
                        let value = unquote(&value).unwrap_or(value.to_string());
                        match serde_json::from_str::<Value>(value.as_str()) {
                            Err(e) => return Err(GetError::new(e.to_string())),
                            Ok(value) => match value.as_array() {
                                None => {
                                    return Err(GetError::new(format!(
                                        "Value '{}' is not an array.",
                                        value
                                    )))
                                }
                                Some(value) => value.to_vec(),
                            },
                        }
                    }
                    Value::Null => vec![],
                    _ => {
                        return Err(GetError::new(format!(
                            "'{}' is not a Value String or Value Array",
                            input_messages
                        )))
                    }
                };
                let mut output_messages: HashMap<&str, Vec<Map<String, Value>>> = HashMap::new();
                let mut max_level: usize = 0;
                let mut message_level = "info".to_string();
                for message in &input_messages {
                    let mut m = Map::new();
                    let message_map = match message.as_object() {
                        Some(o) => o,
                        None => {
                            return Err(GetError::new(format!("{:?} is not an object.", message)))
                        }
                    };
                    for (key, value) in message_map.iter() {
                        if key != "column" && key != "value" {
                            m.insert(key.clone(), value.clone());
                        }
                    }
                    let column = match message_map.get("column") {
                        Some(c) => match c.as_str() {
                            Some(s) => s,
                            None => {
                                return Err(GetError::new(format!(
                                    "Could not convert '{}' to str",
                                    c
                                )))
                            }
                        },
                        None => {
                            return Err(GetError::new(format!(
                                "No 'column' key in {:?}",
                                message_map
                            )))
                        }
                    };
                    let value = match message.get("value") {
                        Some(v) => match v.as_str() {
                            Some(s) => s,
                            None => {
                                return Err(GetError::new(format!(
                                    "Could not convert '{}' to str",
                                    v
                                )))
                            }
                        },
                        None => {
                            return Err(GetError::new(format!(
                                "No 'value' key in {:?}",
                                message_map
                            )))
                        }
                    };
                    error_values.insert(column.clone(), value);
                    if let Some(v) = output_messages.get_mut(&column) {
                        v.push(m);
                    } else {
                        output_messages.insert(column, vec![m]);
                    }

                    let level = match message.get("level") {
                        Some(v) => match v.as_str() {
                            Some(s) => s.to_string(),
                            None => {
                                return Err(GetError::new(format!(
                                    "Could not convert '{}' to str",
                                    v
                                )))
                            }
                        },
                        None => {
                            return Err(GetError::new(format!(
                                "No 'level' key in {:?}",
                                message_map
                            )))
                        }
                    };
                    let lvl = level_to_int(&level);
                    if lvl > max_level {
                        max_level = lvl;
                        message_level = level;
                    }
                }

                for (column, messages) in &output_messages {
                    if let Some(cell) = crow.get_mut(column.clone()) {
                        if let Some(cell) = cell.as_object_mut() {
                            cell.remove("nulltype");
                            let mut new_classes = vec![];
                            if let Some(classes) = cell.get_mut("classes") {
                                match classes.as_array() {
                                    None => {
                                        return Err(GetError::new(format!(
                                            "{:?} is not an array",
                                            classes
                                        )))
                                    }
                                    Some(classes_array) => {
                                        for class in classes_array {
                                            match class.as_str() {
                                                None => {
                                                    return Err(GetError::new(format!(
                                                        "Could not convert '{}' to str",
                                                        class
                                                    )))
                                                }
                                                Some(s) => {
                                                    if s.to_string() != "bg-null" {
                                                        new_classes.push(class.clone());
                                                    }
                                                }
                                            };
                                        }
                                    }
                                };
                            }
                            let value = match error_values.get(column) {
                                Some(v) => v,
                                None => {
                                    return Err(GetError::new(format!(
                                        "No '{}' in {:?}",
                                        column, error_values
                                    )))
                                }
                            };
                            cell.insert("value".to_string(), json!(value));
                            cell.insert("classes".to_string(), json!(new_classes));
                            cell.insert("message_level".to_string(), json!(message_level));
                            cell.insert("messages".to_string(), json!(messages));
                        }
                    }
                }
            }
        }

        cell_rows.push(crow);
    }

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
        tables.insert(key.clone(), Value::String(key.clone()));
    }

    let elapsed = start.elapsed().as_millis() as usize;
    let result: Value = json!({
        "page": {
            "project_name": "Nanobot",
            "tables": tables,
            "title": unquoted_table,
            "select": select,
            "elapsed": elapsed,
        },
        "table": this_table,
        "column": column_map,
        "row": cell_rows,
    });
    tracing::debug!("Elapsed time for get_page(): {}", elapsed);
    Ok(result)
}

fn value_rows_to_text(rows: &Vec<Map<String, Value>>) -> Result<String, GetError> {
    // This would be nicer with map, but I got weird borrowing errors.
    let mut lines: Vec<String> = vec![];
    let mut line: Vec<String> = vec![];
    match rows.first().and_then(|f| Some(f.keys())) {
        Some(first_keys) => {
            for key in first_keys {
                line.push(key.clone());
            }
        }
        None => return Ok("".to_string()),
    };
    lines.push(line.join("\t"));
    for row in rows {
        let mut line: Vec<String> = vec![];
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
            line.push(value);
        }
        lines.push(line.join("\t"));
    }

    // Format using elastic tabstops
    let mut tw = TabWriter::new(vec![]);
    if let Err(e) = write!(&mut tw, "{}", lines.join("\n")) {
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
    } else {
        tracing::info!("Adding default templates");
        env.add_template("page.html", page_html).unwrap();
        env.add_template("table.html", table_html).unwrap();
        env.add_template("form.html", form_html).unwrap();
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
