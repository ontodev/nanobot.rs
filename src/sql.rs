use serde::{Deserialize, Serialize};
use serde_json::{from_str, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use tree_sitter::{Node, Parser};

pub const LIMIT_MAX: usize = 100;
pub const LIMIT_DEFAULT: usize = 10; // TODO: 100?

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum Operator {
    EQUALS,
    NOT_EQUALS,
    LESS,
    GREATER,
    LESS_EQUALS,
    GREATER_EQUALS,
    LIKE,
    ILIKE,
    IS,
    IN,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum Direction {
    ASC,
    DESC,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Select {
    pub table: String,
    pub select: Vec<String>,
    pub filter: Vec<(String, Operator, Value)>,
    pub order: Vec<(String, Direction)>,
    pub limit: usize,
    pub offset: usize,
}

/// Convert a Select struct to a SQL string.
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
pub fn select_to_sql(s: &Select) -> String {
    let mut lines: Vec<String> = vec!["SELECT json_object(".to_string()];
    let parts: Vec<String> = s
        .select
        .iter()
        .map(|c| format!(r#"'{}', "{}""#, c, c))
        .collect();
    lines.push(format!("  {}", parts.join(",\n  ")));
    lines.push(") AS json_result".to_string());
    lines.push(format!(r#"FROM "{}""#, s.table));
    let mut filters: Vec<String> = vec![];
    if s.filter.len() > 0 {
        for filter in &s.filter {
            filters.push(format!(
                r#""{}" = '{}'"#,
                filter.0,
                filter.2.as_str().unwrap().to_string()
            ));
        }
        lines.push(format!("WHERE {}", filters.join("\n  AND ")));
    }
    if s.order.len() > 0 {
        let parts: Vec<String> = s
            .order
            .iter()
            .map(|(c, d)| format!(r#""{}" {:?}"#, c, d))
            .collect();
        lines.push(format!("ORDER BY {}", parts.join(", ")));
    }
    if s.limit > 0 {
        lines.push(format!("LIMIT {}", s.limit));
    }
    if s.offset > 0 {
        lines.push(format!("OFFSET {}", s.offset));
    }
    lines.join("\n")
}

// TODO: remove duplicate code
pub fn select_to_sql_count(s: &Select) -> String {
    let mut lines: Vec<String> = vec!["SELECT COUNT(*) AS count".to_string()];
    lines.push(format!(r#"FROM "{}""#, s.table));
    let mut filters: Vec<String> = vec![];
    if s.filter.len() > 0 {
        for filter in &s.filter {
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

pub fn select_to_url(s: &Select) -> String {
    let mut params: Vec<String> = vec![];
    if s.filter.len() > 0 {
        for filter in &s.filter {
            params.push(format!(
                r#"{}=eq.{}"#,
                filter.0,
                filter.2.as_str().unwrap().to_string()
            ));
        }
    }
    if s.order.len() > 0 {
        let parts: Vec<String> = s
            .order
            .iter()
            .map(|(c, d)| format!(r#"{}.{}"#, c, format!("{:?}", d).to_lowercase()))
            .collect();
        params.push(format!("order={}", parts.join(", ")));
    }
    if s.limit > 0 && s.limit != LIMIT_DEFAULT {
        params.push(format!("limit={}", s.limit));
    }
    if s.offset > 0 {
        params.push(format!("offset={}", s.offset));
    }
    if params.len() > 0 {
        format!("{}?{}", s.table, params.join("&"))
    } else {
        s.table.clone()
    }
}

pub async fn get_table_from_pool(
    pool: &SqlitePool,
    select: &Select,
) -> Result<Vec<Map<String, Value>>, sqlx::Error> {
    let sql = select_to_sql(select);
    let rows: Vec<SqliteRow> = sqlx::query(&sql).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| {
            let result: &str = row.get("json_result");
            from_str::<Map<String, Value>>(&result).unwrap()
        })
        .collect())
}

pub async fn get_count_from_pool(pool: &SqlitePool, select: &Select) -> Result<usize, sqlx::Error> {
    let sql = select_to_sql_count(select);
    let row: SqliteRow = sqlx::query(&sql).fetch_one(pool).await?;
    let count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();
    Ok(count)
}

pub fn rows_to_map(rows: Vec<Map<String, Value>>, column: &str) -> Map<String, Value> {
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

pub fn parse(input: &str) -> Select {
    let mut parser = Parser::new();

    parser
        .set_language(tree_sitter_sqlrest::language())
        .expect("Error loading sqlrest grammar");

    let tree = parser.parse(input, None).unwrap();

    let mut query = Select {
        table: String::from("no table given"),
        select: Vec::new(),
        filter: Vec::new(),
        order: Vec::new(),
        limit: 0,
        offset: 0,
    };

    transduce(&tree.root_node(), input, &mut query);
    query
}

pub fn transduce(n: &Node, raw: &str, query: &mut Select) {
    match n.kind() {
        "query" => transduce_children(n, raw, query),
        "select" => transduce_select(n, raw, query),
        "table" => transduce_table(n, raw, query),
        "expression" => transduce_children(n, raw, query),
        "part" => transduce_children(n, raw, query),
        "filter" => transduce_children(n, raw, query),
        "simple_filter" => transduce_filter(n, raw, query),
        "special_filter" => transduce_children(n, raw, query),
        "in" => transduce_in(n, raw, query),
        "order" => transduce_order(n, raw, query),
        "limit" => transduce_limit(n, raw, query),
        "offset" => transduce_offset(n, raw, query),
        "STRING" => panic!("Encountered STRING in top level translation"),
        _ => {
            panic!("Parsing Error");
        }
    }
}

pub fn transduce_in(n: &Node, raw: &str, query: &mut Select) {
    let column = get_from_raw(&n.named_child(0).unwrap(), raw);
    let value = transduce_list(&n.named_child(1).unwrap(), raw);

    let filter = (column, Operator::IN , value);
    query.filter.push(filter); 
}

pub fn transduce_list(n: &Node, raw: &str) -> Value {

    let quoted_strings = match n.kind() {
        "list" => false,
        "list_of_strings" => true,
        _ => panic!("Not a valid list")
    };

    let mut vec = Vec::new();

    let child_count = n.named_child_count();
    for i in 0..child_count {
        if quoted_strings {
            let quoted_string = format!("{}", get_from_raw(&n.named_child(i).unwrap(), raw)); 
             vec.push(Value::String(quoted_string));
        } else {
             vec.push(Value::String(get_from_raw(&n.named_child(i).unwrap(), raw))); 
        } 
    };
    Value::Array(vec)
}

pub fn transduce_table(n: &Node, raw: &str, query: &mut Select) {
    let table = get_from_raw(&n.named_child(0).unwrap(), raw);
    query.table = table;
}

pub fn transduce_offset(n: &Node, raw: &str, query: &mut Select) {
    let offset_string = get_from_raw(&n.named_child(0).unwrap(), raw);
    let offset: usize = offset_string.parse().unwrap();
    query.offset = offset;
}

pub fn transduce_limit(n: &Node, raw: &str, query: &mut Select) {
    let limit_string = get_from_raw(&n.named_child(0).unwrap(), raw);
    let limit: usize = limit_string.parse().unwrap();
    query.limit = limit;
}

fn get_operator(operator_string: &str) -> Operator {
    match operator_string {
        "lt." => Operator::LESS,
        "lte." => Operator::LESS_EQUALS,
        "eq." => Operator::EQUALS,
        "neq." => Operator::NOT_EQUALS,
        "gt." => Operator::GREATER,
        "gte." => Operator::GREATER_EQUALS,
        "is." => Operator::IS,
        "like." => Operator::LIKE,
        "ilike." => Operator::ILIKE,
        "in." => Operator::IN,
        _ => panic!("Operator {} not supported", operator_string),
    }
}

pub fn transduce_filter(n: &Node, raw: &str, query: &mut Select) {
    let column = get_from_raw(&n.named_child(0).unwrap(), raw);
    let operator_string = get_from_raw(&n.named_child(1).unwrap(), raw);
    let value = get_from_raw(&n.named_child(2).unwrap(), raw);

    let operator = get_operator(&operator_string);

    let filter = (column, operator, Value::String(value));
    query.filter.push(filter);
}

fn get_ordering(ordering_string: &str) -> Direction {
    match ordering_string {
        ".asc" => Direction::ASC,
        ".desc" => Direction::DESC,
        _ => panic!("Ordering {} not supported", ordering_string),
    }
}

pub fn transduce_order(n: &Node, raw: &str, query: &mut Select) {
    let child_count = n.named_child_count();
    let mut position = 0;

    while position < child_count {
        let column = get_from_raw(&n.named_child(position).unwrap(), raw);
        position = position + 1;
        if position < child_count && n.named_child(position).unwrap().kind().eq("ordering") {
            let ordering_string = get_from_raw(&n.named_child(position).unwrap(), raw);
            let ordering = get_ordering(&ordering_string);
            position = position + 1;
            let order = (column, ordering);
            query.order.push(order);
        } else {
            let ordering = Direction::ASC; //default ordering is ASC
            let order = (column, ordering);
            query.order.push(order);
        }
    }
}

pub fn transduce_select(n: &Node, raw: &str, query: &mut Select) {
    let child_count = n.named_child_count();
    for position in 0..child_count {
        let column = get_from_raw(&n.named_child(position).unwrap(), raw);
        query.select.push(column);
    }
}

pub fn get_from_raw(n: &Node, raw: &str) -> String {
    let start = n.start_position().column;
    let end = n.end_position().column;
    let extract = &raw[start..end];
    String::from(extract)
}

pub fn transduce_children(n: &Node, raw: &str, q: &mut Select) {
    let child_count = n.named_child_count();
    for i in 0..child_count {
        transduce(&n.named_child(i).unwrap(), raw, q);
    }
}
