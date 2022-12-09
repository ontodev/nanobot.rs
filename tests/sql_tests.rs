use nanobot::sql::{query_to_sql, query_to_url, Direction, Operator, Query};
use serde_json::{from_value, json, Value};

const SQL_SMALL: &str = r#"SELECT json_object(
  'table', "table",
  'path', "path",
  'type', "type",
  'description', "description"
) AS json_result
FROM "table""#;
const URL_SMALL: &str = "table";

const SQL_BIG: &str = r#"SELECT json_object(
  'table', "table",
  'path', "path",
  'type', "type",
  'description', "description"
) AS json_result
FROM "table"
WHERE "table" = 'table'
  AND "type" = 'table'
ORDER BY "path" DESC
LIMIT 1
OFFSET 1"#;
const URL_BIG: &str = "table?table=eq.table&type=eq.table&order=path.desc&limit=1&offset=1";

#[test]
fn test_query_to_sql() {
    let query = Query {
        table: "table".to_string(),
        select: ["table", "path", "type", "description"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        filter: vec![
            (
                "table".to_string(),
                Operator::EQUALS,
                Value::String("table".to_string()),
            ),
            (
                "type".to_string(),
                Operator::EQUALS,
                Value::String("table".to_string()),
            ),
        ],
        order: vec![("path".to_string(), Direction::DESC)],
        limit: 1,
        offset: 1,
    };
    assert_eq!(query_to_sql(&query), SQL_BIG);
}

#[test]
fn test_query_to_sql_json() {
    let query: Query = from_value(json!({
        "table": "table",
        "select": ["table", "path", "type", "description"],
        "filter": [
            ["table", "EQUALS", "table"],
            ["type", "EQUALS", "table"]
        ],
        "order": [("path", "DESC")],
        "limit": 1,
        "offset": 1
    }))
    .unwrap();
    assert_eq!(query_to_sql(&query), SQL_BIG);
}

#[test]
fn test_query_to_sql_default() {
    let tables = ["table", "path", "type", "description"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let query = Query {
        table: "table".to_string(),
        select: tables,
        ..Default::default()
    };
    assert_eq!(query_to_sql(&query), SQL_SMALL);
}

#[test]
fn test_query_to_url() {
    let query: Query = from_value(json!({
        "table": "table",
        "select": ["table", "path", "type", "description"],
        "filter": [
            ["table", "EQUALS", "table"],
            ["type", "EQUALS", "table"]
        ],
        "order": [("path", "DESC")],
        "limit": 1,
        "offset": 1
    }))
    .unwrap();
    assert_eq!(query_to_url(&query), URL_BIG);
}

#[test]
fn test_query_to_url_default() {
    let tables = ["table", "path", "type", "description"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let query = Query {
        table: "table".to_string(),
        select: tables,
        ..Default::default()
    };
    assert_eq!(query_to_url(&query), URL_SMALL);
}
