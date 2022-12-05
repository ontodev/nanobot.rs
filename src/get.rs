use minijinja::Environment;
use serde_json::{from_str, json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::Row;

pub async fn table(table: String) -> Result<String, sqlx::Error> {
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

    let columns = vec!["table", "path", "type", "description"];
    let filter = None;
    let table_rows = get_table_from_pool(&pool, "table", columns, filter).await?;
    let table_map = rows_to_map(table_rows, "table");

    let columns = vec!["column", "nulltype", "datatype", "structure", "description"];
    let f = format!(r#"WHERE "table" = '{}'"#, table);
    let filter = Some(f.as_str());
    let column_rows = get_table_from_pool(&pool, "column", columns, filter).await?;
    let column_map = rows_to_map(column_rows, "column");

    // TODO: collect and fetch datatypes

    let columns = column_map.keys().map(|k| k.as_str()).collect();
    let filter = None;
    let rows = get_table_from_pool(&pool, &table, columns, filter).await?;

    // TODO: get the nulltypes
    // TODO: get the messages
    // TODO: merge into cells

    let data: Value = json!({
        "page": {
            "title": table
        },
        "table": table_map,
        "column": column_map,
        "row": rows
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
    table: &str,
    columns: Vec<&str>,
    filter: Option<&str>,
) -> Result<Vec<Value>, sqlx::Error> {
    // Build a SQLite query string that returns JSON, like:
    //     SELECT json_object(
    //         'table', "table",
    //         'path', "path",
    //         'type', "type",
    //         'description', "description"
    //     ) AS json_result
    //     FROM "table";
    let mut query = vec!["SELECT json_object(".to_string()];
    for t in &columns {
        let mut x = ",";
        if t == columns.last().unwrap() {
            x = ""
        }
        query.push(format!(r#"  '{}', "{}"{}"#, t, t, x));
    }
    query.push(") AS json_result".to_string());
    query.push(format!(r#"FROM "{}""#, table));
    if let Some(filter) = filter {
        query.push(filter.to_string());
    }
    let query_string = query.join("\n");
    let rows: Vec<SqliteRow> = sqlx::query(&query_string).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| {
            let result: &str = row.get("json_result");
            from_str(&result).unwrap()
        })
        .collect())
}

fn rows_to_map(rows: Vec<Value>, column: &str) -> Map<String, Value> {
    let mut map = Map::new();
    for row in rows.iter() {
        // we want to drop one key (column), but remove does not preserve order
        // https://github.com/serde-rs/json/issues/807
        let mut r = Map::new();
        let mut key = String::from("");
        for (k, v) in row.as_object().unwrap().iter() {
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
