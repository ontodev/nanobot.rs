use crate::serve;
use crate::sql::{
    get_count_from_pool, get_table_from_pool, query_to_url, rows_to_map, Operator, Query,
    LIMIT_DEFAULT, LIMIT_MAX,
};
use minijinja::Environment;
use serde_json::{json, Map, Value};
use sqlx::sqlite::{SqlitePoolOptions, SqliteRow};
use sqlx::Row;
use std::io::Write;
use tabwriter::TabWriter;

fn format_table_stdout(rows: &Vec<SqliteRow>) -> String {
    //transform rows into a single string
    let rows_string = rows
        .iter()
        .map(|r| {
            format!(
                "{}\t{}\t{}\t{}",
                r.get::<String, _>("table"),
                r.get::<String, _>("path"),
                r.get::<String, _>("type"),
                r.get::<String, _>("description")
            )
        })
        .collect::<Vec<String>>()
        .join("\n");

    //add header information
    let mut rows_with_header: String = "table\tpath\ttype\tdescription\n".to_owned();
    rows_with_header.push_str(&rows_string);

    //format using elastic tabstops
    let mut tw = TabWriter::new(vec![]);
    write!(&mut tw, "{}", rows_with_header).unwrap();
    tw.flush().unwrap();

    //return result
    let result = String::from_utf8(tw.into_inner().unwrap()).unwrap();
    result
}

fn format_table_json(rows: &Vec<SqliteRow>) -> String {
    let json_vector = rows
        .iter()
        .map(|r| {
            let mut map = Map::new();
            map.insert(
                String::from("table"),
                Value::String(r.get::<String, _>("table")),
            );
            map.insert(
                String::from("path"),
                Value::String(r.get::<String, _>("path")),
            );
            map.insert(
                String::from("type"),
                Value::String(r.get::<String, _>("type")),
            );
            map.insert(
                String::from("description"),
                Value::String(r.get::<String, _>("description")),
            );
            Value::Object(map)
        })
        .collect::<Vec<Value>>();

    let array = Value::Array(json_vector);
    array.to_string()
}

pub async fn get_table_from_database(
    database: &str,
    table: &str,
    format: &str,
) -> Result<String, String> {
    let connection_string = format!("sqlite://{}?mode=rwc", database);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let table_checked = match table {
        "table" => "table",
        _ => panic!("Unsupported table"),
    };

    let query_string = format!("SELECT * FROM '{}'", &table_checked);
    let rows: Vec<SqliteRow> = sqlx::query(&query_string).fetch_all(&pool).await.unwrap();

    let result = match format {
        "stdout" => format_table_stdout(&rows),
        "json" => format_table_json(&rows),
        _ => panic!("Format '{}' not supported", format),
    };

    Ok(result)
}

pub async fn get_table(table: String, params: serve::Params) -> Result<String, sqlx::Error> {
    // 1. connect to the database
    // 2. get the 'table' table
    // 3. get columns
    // 4. get datatype tree
    // 5. get the actual rows
    // 6. get the nulltypes
    // 7. get the messages
    // 8. merge
    // 9. render template

    let database = ".nanobot.db";
    let connection_string = format!("sqlite://{}?mode=rwc", database);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    // Get all the tables
    let query = Query {
        table: "table".to_string(),
        select: vec!["table", "path", "type", "description"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        ..Default::default()
    };
    let table_rows = get_table_from_pool(&pool, &query).await?;
    let table_map = rows_to_map(table_rows, "table");

    // Get the columns for the selected table
    let query = Query {
        table: "column".to_string(),
        select: vec!["column", "nulltype", "datatype", "structure", "description"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        filter: vec![(
            "table".to_string(),
            Operator::EQUALS,
            Value::String(table.clone()),
        )],
        ..Default::default()
    };
    let column_rows = get_table_from_pool(&pool, &query).await?;

    // TODO: collect and fetch datatypes

    let mut limit = LIMIT_DEFAULT;
    if let Some(x) = params.limit {
        if x < LIMIT_MAX {
            limit = x;
        }
    }
    let mut filter: Vec<(String, Operator, Value)> = vec![];
    if let Some(value) = &params.table {
        let v = value.clone().replace("eq.", "");
        if table_map.contains_key(&v) {
            filter.push((
                "table".to_string(),
                Operator::EQUALS,
                Value::String(v.clone()),
            ));
        }
    }
    let mut select: Vec<String> = vec!["row_number".to_string()];
    select.append(
        &mut column_rows
            .clone()
            .into_iter()
            .map(|r| r.get("column").unwrap().as_str().unwrap().to_string())
            .collect(),
    );
    let query = Query {
        table: table.clone(),
        select: select,
        filter: filter,
        limit: limit.clone(),
        offset: params.offset.unwrap_or_default(),
        ..Default::default()
    };
    let value_rows = get_table_from_pool(&pool, &query).await?;

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
        for filter in &query.filter {
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
        column_map.insert(key, Value::Object(r));
    }

    // TODO: get the nulltypes
    // TODO: get the messages

    // convert value_rows to cell_rows
    let mut cell_rows: Vec<Map<String, Value>> = vec![];
    for row in &value_rows {
        let mut crow: Map<String, Value> = Map::new();
        for (k, v) in row.iter() {
            let mut cell: Map<String, Value> = Map::new();
            cell.insert("value".to_string(), v.clone());
            if k == "row_number" {
                cell.insert("datatype".to_string(), Value::String("integer".to_string()));
                crow.insert(k.to_string(), Value::Object(cell));
                continue;
            }
            if v.is_null() {
                if let Some(nulltype) = column_map.get(k).unwrap().get("nulltype") {
                    if nulltype.is_string() {
                        cell.insert("nulltype".to_string(), nulltype.clone());
                    }
                }
            }
            if !cell.contains_key("nulltype") {
                let datatype = column_map.get(k).unwrap().get("datatype").unwrap();
                cell.insert("datatype".to_string(), datatype.clone());
            }
            let structure = column_map.get(k).unwrap().get("structure").unwrap();
            if structure == "from(table.table)" {
                let href = format!("/table?table=eq.{}", v.as_str().unwrap().to_string());
                cell.insert("href".to_string(), Value::String(href));
            } else if k == "table" && table == "table" {
                // In the 'table' table, link to the other tables
                let href = format!("/{}", v.as_str().unwrap().to_string());
                cell.insert("href".to_string(), Value::String(href));
            }
            crow.insert(k.to_string(), Value::Object(cell));
        }
        cell_rows.push(crow);
    }

    let count = get_count_from_pool(&pool, &query).await?;
    let end = query.offset + cell_rows.len();

    let mut this_table = table_map.get(&table).unwrap().as_object().unwrap().clone();
    this_table.insert("table".to_string(), json!(table.clone()));
    this_table.insert("href".to_string(), json!(format!("/{}", table)));
    this_table.insert("start".to_string(), json!(query.offset + 1));
    this_table.insert("end".to_string(), json!(end));
    this_table.insert("count".to_string(), json!(count));

    // Pagination
    if query.offset > 0 {
        let href = query_to_url(&Query {
            offset: 0,
            ..query.clone()
        });
        this_table.insert("first".to_string(), json!(href));
        if query.offset > query.limit {
            let href = query_to_url(&Query {
                offset: query.offset - query.limit,
                ..query.clone()
            });
            this_table.insert("previous".to_string(), json!(href));
        } else {
            this_table.insert("previous".to_string(), json!(href));
        }
    }
    if end < count {
        let href = query_to_url(&Query {
            offset: query.offset + query.limit,
            ..query.clone()
        });
        this_table.insert("next".to_string(), json!(href));
        let remainder = count % query.limit;
        let last = if remainder == 0 {
            count - query.limit
        } else {
            count - (count % query.limit)
        };
        let href = query_to_url(&Query {
            offset: last,
            ..query.clone()
        });
        this_table.insert("last".to_string(), json!(href));
    }

    let mut tables = Map::new();
    for key in table_map.keys() {
        tables.insert(key.clone(), Value::String(format!("/{}", key)));
    }

    let data: Value = json!({
        "page": {
            "project_name": "Nanobot",
            "tables": tables,
            "title": table,
            "params": params,
            "query": query,
        },
        "table": this_table,
        "column": column_map,
        "row": cell_rows,
    });

    let mut env = Environment::new();
    env.add_template("debug.html", include_str!("resources/debug.html"))
        .unwrap();
    env.add_template("page.html", include_str!("resources/page.html"))
        .unwrap();
    env.add_template("table.html", include_str!("resources/table.html"))
        .unwrap();

    let template = env.get_template("table.html").unwrap();
    Ok(template.render(data).unwrap())
}
