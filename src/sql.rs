use serde::{Deserialize, Serialize};
use serde_json::{from_str, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;

pub const LIMIT_MAX: usize = 100;
pub const LIMIT_DEFAULT: usize = 10; // TODO: 100?

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub enum Operator {
    EQUALS,
    LT,
    GT,
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

fn filter_to_sql(filter: &(String, Operator, Value)) -> String {
    match filter.1 {
        Operator::EQUALS => format!(
            r#""{}" = '{}'"#,
            filter.0,
            filter.2.as_str().unwrap().to_string()
        ),
        Operator::LT => format!(
            r#""{}" < {}"#,
            filter.0,
            filter.2.as_u64().unwrap().to_string()
        ),
        Operator::GT => format!(
            r#""{}" > {}"#,
            filter.0,
            filter.2.as_u64().unwrap().to_string()
        ),
        Operator::IN => format!(
            r#""{}" IN ({})"#,
            filter.0,
            // WARN: This is not a good idea!
            filter
                .2
                .to_string()
                .trim_start_matches("[")
                .trim_end_matches("]")
        ),
    }
}

fn filters_to_sql(filters: &Vec<(String, Operator, Value)>) -> String {
    let mut parts: Vec<String> = vec![];
    for filter in filters {
        parts.push(filter_to_sql(&filter));
    }
    format!("WHERE {}", parts.join("\n  AND "))
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
    if s.filter.len() > 0 {
        lines.push(filters_to_sql(&s.filter));
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

pub fn select_to_sql_count(s: &Select) -> String {
    let mut lines: Vec<String> = vec!["SELECT COUNT() AS count".to_string()];
    lines.push(format!(r#"FROM "{}""#, s.table));
    if s.filter.len() > 0 {
        lines.push(filters_to_sql(&s.filter));
    }
    lines.join("\n")
}

pub fn select_to_url(s: &Select) -> String {
    let mut params: Vec<String> = vec![];
    if s.filter.len() > 0 {
        for filter in &s.filter {
            let x = match filter.1 {
                Operator::EQUALS => format!(
                    r#"{}=eq.{}"#,
                    filter.0,
                    filter.2.as_str().unwrap().to_string()
                ),
                Operator::LT => format!(
                    r#"{}=lt.{}"#,
                    filter.0,
                    filter.2.as_u64().unwrap().to_string()
                ),
                Operator::GT => format!(
                    r#"{}=gt.{}"#,
                    filter.0,
                    filter.2.as_u64().unwrap().to_string()
                ),
                Operator::IN => format!(
                    r#"{}=in.({})"#,
                    filter.0,
                    // WARN: This is not a good idea!
                    filter
                        .2
                        .to_string()
                        .trim_start_matches("[")
                        .trim_end_matches("]")
                ),
            };
            params.push(x);
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
    let mut new_select = select.clone();

    // Order by row_number by default
    if select.order.len() == 0 {
        new_select = Select {
            order: vec![("row_number".to_string(), Direction::ASC)],
            ..select.clone()
        };
    }

    // For basic queries, use row_number instead of offset
    if select.filter.len() == 0 && select.offset > 0 {
        new_select = Select {
            filter: vec![(
                "row_number".to_string(),
                Operator::GT,
                serde_json::json!(select.offset),
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

pub async fn get_count_from_pool(pool: &SqlitePool, select: &Select) -> Result<usize, sqlx::Error> {
    let sql = select_to_sql_count(select);
    let row: SqliteRow = sqlx::query(&sql).fetch_one(pool).await?;
    let count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();
    Ok(count)
}

pub async fn get_total_from_pool(pool: &SqlitePool, table: &String) -> Result<usize, sqlx::Error> {
    let sql = format!(r#"SELECT COUNT() AS count FROM "{}""#, table);
    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();
    Ok(count)
}

pub async fn get_message_counts_from_pool(
    pool: &SqlitePool,
    table: &String,
) -> Result<Map<String, Value>, sqlx::Error> {
    let sql = format!(
        r#"SELECT json_object(
          'message', COUNT(),
          'error', SUM(level = 'error'),
          'warn', SUM(level = 'warn'),
          'info', SUM(level = 'info') 
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
