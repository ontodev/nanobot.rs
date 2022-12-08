use crate::serve;
use minijinja::Environment;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::Row;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Operator {
    EQUALS,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Direction {
    ASC,
    DESC,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Query {
    pub table: String,
    pub select: Vec<String>,
    pub filter: Vec<(String, Operator, Value)>,
    pub order: Vec<(String, Direction)>,
    pub limit: usize,
    pub offset: usize,
}

/// Convert a Query struct to a SQL string.
///
/// ```sql
/// SELECT json_object(
///     'table', "table",
///     'path', "path",
///     'type', "type",
///     'description', "description"
/// ) AS json_result
/// FROM "table";
/// ```
///
/// # Examples
///
/// ```
/// assert_eq!("foo", "foo");
/// ```
pub fn query_to_sql(q: &Query) -> String {
    let mut lines: Vec<String> = vec!["SELECT json_object(".to_string()];
    let parts: Vec<String> = q
        .select
        .iter()
        .map(|c| format!(r#"'{}', "{}""#, c, c))
        .collect();
    lines.push(format!("  {}", parts.join(",\n  ")));
    lines.push(") AS json_result".to_string());
    lines.push(format!(r#"FROM "{}""#, q.table));
    let mut filters: Vec<String> = vec![];
    if q.filter.len() > 0 {
        for filter in &q.filter {
            filters.push(format!(
                r#""{}" = '{}'"#,
                filter.0,
                filter.2.as_str().unwrap().to_string()
            ));
        }
        lines.push(format!("WHERE {}", filters.join("\n  AND ")));
    }
    if q.order.len() > 0 {
        let parts: Vec<String> = q
            .order
            .iter()
            .map(|(c, d)| format!(r#""{}" {:?}"#, c, d))
            .collect();
        lines.push(format!("ORDER BY {}", parts.join(", ")));
    }
    if q.limit > 0 {
        lines.push(format!("LIMIT {}", q.limit));
    }
    if q.offset > 0 {
        lines.push(format!("OFFSET {}", q.offset));
    }
    lines.join("\n")
}

pub fn query_to_sql_count(q: &Query) -> String {
    let mut lines: Vec<String> = vec!["SELECT COUNT(*) AS count".to_string()];
    lines.push(format!(r#"FROM "{}""#, q.table));
    let mut filters: Vec<String> = vec![];
    if q.filter.len() > 0 {
        for filter in &q.filter {
            filters.push(format!(
                r#""{}" = '{}'"#,
                filter.0,
                filter.2.as_str().unwrap().to_string()
            ));
        }
        lines.push(format!("WHERE {}", filters.join("\n  AND ")));
    }
    lines.join("\n")
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

    // let mut limit = 100;
    let mut limit = 10;
    if let Some(x) = params.limit {
        if x < limit {
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
    let query = Query {
        table: table.clone(),
        select: column_rows
            .clone()
            .into_iter()
            .map(|r| r.get("column").unwrap().as_str().unwrap().to_string())
            .collect(),
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
            let datatype = column_map.get(k).unwrap().get("datatype").unwrap();
            cell.insert("datatype".to_string(), datatype.clone());
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
    // TODO: Account for the current filters
    if query.offset > 0 {
        this_table.insert("first".to_string(), json!(format!("/{}", table)));
        if query.offset > query.limit {
            let prev = query.offset - query.limit;
            this_table.insert("previous".to_string(), json!(format!("?offset={}", prev)));
        } else {
            this_table.insert("previous".to_string(), json!(format!("/{}", table)));
        }
    }
    if end < count {
        let next = query.offset + query.limit;
        this_table.insert("next".to_string(), json!(format!("?offset={}", next)));
        let remainder = count % query.limit;
        let last = if remainder == 0 {
            count - query.limit
        } else {
            count - (count % query.limit)
        };
        this_table.insert("last".to_string(), json!(format!("?offset={}", last)));
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

async fn get_table_from_pool(
    pool: &SqlitePool,
    query: &Query,
) -> Result<Vec<Map<String, Value>>, sqlx::Error> {
    let sql = query_to_sql(query);
    let rows: Vec<SqliteRow> = sqlx::query(&sql).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| {
            let result: &str = row.get("json_result");
            from_str::<Map<String, Value>>(&result).unwrap()
        })
        .collect())
}

async fn get_count_from_pool(pool: &SqlitePool, query: &Query) -> Result<usize, sqlx::Error> {
    let sql = query_to_sql_count(query);
    let row: SqliteRow = sqlx::query(&sql).fetch_one(pool).await?;
    let count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();
    Ok(count)
}

fn rows_to_map(rows: Vec<Map<String, Value>>, column: &str) -> Map<String, Value> {
    let mut map = Map::new();
    for row in rows.iter() {
        // we want to drop one key (column), but remove does not preserve order
        // https://github.com/serde-rs/json/issues/807
        let mut r = Map::new();
        let mut key = String::from("");
        for (k, v) in row.iter() {
            if k == column {
                key = v.as_str().unwrap().to_string();
            } else {
                r.insert(k.to_string(), v.clone());
            }
        }
        map.insert(key, Value::Object(r));
    }
    map
}