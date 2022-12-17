use crate::sql::{
    get_count_from_pool, get_message_counts_from_pool, get_rows_from_pool, get_table_from_pool,
    get_total_from_pool, rows_to_map, select_to_url, Operator, Select, LIMIT_DEFAULT, LIMIT_MAX,
};
use minijinja::Environment;
use regex::Regex;
use serde_json::{json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
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

pub async fn get_table(
    database: &str,
    table: &str,
    shape: &str,
    format: &str,
) -> Result<String, GetError> {
    let select = Select {
        table: table.to_string(),
        limit: LIMIT_DEFAULT,
        ..Default::default()
    };
    get_rows(database, &select, shape, format).await
}

pub async fn get_rows(
    database: &str,
    base_select: &Select,
    shape: &str,
    format: &str,
) -> Result<String, GetError> {
    let connection_string = format!("sqlite://{}?mode=rwc", database);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    // Get all the tables
    let select = Select {
        table: "table".to_string(),
        select: vec!["table", "path", "type", "description"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        ..Default::default()
    };
    let table_rows = get_table_from_pool(&pool, &select).await?;
    let table_map = rows_to_map(table_rows, "table");
    if !table_map.contains_key(&base_select.table) {
        return Err(GetError::new(format!(
            "Invalid table '{}'",
            &base_select.table
        )));
    }

    // Get the columns for the selected table
    let select = Select {
        table: "column".to_string(),
        select: vec!["column", "nulltype", "datatype", "structure", "description"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        filter: vec![(
            "table".to_string(),
            Operator::Equals,
            Value::String(base_select.table.clone()),
        )],
        ..Default::default()
    };
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
    let mut limit = base_select.limit;
    if limit > LIMIT_MAX {
        limit = LIMIT_MAX;
    } else if limit == 0 {
        limit = LIMIT_DEFAULT;
    }
    let select = Select {
        select: columns,
        limit,
        ..base_select.clone()
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
            let page: Value = get_page(&pool, &select, &table_map, &column_rows).await?;
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
    pool: &SqlitePool,
    select: &Select,
    table_map: &Map<String, Value>,
    column_rows: &Vec<Map<String, Value>>,
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
            if filter.0 == key {
                filters.push(json!([
                    filter.0.clone(),
                    filter.1.clone(),
                    filter.2.clone(),
                ]));
            }
        }
        if filters.len() > 0 {
            r.insert("filters".to_string(), Value::Array(filters));
        }
        // TODO: order
        column_map.insert(key, Value::Object(r));
    }

    // get the data and the messages
    let start = std::time::Instant::now();
    let mut view_select = Select {
        table: format!("{}_view", select.table.clone()),
        ..select.clone()
    };

    // If we're filtering for rows with messages
    if select.message != "" {
        let sql = format!(
            r#"
            SELECT json_object('row', row) AS json_result
            FROM (
              SELECT DISTINCT row
              FROM message
              WHERE "table" = '{}'
              ORDER BY row
              LIMIT {}
              OFFSET {}
            )
        "#,
            select.table, select.limit, select.offset
        );
        let result = get_rows_from_pool(&pool, &sql).await?;
        let row_numbers: Vec<Value> = result
            .clone()
            .into_iter()
            .map(|r| r.get("row").unwrap().clone())
            .collect();
        view_select = Select {
            filter: vec![(
                "row_number".to_string(),
                Operator::In,
                json!(row_numbers.clone()),
            )],
            offset: 0,
            ..view_select
        };
    }

    // Use the view to select the data
    let value_rows = get_table_from_pool(&pool, &view_select).await?;
    let row_numbers: Vec<Value> = value_rows
        .clone()
        .into_iter()
        .map(|r| r.get("row_number").unwrap().clone())
        .collect();
    let select_messages = Select {
        table: "message".to_string(),
        select: vec!["table", "row", "column", "level", "rule", "message"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        filter: vec![
            (
                "table".to_string(),
                Operator::Equals,
                json!(select.table.clone()),
            ),
            ("row".to_string(), Operator::In, json!(row_numbers.clone())),
        ],
        limit: 1000,
        ..Default::default()
    };
    let message_rows = get_table_from_pool(&pool, &select_messages).await?;
    let message_counts = get_message_counts_from_pool(&pool, &select.table.clone()).await?;

    // convert value_rows to cell_rows
    let mut cell_rows: Vec<Map<String, Value>> = vec![];
    for row in &value_rows {
        let mut crow: Map<String, Value> = Map::new();
        let row_number = row.get("row_number").unwrap();
        for (k, v) in row.iter() {
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
            } else if k == "table" && select.table == "table" {
                // In the 'table' table, link to the other tables
                let href = format!("/{}", v.as_str().unwrap().to_string());
                cell.insert("href".to_string(), Value::String(href));
            }

            // collect messages
            let mut messages: Vec<Map<String, Value>> = vec![];
            let mut max_level: usize = 0;
            let mut message_level = "info".to_string();
            for message in &message_rows {
                if row_number == message.get("row").unwrap() && k == message.get("column").unwrap()
                {
                    let mut m = Map::new();
                    for (key, value) in message {
                        if key == "table" || key == "row" || key == "column" {
                            continue;
                        }
                        m.insert(key.clone(), value.clone());
                    }
                    messages.push(m);
                    let level = message.get("level").unwrap().as_str().unwrap().to_string();
                    let lvl = level_to_int(&level);
                    if lvl > max_level {
                        max_level = lvl;
                        message_level = level;
                    }
                }
            }
            if messages.len() > 0 {
                cell.insert("message_level".to_string(), json!(message_level));
                cell.insert("messages".to_string(), json!(messages));
            }
            if classes.len() > 0 {
                cell.insert("classes".to_string(), json!(classes));
            }

            crow.insert(k.to_string(), Value::Object(cell));
        }
        cell_rows.push(crow);
    }

    let mut counts = Map::new();
    let mut count = get_count_from_pool(&pool, &select).await?;
    if select.message != "" {
        count = message_counts
            .get("message_row")
            .unwrap()
            .as_u64()
            .unwrap()
            .clone() as usize;
    }
    counts.insert("count".to_string(), json!(count));

    let total = get_total_from_pool(&pool, &select.table).await?;
    counts.insert("total".to_string(), json!(total));
    for (k, v) in message_counts {
        counts.insert(k, v);
    }

    let end = select.offset + cell_rows.len();

    let mut this_table = table_map
        .get(&select.table)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    this_table.insert("table".to_string(), json!(select.table.clone()));
    this_table.insert("href".to_string(), json!(format!("/{}", select.table)));
    this_table.insert("start".to_string(), json!(select.offset + 1));
    this_table.insert("end".to_string(), json!(end));
    this_table.insert("counts".to_string(), json!(counts));

    let mut formats = Map::new();
    let href = select_to_url(&Select {
        table: format!("{}.json", select.table),
        ..select.clone()
    });
    formats.insert("JSON".to_string(), json!(href));
    let href = select_to_url(&Select {
        table: format!("{}.pretty.json", select.table),
        ..select.clone()
    });
    formats.insert("JSON (Pretty)".to_string(), json!(href));
    this_table.insert("formats".to_string(), json!(formats));

    // Pagination
    if select.offset > 0 {
        let href = select_to_url(&Select {
            offset: 0,
            ..select.clone()
        });
        this_table.insert("first".to_string(), json!(href));
        if select.offset > select.limit {
            let href = select_to_url(&Select {
                offset: select.offset - select.limit,
                ..select.clone()
            });
            this_table.insert("previous".to_string(), json!(href));
        } else {
            this_table.insert("previous".to_string(), json!(href));
        }
    }
    if end < count {
        let href = select_to_url(&Select {
            offset: select.offset + select.limit,
            ..select.clone()
        });
        this_table.insert("next".to_string(), json!(href));
        let remainder = count % select.limit;
        let last = if remainder == 0 {
            count - select.limit
        } else {
            count - (count % select.limit)
        };
        let href = select_to_url(&Select {
            offset: last,
            ..select.clone()
        });
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
            "title": select.table,
            "select": select,
            "elapsed": start.elapsed().as_millis() as usize,
        },
        "table": this_table,
        "column": column_map,
        "row": cell_rows,
    });
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
    env.add_template("page.html", include_str!("resources/page.html"))
        .unwrap();
    env.add_template("table.html", include_str!("resources/table.html"))
        .unwrap();

    let template = env.get_template("table.html").unwrap();
    template.render(page).unwrap()
}
