use serde_json::{from_str, json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use wiring_rs::util::signature;

#[derive(Debug)]
pub enum SerdeError {
    NotAMap(String),
    NotAnObject(String),
}

#[derive(Debug)]
pub enum Error {
    SerdeError(SerdeError),
    SQLError(sqlx::Error),
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::SQLError(e)
    }
}

impl From<SerdeError> for Error {
    fn from(e: SerdeError) -> Self {
        Error::SerdeError(e)
    }
}

// ################################################
// ######## build prefix map ######################
// ################################################
//
pub async fn get_prefix_map(iris: &HashSet<String>, pool: &SqlitePool) -> Result<Value, Error> {
    let mut json_map = Map::new();

    let prefixes: HashSet<&str> = iris
        .iter()
        .map(|x| {
            let h = x.as_str();
            let split: Vec<&str> = h.split(":").collect();
            split[0]
        })
        .collect();

    for p in prefixes {
        let query = format!("SELECT * FROM prefix WHERE prefix='{}'", p);
        let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
        for r in rows {
            //NB: there is only one row (becase 'prefix' is a primary key)
            let base: &str = r.get("base");
            json_map.insert(String::from(p), json!(base));
        }
    }

    Ok(json!({ "@prefixes": json_map }))
}

// ################################################
// ######## build label map ####################
// ################################################

pub async fn get_label_map(
    iris: &HashSet<String>,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, Error> {
    let mut entity_2_label = HashMap::new();

    //get labels for all subjects
    for i in iris {
        let label = get_label(&i, table, pool).await;
        match label {
            Ok(x) => {
                entity_2_label.insert(i, x);
            }
            Err(_x) => {} //TODO
        };
    }

    //merge label maps
    let mut json_map = Map::new();
    for (k, v) in entity_2_label {
        json_map.insert(k.clone(), json!(v));
    }

    Ok(json!({ "@labels": json_map }))
}

pub async fn get_label(entity: &str, table: &str, pool: &SqlitePool) -> Result<String, Error> {
    let query = format!(
        "SELECT * FROM {} WHERE subject='{}' AND predicate='rdfs:label'",
        table, entity
    );
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    //NB: this should be a singleton
    for row in rows {
        //let subject: &str = row.get("subject");
        let label: &str = row.get("object");
        return Ok(String::from(label));
    }

    Err(Error::SQLError(sqlx::Error::RowNotFound))
}

// ################################################
// ######## build property map ####################
// ################################################
pub async fn get_json_representation(
    subject: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<String, Error> {
    let json_map = get_subject_map(subject, table, pool).await;
    match json_map {
        Ok(x) => Ok(x.to_string()),
        Err(x) => Err(x),
    }
}

pub async fn get_subject_map(
    subject: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, Error> {
    let query = format!("SELECT * FROM {} WHERE subject='{}'", table, subject);
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    let predicates: Vec<Value> = rows.iter().map(|row| ldtab_2_json_shape(row)).collect();

    let mut predicate_map = Map::new();
    for p in predicates {
        match p {
            Value::Object(mut x) => predicate_map.append(&mut x),
            //TODO: what's an idiomatic of nesting errors?
            _ => {
                return Err(Error::SerdeError(SerdeError::NotAnObject(format!(
                    "Given Value: {}",
                    p.to_string()
                ))))
            }
        }
    }

    //1. predicate map
    let subject_map = json!({ subject: predicate_map });

    //extract IRIs
    let mut iris = HashSet::new();
    signature::get_iris(&subject_map, &mut iris);

    //2. labels
    let label_map = get_label_map(&iris, table, pool).await;

    //3. prefixes
    let prefix_map = get_prefix_map(&iris, pool).await;

    //4. putting things together
    let mut json_map = Map::new();
    match subject_map {
        Value::Object(mut map) => json_map.append(&mut map),
        _ => {
            return Err(Error::SerdeError(SerdeError::NotAMap(format!(
                "Given Value: {}",
                subject_map.to_string()
            ))))
        }
    }

    if let Ok(object) = label_map {
        match object {
            Value::Object(mut map) => json_map.append(&mut map),
            _ => {
                return Err(Error::SerdeError(SerdeError::NotAMap(format!(
                    "Given Value: {}",
                    object.to_string()
                ))))
            }
        }
    }

    if let Ok(object) = prefix_map {
        match object {
            Value::Object(mut map) => json_map.append(&mut map),
            _ => {
                return Err(Error::SerdeError(SerdeError::NotAMap(format!(
                    "Given Value: {}",
                    object.to_string()
                ))))
            }
        }
    }

    Ok(Value::Object(json_map))
}

pub fn ldtab_2_json_shape(row: &SqliteRow) -> Value {
    let predicate: &str = row.get("predicate");
    let object: &str = row.get("object");
    let datatype: &str = row.get("datatype");
    //let  annotation : &str = row.get("annotation");//TODO

    //NB: an LDTab thick triple makes use of strings (which are not JSON strings
    //example: "this is a string" and "\"this is a JSON string\"".).
    let object_value = match from_str::<Value>(object) {
        Ok(x) => x,
        _ => json!(object),
    };

    //add datatype to object
    let object_datatype = json!({"object" : object_value, "datatype" : datatype});

    //put things into map
    json!({ predicate: vec![object_datatype] })
}
