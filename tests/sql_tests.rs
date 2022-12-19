use nanobot::sql::{parse, select_to_sql, select_to_url, Direction, Operator, Select};
use serde_json::{from_value, json};

const SQL_SMALL: &str = r#"SELECT json_object(
  'table', "table",
  'path', "path",
  'type', "type",
  'description', "description"
) AS json_result
FROM (
  SELECT *
  FROM "table"
)"#;
const URL_SMALL: &str = "table";

const SQL_BIG: &str = r#"SELECT json_object(
  'table', "table",
  'path', "path",
  'type', "type",
  'description', "description"
) AS json_result
FROM (
  SELECT *
  FROM "table"
  WHERE "table" = 'table'
    AND "type" IN (1,2,3)
  ORDER BY "path" DESC
  LIMIT 1
  OFFSET 1
)"#;
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
            ("table".to_string(), Operator::Equals, "table".to_string()),
            ("type".to_string(), Operator::In, "(1,2,3)".to_string()),
        ],
        order: vec![("path".to_string(), Direction::Descending)],
        limit: 1,
        offset: 1,
        message: "".to_string(),
    };
    assert_eq!(select_to_sql(&select), SQL_BIG);
}

#[test]
fn test_select_to_sql_json() {
    let select: Select = from_value(json!({
        "table": "table",
        "select": ["table", "path", "type", "description"],
        "filter": [
            ["table", "Equals", "table"],
            ["type", "In", "(1,2,3)"]
        ],
        "order": [("path", "Descending")],
        "limit": 1,
        "offset": 1,
        "message": ""
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
            ["table", "Equals", "table"],
            ["type", "In", "(1,2,3)"]
        ],
        "order": [("path", "Descending")],
        "limit": 1,
        "offset": 1,
        "message": ""
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

#[test]
fn test_parse() {
    let url = "table?foo=eq.bar".to_string();
    let select = parse(&url);
    assert_eq!(select_to_url(&select), url);
}

#[test]
fn test_parse_message() {
    let url = "table?message=any".to_string();
    let select = parse(&url);
    assert_eq!(select.message, "any");
}
