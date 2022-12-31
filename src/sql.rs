use serde::{Deserialize, Serialize};
use serde_json::{from_str, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use std::str::FromStr;
use tree_sitter::{Node, Parser};
use urlencoding::decode;

pub const LIMIT_MAX: usize = 100;
pub const LIMIT_DEFAULT: usize = 20; // TODO: 100?

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum Operator {
    Equals,
    NotEquals,
    LessThan,
    GreaterThan,
    LessThanEquals,
    GreaterThanEquals,
    Like,
    ILike,
    Is,
    In,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseOperatorError;

impl FromStr for Operator {
    type Err = ParseOperatorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "lt" => Ok(Operator::LessThan),
            "lte" => Ok(Operator::LessThanEquals),
            "eq" => Ok(Operator::Equals),
            "neq" => Ok(Operator::NotEquals),
            "gt" => Ok(Operator::GreaterThan),
            "gte" => Ok(Operator::GreaterThanEquals),
            "is" => Ok(Operator::Is),
            "like" => Ok(Operator::Like),
            "ilike" => Ok(Operator::ILike),
            "in" => Ok(Operator::In),
            _ => Err(ParseOperatorError),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum Direction {
    Ascending,
    Descending,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseDirectionError;

impl FromStr for Direction {
    type Err = ParseDirectionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "asc" | "ascending" => Ok(Direction::Ascending),
            "desc" | "descending" => Ok(Direction::Descending),
            _ => Err(ParseDirectionError),
        }
    }
}

impl Direction {
    fn to_sql(&self) -> &str {
        match self {
            Direction::Ascending => "ASC",
            Direction::Descending => "DESC",
        }
    }
    fn to_url(&self) -> &str {
        match self {
            Direction::Ascending => "asc",
            Direction::Descending => "desc",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Select {
    pub table: String,
    pub select: Vec<String>,
    pub filter: Vec<(String, Operator, String)>,
    pub order: Vec<(String, Direction)>,
    pub limit: usize,
    pub offset: usize,
    pub message: String,
}

fn filter_to_sql(filter: &(String, Operator, String)) -> String {
    let keywords = vec!["true".to_string(), "false".to_string(), "null".to_string()];
    let value = if keywords.contains(&filter.2) {
        filter.2.clone()
    } else if filter.2.parse::<i64>().is_ok() {
        filter.2.clone()
    } else if filter.2.parse::<f64>().is_ok() {
        filter.2.clone()
    } else if filter.2.starts_with("(") && filter.2.ends_with(")") {
        // WARN: This is not safe.
        filter
            .2
            .clone()
            .trim_start_matches("(")
            .trim_end_matches(")")
            .to_string()
    } else {
        // WARN: This is not handling single quotes propertly.
        format!("'{}'", filter.2.replace("'", ""))
    };
    match filter.1 {
        Operator::Equals => format!(r#""{}" = {}"#, filter.0, value),
        Operator::LessThan => format!(r#""{}" < {}"#, filter.0, value),
        Operator::GreaterThan => format!(r#""{}" > {}"#, filter.0, value),
        // WARN: This is not handling lists properly.
        Operator::In => format!(r#""{}" IN ({})"#, filter.0, value),
        _ => todo!(),
    }
}

fn filters_to_sql(indent: &str, filters: &Vec<(String, Operator, String)>) -> String {
    let mut parts: Vec<String> = vec![];
    for filter in filters {
        parts.push(filter_to_sql(&filter));
    }
    let joiner = format!("\n{}  AND ", indent);
    format!("{}WHERE {}", indent, parts.join(&joiner))
}

impl Select {
    pub fn new() -> Select {
        Default::default()
    }

    pub fn clone(select: &Select) -> Select {
        Select { ..select.clone() }
    }

    pub fn table<S: Into<String>>(&mut self, table: S) -> &mut Select {
        self.table = table.into();
        self
    }

    pub fn select<S: Into<String>>(&mut self, select: Vec<S>) -> &mut Select {
        for s in select {
            self.select.push(s.into());
        }
        self
    }

    pub fn filter<S: Into<String>>(&mut self, filter: Vec<(S, Operator, String)>) -> &mut Select {
        for (s, o, v) in filter {
            self.filter.push((s.into(), o, v));
        }
        self
    }

    pub fn order<S: Into<String>>(&mut self, order: Vec<(S, Direction)>) -> &mut Select {
        for (s, d) in order {
            self.order.push((s.into(), d));
        }
        self
    }

    pub fn limit(&mut self, limit: usize) -> &mut Select {
        self.limit = limit;
        self
    }

    pub fn offset(&mut self, offset: usize) -> &mut Select {
        self.offset = offset;
        self
    }
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
/// FROM (
///   SELECT *
///   FROM "table"
/// )
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
    lines.push("FROM (".to_string());
    lines.push("  SELECT *".to_string());
    lines.push(format!(r#"  FROM "{}""#, s.table));
    if s.filter.len() > 0 {
        lines.push(filters_to_sql("  ", &s.filter));
    }
    if s.order.len() > 0 {
        let parts: Vec<String> = s
            .order
            .iter()
            .map(|(c, d)| format!(r#""{}" {}"#, c, d.to_sql()))
            .collect();
        lines.push(format!("  ORDER BY {}", parts.join(", ")));
    }
    if s.limit > 0 {
        lines.push(format!("  LIMIT {}", s.limit));
    }
    if s.offset > 0 {
        lines.push(format!("  OFFSET {}", s.offset));
    }
    lines.push(")".to_string());
    lines.join("\n")
}

pub fn select_to_sql_count(s: &Select) -> String {
    let mut lines: Vec<String> = vec!["SELECT COUNT() AS count".to_string()];
    lines.push(format!(r#"FROM "{}""#, s.table));
    if s.filter.len() > 0 {
        lines.push(filters_to_sql("", &s.filter));
    }
    lines.join("\n")
}

pub fn select_to_url(s: &Select) -> String {
    let mut params: Vec<String> = vec![];
    if s.message != "" {
        params.push(format!("message={}", s.message));
    }
    if s.filter.len() > 0 {
        for filter in &s.filter {
            let x = match filter.1 {
                Operator::Equals => format!(r#"{}=eq.{}"#, filter.0, filter.2),
                Operator::LessThan => format!(r#"{}=lt.{}"#, filter.0, filter.2),
                Operator::GreaterThan => format!(r#"{}=gt.{}"#, filter.0, filter.2),
                Operator::In => format!(
                    r#"{}=in.({})"#,
                    filter.0,
                    // WARN: This is not a good idea!
                    filter.2.trim_start_matches("(").trim_end_matches(")")
                ),
                _ => todo!(),
            };
            params.push(x);
        }
    }
    if s.order.len() > 0 {
        let parts: Vec<String> = s
            .order
            .iter()
            .map(|(c, d)| format!(r"{}.{}", c, d.to_url()))
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
    let mut new_select = select.clone();

    // Order by row_number by default
    if select.order.len() == 0 {
        new_select = Select {
            order: vec![("row_number".to_string(), Direction::Ascending)],
            ..select.clone()
        };
    }

    // For basic queries, use row_number instead of offset
    if select.filter.len() == 0 && select.offset > 0 {
        new_select = Select {
            filter: vec![(
                "row_number".to_string(),
                Operator::GreaterThan,
                select.offset.to_string(),
            )],
            offset: 0,
            ..new_select.clone()
        };
    }

    let sql = select_to_sql(&new_select);
    let rows: Vec<SqliteRow> = sqlx::query(&sql).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| {
            let result: &str = row.get("json_result");
            from_str::<Map<String, Value>>(&result).unwrap()
        })
        .collect())
}

pub async fn get_rows_from_pool(
    pool: &SqlitePool,
    sql: &String,
) -> Result<Vec<Map<String, Value>>, sqlx::Error> {
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
    let value_count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();

    let conflict_select = Select {
        table: format!("{}_conflict", select.table.clone()),
        ..select.clone()
    };
    let sql = select_to_sql_count(&conflict_select);
    let row: SqliteRow = sqlx::query(&sql).fetch_one(pool).await?;
    let conflict_count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();
    Ok(value_count + conflict_count)
}

pub async fn get_total_from_pool(pool: &SqlitePool, table: &String) -> Result<usize, sqlx::Error> {
    let sql = format!(r#"SELECT COUNT() AS count FROM "{}""#, table);
    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let value_count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();

    let sql = format!(r#"SELECT COUNT() AS count FROM "{}_conflict""#, table);
    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let conflict_count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();
    Ok(value_count + conflict_count)
}

pub async fn get_message_counts_from_pool(
    pool: &SqlitePool,
    table: &String,
) -> Result<Map<String, Value>, sqlx::Error> {
    let sql = format!(
        r#"SELECT json_object(
          'message', COUNT(),
          'message_row', COUNT(DISTINCT row),
          'error', SUM(level = 'error'),
          'warn', SUM(level = 'warn'),
          'info', SUM(level = 'info'),
          'update', SUM(level = 'update')
        ) AS json_result
        FROM message
        WHERE "table" = '{}'"#,
        table
    );
    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let result: &str = row.get("json_result");
    let map = from_str::<Map<String, Value>>(&result).unwrap();
    Ok(map)
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
    // WARN: This is a hack to handle the special 'message' parameter.
    let mut message = "".to_string();
    let mut i = input.to_string();
    if i.contains("message=any") {
        message = "any".to_string();
        if i.contains("?message=any&") {
            i = i.replace("?message=any&", "?");
        } else if i.contains("?message=any") {
            i = i.replace("?message=any", "");
        } else if i.contains("&message=any") {
            i = i.replace("&message=any", "");
        }
    }

    let mut parser = Parser::new();

    parser
        .set_language(tree_sitter_sqlrest::language())
        .expect("Error loading sqlrest grammar");

    let tree = parser.parse(&i, None).unwrap();

    let mut query = Select {
        table: String::from("no table given"),
        select: Vec::new(),
        filter: Vec::new(),
        order: Vec::new(),
        limit: 0,
        offset: 0,
        message,
    };

    transduce(&tree.root_node(), &i, &mut query);
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
            panic!("Parsing Error: {:?} {} {:?}", n, raw, query);
        }
    }
}

pub fn transduce_in(n: &Node, raw: &str, query: &mut Select) {
    let column = decode(&get_from_raw(&n.named_child(0).unwrap(), raw))
        .unwrap()
        .into_owned();
    let value = transduce_list(&n.named_child(1).unwrap(), raw);

    let filter = (column, Operator::In, value);
    query.filter.push(filter);
}

pub fn transduce_list(n: &Node, raw: &str) -> String {
    let quoted_strings = match n.kind() {
        "list" => false,
        "list_of_strings" => true,
        _ => panic!("Not a valid list"),
    };

    let mut vec = Vec::new();

    let child_count = n.named_child_count();
    for i in 0..child_count {
        let value = decode(&get_from_raw(&n.named_child(i).unwrap(), raw))
            .unwrap()
            .into_owned();
        if quoted_strings {
            let quoted_string = format!("{}", value);
            vec.push(quoted_string);
        } else {
            vec.push(value);
        }
    }
    format!("({})", vec.join(","))
}

pub fn transduce_table(n: &Node, raw: &str, query: &mut Select) {
    let table = decode(&get_from_raw(&n.named_child(0).unwrap(), raw))
        .unwrap()
        .into_owned();
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

pub fn transduce_filter(n: &Node, raw: &str, query: &mut Select) {
    let column = decode(&get_from_raw(&n.named_child(0).unwrap(), raw))
        .unwrap()
        .into_owned();
    let operator_string = get_from_raw(&n.named_child(1).unwrap(), raw);
    let value = decode(&get_from_raw(&n.named_child(2).unwrap(), raw))
        .unwrap()
        .into_owned();

    let operator = Operator::from_str(&operator_string);
    match operator {
        Ok(o) => {
            let filter = (column, o, value);
            query.filter.push(filter);
        }
        Err(_) => {
            tracing::warn!("Unhandled operator '{}'", operator_string);
        }
    };
}

pub fn transduce_order(n: &Node, raw: &str, query: &mut Select) {
    let child_count = n.named_child_count();
    let mut position = 0;

    while position < child_count {
        let column = decode(&get_from_raw(&n.named_child(0).unwrap(), raw))
            .unwrap()
            .into_owned();
        position = position + 1;
        if position < child_count && n.named_child(position).unwrap().kind().eq("ordering") {
            let ordering_string = get_from_raw(&n.named_child(position).unwrap(), raw);
            let ordering = Direction::from_str(&ordering_string);
            match ordering {
                Ok(o) => {
                    position = position + 1;
                    let order = (column, o);
                    query.order.push(order);
                }
                Err(_) => {
                    tracing::warn!("Unhandled order param '{}'", ordering_string);
                }
            };
        } else {
            let ordering = Direction::Ascending; //default ordering is ASC
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
