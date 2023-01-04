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
fn test_select_new() {
    let select_new = Select::new();
    let expected = Select {
        ..Default::default()
    };
    assert_eq!(select_new, expected);
}

#[test]
fn test_select_clone() {
    let select_new = Select::new();
    let clone = select_new.clone();
    assert_eq!(select_new, clone);
}

#[test]
fn test_select_table() {
    let mut select = Select::new();
    select.table("table");
    let expected = Select {
        table: String::from("table"),
        ..Default::default()
    };

    assert_eq!(select, expected);
}

#[test]
fn test_select_select() {
    let mut select = Select::new();
    select.select(vec!["v1", "v2", "v3"]);
    let expected = Select {
        select: vec![String::from("v1"), String::from("v2"), String::from("v3")],
        ..Default::default()
    };

    assert_eq!(select, expected);
}

#[test]
fn test_select_filter() {
    let mut select = Select::new();
    select.filter(vec![
        ("a1", Operator::Equals, "b1"),
        ("a2", Operator::NotEquals, "b2"),
    ]);
    let expected = Select {
        filter: vec![
            (String::from("a1"), Operator::Equals, String::from("b1")),
            (String::from("a2"), Operator::NotEquals, String::from("b2")),
        ],
        ..Default::default()
    };

    assert_eq!(select, expected);
}

#[test]
fn test_select_order() {
    let mut select = Select::new();
    select.order(vec![
        ("v1", Direction::Ascending),
        ("v2", Direction::Descending),
    ]);
    let expected = Select {
        order: vec![
            (String::from("v1"), Direction::Ascending),
            (String::from("v2"), Direction::Descending),
        ],
        ..Default::default()
    };

    assert_eq!(select, expected);
}

#[test]
fn test_select_limit() {
    let mut select = Select::new();
    select.limit(1);
    let expected = Select {
        limit: 1,
        ..Default::default()
    };

    assert_eq!(select, expected);
}

#[test]
fn test_select_offset() {
    let mut select = Select::new();
    select.offset(1);
    let expected = Select {
        offset: 1,
        ..Default::default()
    };

    assert_eq!(select, expected);
}

#[test]
fn test_select_message() {
    let mut select = Select::new();
    select.message("message");
    let expected = Select {
        message: String::from("message"),
        ..Default::default()
    };

    assert_eq!(select, expected);
}

#[test]
fn test_select_all() {
    let mut select = Select::new();
    select
        .table("table")
        .select(vec!["v1", "v2", "v3"])
        .filter(vec![
            ("a1", Operator::Equals, "b1"),
            ("a2", Operator::NotEquals, "b2"),
        ])
        .order(vec![
            ("v1", Direction::Ascending),
            ("v2", Direction::Descending),
        ])
        .limit(1)
        .offset(1)
        .message("message");

    let expected = Select {
        table: String::from("table"),
        select: vec![String::from("v1"), String::from("v2"), String::from("v3")],
        filter: vec![
            (String::from("a1"), Operator::Equals, String::from("b1")),
            (String::from("a2"), Operator::NotEquals, String::from("b2")),
        ],
        order: vec![
            (String::from("v1"), Direction::Ascending),
            (String::from("v2"), Direction::Descending),
        ],
        limit: 1,
        offset: 1,
        message: String::from("message"),
    };

    assert_eq!(select, expected);
}

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
