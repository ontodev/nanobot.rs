use crate::config::Config;
use crate::sql::{
    get_count_from_pool, get_message_counts_from_pool, get_table_from_pool, get_total_from_pool,
    rows_to_map, LIMIT_DEFAULT, LIMIT_MAX,
};
use enquote::unquote;
use minijinja::Environment;
use ontodev_sqlrest::{Filter, Select};
use regex::Regex;
use serde_json::{json, Map, Value};
use sqlx::any::{AnyKind, AnyPool};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::Write;
use tabwriter::TabWriter;

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
    show_messages: bool,
) -> Result<String, GetError> {
    let mut select = Select::new(table);
    select.limit(LIMIT_DEFAULT);
    get_rows(config, &select, shape, format, show_messages).await
}

pub async fn get_rows(
    config: &Config,
    base_select: &Select,
    shape: &str,
    format: &str,
    show_messages: bool,
) -> Result<String, GetError> {
    // Get all the tables
    let mut select = Select::new("\"table\"");
    select.select(vec!["\"table\"", "\"path\"", "\"type\"", "\"description\""]);
    let pool = match config.pool.as_ref() {
        Some(p) => p,
        _ => {
            return Err(GetError::new(format!(
                "Could not connect to database using pool {:?}",
                config.pool
            )))
        }
    };

    let table_rows = get_table_from_pool(&pool, &select).await?;
    let table_map = rows_to_map(table_rows, "table");
    let unquoted_table = unquote(&base_select.table).unwrap_or(base_select.table.to_string());
    if !table_map.contains_key(&unquoted_table) {
        return Err(GetError::new(format!("Invalid table '{}'", &base_select.table)));
    }

    // Get the columns for the selected table
    let mut select = Select::new("\"column\"");
    select
        .select(vec![
            "\"column\"",
            "\"nulltype\"",
            "\"datatype\"",
            "\"structure\"",
            "\"description\"",
        ])
        .filter(vec![
            match Filter::new("\"table\"", "eq", json!(format!("'{}'", unquoted_table,))) {
                Ok(f) => f,
                Err(e) => return Err(GetError::new(e)),
            },
        ]);

    let column_rows = get_table_from_pool(&pool, &select).await?;

    let mut columns: Vec<String> = vec![];
    if shape == "page" {
        columns.push("row_number".to_string());
    }
    columns.append(
        &mut column_rows
            .clone()
            .into_iter()
            .map(|r| r.get("column").unwrap().as_str().unwrap().to_string())
            .collect(),
    );

    let mut select = Select::clone(base_select);
    match select.limit {
        Some(l) if l > LIMIT_MAX => select.select(columns).limit(LIMIT_MAX),
        Some(l) if l > 0 => select.select(columns).limit(l),
        _ => select.limit(LIMIT_DEFAULT),
    };

    match shape {
        "value_rows" => {
            let value_rows = get_table_from_pool(&pool, &select).await?;
            match format {
                "text" => Ok(value_rows_to_text(&value_rows)),
                "json" => Ok(json!(value_rows).to_string()),
                "pretty.json" => Ok(serde_json::to_string_pretty(&json!(value_rows)).unwrap()),
                &_ => Err(GetError::new(format!(
                    "Shape '{}' does not support format '{}'",
                    shape, format
                ))),
            }
        }
        "page" => {
            let page: Value =
                get_page(&pool, &select, &table_map, &column_rows, show_messages).await?;
            match format {
                "json" => Ok(page.to_string()),
                "pretty.json" => Ok(serde_json::to_string_pretty(&page).unwrap()),
                "html" => Ok(page_to_html(&page)),
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
    filter_messages: bool,
) -> Result<Value, GetError> {
    // Annotate columns with filters and sorting
    let mut column_map = Map::new();
    for row in column_rows.iter() {
        let mut r = Map::new();
        let mut key = String::from("");
        for (k, v) in row.iter() {
            if k == "column" {
                key = v.as_str().unwrap().to_string();
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

    let unquoted_table = unquote(&select.table).unwrap_or(select.table.to_string());
    let start = std::time::Instant::now();
    // Query the table view instead of the base table, which includes conflict rows and the
    // message column:
    let mut view_select = Select { table: format!("{}_view", unquoted_table), ..select.clone() };
    view_select.select_all(pool).unwrap();

    // If we're filtering for rows with messages:
    if filter_messages {
        view_select
            .add_filter(
                Filter::new("message", "not_is", {
                    if pool.any_kind() == AnyKind::Postgres {
                        "null".into()
                    } else {
                        "'[]'".into()
                    }
                })
                .unwrap(),
            )
            .limit(1000);
    }

    // Use the view to select the data
    let value_rows = get_table_from_pool(&pool, &view_select).await?;
    let message_counts = get_message_counts_from_pool(&pool, &unquoted_table).await?;

    // convert value_rows to cell_rows
    let mut cell_rows: Vec<Map<String, Value>> = vec![];
    for row in &value_rows {
        let mut crow: Map<String, Value> = Map::new();
        for (k, v) in row.iter() {
            if k == "message" {
                continue;
            }
            let mut cell: Map<String, Value> = Map::new();
            let mut classes: Vec<String> = vec![];

            // handle the value
            cell.insert("value".to_string(), v.clone());
            if k == "row_number" {
                cell.insert("datatype".to_string(), Value::String("integer".to_string()));
                crow.insert(k.to_string(), Value::Object(cell));
                continue;
            }

            // handle null and nulltype
            if v.is_null() {
                classes.push("bg-null".to_string());
                if let Some(nulltype) = column_map.get(k).unwrap().get("nulltype") {
                    if nulltype.is_string() {
                        cell.insert("nulltype".to_string(), nulltype.clone());
                    }
                }
            }

            // handle datatype
            if !cell.contains_key("nulltype") {
                let datatype = column_map.get(k).unwrap().get("datatype").unwrap();
                cell.insert("datatype".to_string(), datatype.clone());
            }
            let structure = column_map.get(k).unwrap().get("structure").unwrap();
            if structure == "from(table.table)" {
                let href = format!("/table?table=eq.{}", v.as_str().unwrap().to_string());
                cell.insert("href".to_string(), Value::String(href));
            } else if k == "table" && unquoted_table == "table" {
                // In the 'table' table, link to the other tables
                let href = format!("/{}", v.as_str().unwrap().to_string());
                cell.insert("href".to_string(), Value::String(href));
            }

            if classes.len() > 0 {
                cell.insert("classes".to_string(), json!(classes));
            }

            crow.insert(k.to_string(), Value::Object(cell));
        }

        let mut error_values = HashMap::new();
        if let Some(input_messages) = row.get("message") {
            let mut output_messages: HashMap<&str, Vec<Map<String, Value>>> = HashMap::new();
            let mut max_level: usize = 0;
            let mut message_level = "info".to_string();
            for message in input_messages.as_array().unwrap() {
                let mut m = Map::new();
                for (key, value) in message.as_object().unwrap() {
                    if key != "column" && key != "value" {
                        m.insert(key.clone(), value.clone());
                    }
                }
                let column = message.as_object().unwrap().get("column").unwrap().as_str().unwrap();
                let value = message.get("value").unwrap().as_str().unwrap();
                error_values.insert(column.clone(), value);
                if let Some(mut v) = output_messages.get_mut(&column) {
                    v.push(m);
                } else {
                    output_messages.insert(column, vec![m]);
                }

                let level = message.get("level").unwrap().as_str().unwrap().to_string();
                let lvl = level_to_int(&level);
                if lvl > max_level {
                    max_level = lvl;
                    message_level = level;
                }
            }

            for (column, messages) in &output_messages {
                if let Some(mut cell) = crow.get_mut(column.clone()) {
                    if let Some(mut cell) = cell.as_object_mut() {
                        cell.remove("nulltype");
                        let mut new_classes = vec![];
                        if let Some(mut classes) = cell.get_mut("classes") {
                            for class in classes.as_array().unwrap() {
                                if class.as_str().unwrap().to_string() != "bg-null" {
                                    new_classes.push(class.clone());
                                }
                            }
                        }
                        let value = error_values.get(column).unwrap();
                        cell.insert("value".to_string(), json!(value));
                        cell.insert("classes".to_string(), json!(new_classes));
                        cell.insert("message_level".to_string(), json!(message_level));
                        cell.insert("messages".to_string(), json!(messages));
                    }
                }
            }
        }

        cell_rows.push(crow);
    }

    let mut counts = Map::new();
    let count = {
        if filter_messages {
            message_counts.get("message_row").unwrap().as_u64().unwrap().clone() as usize
        } else {
            get_count_from_pool(&pool, &select).await?
        }
    };
    counts.insert("count".to_string(), json!(count));

    let total = get_total_from_pool(&pool, &unquoted_table).await?;
    counts.insert("total".to_string(), json!(total));
    for (k, v) in message_counts {
        counts.insert(k, v.into());
    }

    let end = select.offset.unwrap_or(0) + cell_rows.len();

    let mut this_table = table_map.get(&unquoted_table).unwrap().as_object().unwrap().clone();
    this_table.insert("table".to_string(), json!(unquoted_table.clone()));
    this_table.insert("href".to_string(), json!(format!("/{}", unquoted_table)));
    this_table.insert("start".to_string(), json!(select.offset.unwrap_or(0) + 1));
    this_table.insert("end".to_string(), json!(end));
    this_table.insert("counts".to_string(), json!(counts));

    let mut formats = Map::new();

    let mut select_format = Select::clone(select);
    select_format.table(format!("\"{}.json\"", unquoted_table));

    let href = select_format.to_url();
    formats.insert("JSON".to_string(), json!(href));

    select_format.table(format!("\"{}.pretty.json\"", unquoted_table));
    let href = select_format.to_url();

    formats.insert("JSON (Pretty)".to_string(), json!(href));
    this_table.insert("formats".to_string(), json!(formats));

    // Pagination
    let mut select_offset = Select::clone(select);
    match select.offset {
        Some(offset) if offset > 0 => {
            let href = select_offset.offset(0).to_url();
            this_table.insert("first".to_string(), json!(href));
            if offset > select.limit.unwrap_or(0) {
                let href = select_offset.offset(offset - select.limit.unwrap_or(0)).to_url();
                this_table.insert("previous".to_string(), json!(href));
            } else {
                this_table.insert("previous".to_string(), json!(href));
            }
        }
        _ => (),
    };
    if end < count {
        let href =
            select_offset.offset(select.offset.unwrap_or(0) + select.limit.unwrap_or(0)).to_url();
        this_table.insert("next".to_string(), json!(href));
        let remainder = count % select.limit.unwrap_or(0);
        let last = if remainder == 0 {
            count - select.limit.unwrap_or(0)
        } else {
            count - (count % select.limit.unwrap_or(0))
        };
        let href = select_offset.offset(last).to_url();
        this_table.insert("last".to_string(), json!(href));
    }

    let mut tables = Map::new();
    for key in table_map.keys() {
        tables.insert(key.clone(), Value::String(format!("/{}", key)));
    }

    let result: Value = json!({
        "page": {
            "project_name": "Nanobot",
            "tables": tables,
            "title": unquoted_table,
            "select": select,
            "elapsed": start.elapsed().as_millis() as usize,
        },
        "table": this_table,
        "column": column_map,
        "row": cell_rows,
    });
    //tracing::info!("RESULT: {:#?}", result);
    Ok(result)
}

fn value_rows_to_text(rows: &Vec<Map<String, Value>>) -> String {
    if rows.len() == 0 {
        return "".to_string();
    }

    // This would be nicer with map, but I got weird borrowing errors.
    let mut lines: Vec<String> = vec![];
    let mut line: Vec<String> = vec![];
    for key in rows.first().unwrap().keys() {
        line.push(key.clone());
    }
    lines.push(line.join("\t"));
    for row in rows {
        let mut line: Vec<String> = vec![];
        for cell in row.values() {
            let mut value = cell.clone().to_string();
            if cell.is_string() {
                value = cell.as_str().unwrap().to_string();
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
    write!(&mut tw, "{}", lines.join("\n")).unwrap();
    tw.flush().unwrap();

    String::from_utf8(tw.into_inner().unwrap()).unwrap()
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

fn page_to_html(page: &Value) -> String {
    let mut env = Environment::new();
    env.add_filter("level_to_bootstrap", level_to_bootstrap);
    env.add_filter("id", name_to_id);
    env.add_template("page.html", include_str!("resources/page.html")).unwrap();
    env.add_template("table.html", include_str!("resources/table.html")).unwrap();

    let template = env.get_template("table.html").unwrap();
    template.render(page).unwrap()
}
