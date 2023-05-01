use enquote::unquote;
use ontodev_sqlrest::{get_db_type, Filter, Select};
use serde_json::{from_str, json, Map, Value};
use sqlx::any::{AnyKind, AnyPool};
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
                let filter = match Filter::new(default_order_by, "ge", json!(offset)) {
                    Err(e) => return Err(e),
                    Ok(f) => f,
                };
                select.add_filter(filter).offset(0);
            }
            _ => (),
        };
    }

    select.fetch_rows_as_json(pool, &HashMap::new())
}

pub async fn get_count_from_pool(pool: &AnyPool, select: &Select) -> Result<usize, sqlx::Error> {
    let db_type = match get_db_type(pool) {
        Ok(db_type) => db_type,
        Err(e) => return Err(sqlx::Error::Configuration(e.into())),
    };
    let sql = match select.to_sql_count(&db_type) {
        Ok(sql) => sql,
        Err(e) => return Err(sqlx::Error::Configuration(e.into())),
    };
    let row = match sqlx::query(&sql).fetch_one(pool).await {
        Ok(row) => row,
        Err(e) => return Err(e),
    };
    let value_count: usize = match usize::try_from(row.get::<i64, &str>("count")) {
        Ok(count) => count,
        Err(e) => return Err(sqlx::Error::Decode(e.into())),
    };

    let unquoted_table = unquote(&select.table).unwrap_or(select.table.to_string());
    let conflict_select =
        Select { table: format!("\"{}_conflict\"", unquoted_table), ..select.clone() };
    let sql = match conflict_select.to_sql_count(&db_type) {
        Ok(sql) => sql,
        Err(e) => return Err(sqlx::Error::Configuration(e.into())),
    };
    let row = match sqlx::query(&sql).fetch_one(pool).await {
        Ok(row) => row,
        Err(e) => return Err(e),
    };
    let conflict_count: usize = match usize::try_from(row.get::<i64, &str>("count")) {
        Ok(count) => count,
        Err(e) => return Err(sqlx::Error::Decode(e.into())),
    };
    Ok(value_count + conflict_count)
}

pub async fn get_total_from_pool(pool: &AnyPool, table: &String) -> Result<usize, sqlx::Error> {
    let unquoted_table = unquote(&table).unwrap_or(table.to_string());

    let select = Select::new(format!("\"{}\"", unquoted_table));
    let db_type = match get_db_type(pool) {
        Ok(db_type) => db_type,
        Err(e) => return Err(sqlx::Error::Configuration(e.into())),
    };
    let sql = match select.to_sql_count(&db_type) {
        Ok(sql) => sql,
        Err(e) => return Err(sqlx::Error::Configuration(e.into())),
    };
    let row = match sqlx::query(&sql).fetch_one(pool).await {
        Ok(row) => row,
        Err(e) => return Err(e),
    };
    let value_count: usize = match usize::try_from(row.get::<i64, &str>("count")) {
        Ok(count) => count,
        Err(e) => return Err(sqlx::Error::Decode(e.into())),
    };

    let select = Select::new(format!("\"{}_conflict\"", unquoted_table));
    let sql = match select.to_sql_count(&db_type) {
        Ok(sql) => sql,
        Err(e) => return Err(sqlx::Error::Configuration(e.into())),
    };
    let row = match sqlx::query(&sql).fetch_one(pool).await {
        Ok(row) => row,
        Err(e) => return Err(e),
    };
    let conflict_count: usize = match usize::try_from(row.get::<i64, &str>("count")) {
        Ok(count) => count,
        Err(e) => return Err(sqlx::Error::Decode(e.into())),
    };

    Ok(value_count + conflict_count)
}

pub async fn get_message_counts_from_pool(
    pool: &AnyPool,
    table: &String,
) -> Result<Map<String, Value>, sqlx::Error> {
    let sql = {
        if pool.any_kind() == AnyKind::Sqlite {
            format!(
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
            )
        } else {
            format!(
                r#"SELECT JSON_ARRAY_ELEMENTS("json_agg")::TEXT AS "json_result"
                   FROM (
                       SELECT JSON_AGG(t1) AS "json_agg"
                       FROM (
                           SELECT
                               COUNT(1) AS "message",
                               COUNT(DISTINCT row) AS "message_row",
                               SUM((level = 'error')::INT) AS "error",
                               SUM((level = 'warn')::INT) AS "warn",
                               SUM((level = 'info')::INT) AS "info",
                               SUM((level = 'update')::INT) AS "update"
                           FROM "message"
                           WHERE "table" = '{}'
                       ) t1
                   ) t2"#,
                table
            )
        }
    };
    let row = match sqlx::query(&sql).fetch_one(pool).await {
        Ok(row) => row,
        Err(e) => return Err(e),
    };
    let result: &str = row.get("json_result");
    let map = match from_str::<Map<String, Value>>(&result) {
        Ok(m) => m,
        Err(e) => return Err(sqlx::Error::Decode(e.into())),
    };
    Ok(map)
}

pub fn rows_to_map(
    rows: Vec<Map<String, Value>>,
    column: &str,
) -> Result<Map<String, Value>, String> {
    let mut map = Map::new();
    for row in rows.iter() {
        // we want to drop one key (column), but remove does not preserve order
        // https://github.com/serde-rs/json/issues/807
        let mut r = Map::new();
        let mut key = String::from("");
        for (k, v) in row.iter() {
            if k == column {
                key = match v.as_str() {
                    Some(k) => k.to_string(),
                    None => return Err(format!("Unable to convert '{}' to str", v)),
                };
            } else {
                r.insert(k.to_string(), v.clone());
            }
        }
        map.insert(key, Value::Object(r));
    }
    Ok(map)
}
