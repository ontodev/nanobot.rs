use csv::WriterBuilder;
use enquote::unquote;
use futures::TryStreamExt;
use ontodev_sqlrest::{get_db_type, Filter, Select};
use serde_json::{from_str, json, Map, Value};
use sqlx::any::{AnyKind, AnyPool};
use sqlx::Row;
use std::collections::HashMap;
use std::error::Error;

pub const LIMIT_MAX: usize = 10000;

pub async fn save_table(
    pool: &AnyPool,
    table: &str,
    columns: &Vec<&str>,
    path: &str,
) -> Result<(), Box<dyn Error>> {
    let quoted_columns = columns.iter().map(|v| enquote::enquote('"', v)).collect();
    let text_view = format!("\"{table}_text_view\"");
    let mut select = Select::new(text_view);
    select.select(quoted_columns);
    select.order_by(vec!["row_number"]);

    // let path = format!("build/{path}");
    // tracing::debug!("SAVE to {path} using {select:?}");

    let dbtype = get_db_type(&pool).unwrap();
    let sql = select.to_sql(&dbtype).unwrap();
    // tracing::debug!("SQL {sql}");

    let mut writer = WriterBuilder::new().delimiter(b'\t').from_path(path)?;
    writer.write_record(columns)?;
    let mut stream = sqlx::query(&sql).fetch(pool);
    while let Some(row) = stream.try_next().await? {
        // tracing::debug!("Some Result");
        let mut record: Vec<&str> = vec![];
        for column in columns.iter() {
            let cell = row.try_get::<&str, &str>(column).ok().unwrap_or_default();
            record.push(cell);
        }
        writer.write_record(record)?;
    }
    writer.flush()?;
    Ok(())
}

pub async fn get_table_from_pool(
    pool: &AnyPool,
    select: &Select,
) -> Result<Vec<Map<String, Value>>, String> {
    let mut select = select.clone();
    // Order by row_number/row by default
    let default_order_by;
    if unquote(&select.table).unwrap_or(select.table.to_string()) == "message" {
        default_order_by = "message_id";
    } else {
        default_order_by = "row_number";
    }
    if select.order_by.len() == 0 {
        select.order_by(vec![default_order_by]);
    }

    // For basic queries, use row_number/message_id instead of offset
    if select.filter.len() == 0 {
        match select.offset {
            Some(offset) if offset > 0 => {
                let filter = match Filter::new(default_order_by, "gt", json!(offset)) {
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
    let conflict_count = {
        if unquoted_table != "message" {
            let conflict_select = Select {
                table: format!("\"{}_conflict\"", unquoted_table),
                ..select.clone()
            };
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
            conflict_count
        } else {
            0
        }
    };
    Ok(value_count + conflict_count)
}

pub async fn get_total_from_pool(pool: &AnyPool, table: &String) -> Result<usize, sqlx::Error> {
    let unquoted_table = unquote(&table).unwrap_or(table.to_string());
    let select = Select::new(format!("\"{}\"", unquoted_table));
    get_count_from_pool(pool, &select).await
}

pub async fn get_message_counts_from_pool(
    pool: &AnyPool,
    table: &String,
) -> Result<Map<String, Value>, sqlx::Error> {
    if table == "message" {
        Ok(json!({
            "message": 0,
            "message_row": 0,
            "error": 0,
            "warn": 0,
            "info": 0,
            "update": 0,
        })
        .as_object()
        .unwrap()
        .clone())
    } else {
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
