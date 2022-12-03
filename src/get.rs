use minijinja::Environment;
use serde_json::{from_str, json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

pub async fn table(table: String) -> String {
    let rows = get_table_from_database(".nanobot.db", "table").await;
    let data: Value = json!({
        "page": {
            "title": table
        },
        "column": {},
        "row": rows[0] // why is this necessary?
    });

    let mut env = Environment::new();
    env.add_template("debug.html", include_str!("resources/debug.html"))
        .unwrap();
    env.add_template("page.html", include_str!("resources/page.html"))
        .unwrap();
    env.add_template("table.html", include_str!("resources/table.html"))
        .unwrap();

    //let data: Value = from_str(include_str!("resources/page.json")).unwrap();
    let title: &str = data
        .get("page")
        .and_then(|value| value.get("title"))
        .and_then(|value| value.as_str())
        .unwrap();
    tracing::info!("format: {:?}", title);
    let template = env.get_template("table.html").unwrap();
    template.render(data).unwrap()
}

async fn get_table_from_database(database: &str, table: &str) -> Vec<Value> {
    let connection_string = format!("sqlite://{}?mode=rwc", database);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    // let query_string = format!("SELECT * FROM '{}'", &table);
    // let query_string = format!("SELECT * FROM '{}'", &table);
    let query_string = r#"SELECT json_group_array( 
        json_object(
            'table', "table",
            'path', "path",
            'type', "type",
            'description', "description" 
        )
    ) AS json_result
    FROM "table";"#;
    let rows: Vec<SqliteRow> = sqlx::query(&query_string).fetch_all(&pool).await.unwrap();
    let mut results: Vec<Value> = vec![];
    for row in rows.iter() {
        let result: &str = row.get("json_result");
        let json_result: Value = from_str(&result).unwrap();
        results.push(json_result);
    }
    results
}
