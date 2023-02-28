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
pub async fn get_label_hash_map(
    iris: &HashSet<String>,
    table: &str,
    pool: &SqlitePool,
) -> HashMap<String, String> {
    let mut entity_2_label = HashMap::new();

    //get labels for all subjects
    for i in iris {
        let label = get_label(&i, table, pool).await;
        match label {
            Ok(x) => {
                entity_2_label.insert(i.clone(), x);
            }
            Err(_x) => {} //TODO
        };
    }

    entity_2_label
}

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
pub async fn get_property_map(
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
            _ => {
                return Err(Error::SerdeError(SerdeError::NotAnObject(format!(
                    "Given Value: {}",
                    p.to_string()
                ))))
            }
        }
    }

    Ok(Value::Object(predicate_map))
}

pub fn ldtab_2_json_shape(row: &SqliteRow) -> Value {
    let predicate: &str = row.get("predicate");
    let object: &str = row.get("object");
    let datatype: &str = row.get("datatype");
    let annotation: &str = row.get("annotation");

    //NB: an LDTab thick triple makes use of strings (which are not JSON strings
    //example: "this is a string" and "\"this is a JSON string\"".).
    let object_value = match from_str::<Value>(object) {
        Ok(x) => x,
        _ => json!(object),
    };

    //handle annotations
    let object_datatype = match from_str::<Value>(annotation) {
        Ok(annotation_value) => {
            json!({"object" : object_value, "datatype" : datatype, "annotation" : annotation_value})
        }
        _ => json!({"object" : object_value, "datatype" : datatype}),
    };

    //put things into map
    json!({ predicate: vec![object_datatype] })
}

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
// ################################################
// ######## HTML view #############################
// ################################################
pub fn ldtab_value_to_html(value: &Value, iri_2_label: &HashMap<String, String>) -> Value {
    let datatype = &value["datatype"];

    let mut list_element = Vec::new();
    list_element.push(json!("li"));

    match datatype {
        Value::String(x) => {
            match x.as_str() {
                "_IRI" => {
                    //get label (or use IRI if no label exists)
                    let entity = value["object"].as_str().unwrap();
                    let label = match iri_2_label.get(entity) {
                        Some(y) => y.clone(),
                        None => String::from(entity),
                    };
                    list_element.push(json!(["a", {"resource" : value["object"]}, label ]));
                }
                "_JSON" => {
                    list_element.push(json!("JSON"));
                } //TODO: encode Manchester
                _ => {
                    list_element.push(value["object"].clone());
                    list_element.push(json!(["sup", {"class" : "text-black-50"}, ["a", {"resource": x.as_str()}, x.as_str()]]));
                }
            }
        }
        _ => {
            json!("ERROR");
        } //TODO
    };
    Value::Array(list_element)
}

pub async fn get_predicate_map_hiccup(subject: &str, table: &str, pool: &SqlitePool) -> Value {
    let predicate_map = get_property_map(subject, table, pool).await.unwrap();

    //extract IRIs
    let mut iris = HashSet::new();
    signature::get_iris(&predicate_map, &mut iris);

    //2. labels
    let label_map = get_label_hash_map(&iris, table, pool).await;
    println!("LABELS: {:?}", label_map);

    let mut outer_list = Vec::new();
    outer_list.push(json!("ul"));

    //each key gets a li
    for (key, value) in predicate_map.as_object().unwrap() {
        let mut outer_list_element = Vec::new();
        outer_list_element.push(json!("li"));
        //encode key as a
        // res_elements.push(json!(["a", {"resource" : child["curie"], "about": parent, "rev":child["property"] }, child["label"] ]));

        outer_list_element.push(json!(["a", { "resource": key }, key])); //TODO: use key label

        let mut inner_list = Vec::new();
        inner_list.push(json!("ul"));
        for v in value.as_array().unwrap() {
            //TODO
            let v_encoding = ldtab_value_to_html(v, &label_map);
            inner_list.push(json!(v_encoding));
        }
        outer_list_element.push(json!(inner_list));
        outer_list.push(json!(outer_list_element));
    }
    json!(outer_list)
}

// ################################################
// ######## putting things together ###############
// ################################################
//
pub async fn get_things(subject: &str, table: &str, pool: &SqlitePool) -> (Value, Value, Value) {
    let predicate_map = get_property_map(subject, table, pool).await.unwrap();
    let subject_map = json!({ subject: predicate_map });

    //extract IRIs
    let mut iris = HashSet::new();
    signature::get_iris(&subject_map, &mut iris);

    //2. labels
    let label_map = get_label_map(&iris, table, pool).await;

    //3. prefixes
    let prefix_map = get_prefix_map(&iris, pool).await;

    (subject_map, label_map.unwrap(), prefix_map.unwrap())
}

pub async fn get_subject_map(
    subject: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, Error> {
    //1. predicate map
    let predicate_map = get_property_map(subject, table, pool).await?;
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
