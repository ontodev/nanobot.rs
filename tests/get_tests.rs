use nanobot::get::{query_to_sql, Direction, Operator, Query};
use serde_json::{from_value, json, Value};

const SQL_SMALL: &str = r#"SELECT json_object(
  'table', "table",
  'path', "path",
  'type', "type",
  'description', "description"
) AS json_result
FROM "table""#;
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

#[test]
fn test_query() {
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
fn test_query_json() {
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
fn test_query_default() {
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
