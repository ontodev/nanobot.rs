use nanobot::sql::{select_to_sql, select_to_url, Direction, Operator, Select};
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
  AND "type" IN (1,2,3)
ORDER BY "path" DESC
LIMIT 1
OFFSET 1"#;
const URL_BIG: &str = "table?table=eq.table&type=in.(1,2,3)&order=path.desc&limit=1&offset=1";

#[test]
fn test_select_to_sql() {
    let select = Select {
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
            ("type".to_string(), Operator::IN, json!([1, 2, 3])),
        ],
        order: vec![("path".to_string(), Direction::DESC)],
        limit: 1,
        offset: 1,
    };
    assert_eq!(select_to_sql(&select), SQL_BIG);
}

#[test]
fn test_select_to_sql_json() {
    let select: Select = from_value(json!({
        "table": "table",
        "select": ["table", "path", "type", "description"],
        "filter": [
            ["table", "EQUALS", "table"],
            ["type", "IN", [1, 2, 3]]
        ],
        "order": [("path", "DESC")],
        "limit": 1,
        "offset": 1
    }))
    .unwrap();
    assert_eq!(select_to_sql(&select), SQL_BIG);
}

#[test]
fn test_select_to_sql_default() {
    let tables = ["table", "path", "type", "description"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let select = Select {
        table: "table".to_string(),
        select: tables,
        ..Default::default()
    };
    assert_eq!(select_to_sql(&select), SQL_SMALL);
}

#[test]
fn test_select_to_url() {
    let select: Select = from_value(json!({
        "table": "table",
        "select": ["table", "path", "type", "description"],
        "filter": [
            ["table", "EQUALS", "table"],
            ["type", "IN", [1, 2, 3]]
        ],
        "order": [("path", "DESC")],
        "limit": 1,
        "offset": 1
    }))
    .unwrap();
    assert_eq!(select_to_url(&select), URL_BIG);
}

#[test]
fn test_select_to_url_default() {
    let tables = ["table", "path", "type", "description"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let select = Select {
        table: "table".to_string(),
        select: tables,
        ..Default::default()
    };
    assert_eq!(select_to_url(&select), URL_SMALL);
}
