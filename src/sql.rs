use enquote::unquote;
use ontodev_sqlrest::{Filter, Select};
use serde_json::{from_str, json, Map, Value};
use sqlx::any::AnyPool;
use sqlx::Row;
use std::collections::HashMap;

pub const LIMIT_MAX: usize = 100;
pub const LIMIT_DEFAULT: usize = 20; // TODO: 100?

pub async fn get_table_from_pool(
    pool: &AnyPool,
    select: &Select,
) -> Result<Vec<Map<String, Value>>, String> {
    let mut select = select.clone();
    // Order by row_number/row by default
    let default_order_by;
    if unquote(&select.table).unwrap_or(select.table.to_string()) == "message" {
        default_order_by = "row";
    } else {
        default_order_by = "row_number";
    }
    if select.order_by.len() == 0 {
        select.order_by(vec![default_order_by]);
    }

    // For basic queries, use row_number/row instead of offset
    if select.filter.len() == 0 {
        match select.offset {
            Some(offset) if offset > 0 => {
                select
                    .filter(vec![Filter::new(default_order_by, "ge", json!(offset)).unwrap()])
                    .offset(0);
            }
            _ => (),
        };
    }

    select.fetch_rows_as_json(pool, &HashMap::new())
}

pub async fn get_count_from_pool(pool: &AnyPool, select: &Select) -> Result<usize, sqlx::Error> {
    let sql = select.to_sqlite_count().unwrap();
    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let value_count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();

    let unquoted_table = unquote(&select.table).unwrap_or(select.table.to_string());
    let conflict_select =
        Select { table: format!("\"{}_conflict\"", unquoted_table), ..select.clone() };
    let sql = conflict_select.to_sqlite_count().unwrap();
    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let conflict_count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();
    Ok(value_count + conflict_count)
}

pub async fn get_total_from_pool(pool: &AnyPool, table: &String) -> Result<usize, sqlx::Error> {
    let unquoted_table = unquote(&table).unwrap_or(table.to_string());
    let select = Select::new(format!("\"{}\"", unquoted_table));
    let sql = select.to_sqlite_count().unwrap();
    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let value_count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();

    let select = Select::new(format!("\"{}_conflict\"", unquoted_table));
    let sql = select.to_sqlite_count().unwrap();
    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let conflict_count: usize = usize::try_from(row.get::<i64, &str>("count")).unwrap();

    Ok(value_count + conflict_count)
}

pub async fn get_message_counts_from_pool(
    pool: &AnyPool,
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
