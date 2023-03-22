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

/// Given a set of IRIs and a connection to an LDTab database,
/// return a map from prefixes to their IRIs
/// (encoded as a JSON Object).
///
/// TODO: example
/// TODO: doc test
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
// ############### label map ######################
// ################################################

/// Given a set of IRIs, a database connection and a target table,
/// return a map from prefixes to their IRIs.
///
/// TODO: example
/// TODO: doc test
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

/// Given a set of IRIs, a database connection, and a target table,
/// return a map from prefixes to their IRIs
/// (encoded as a JSON Object).
///
/// TODO: example
/// TODO: doc test
pub async fn get_label_map(
    iris: &HashSet<String>,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, Error> {
    let entity_2_label = get_label_hash_map(iris, table, pool).await;

    //encode HashMap as JSON object
    let mut json_map = Map::new();
    for (k, v) in entity_2_label {
        json_map.insert(k.clone(), json!(v));
    }

    //return label map
    Ok(json!({ "@labels": json_map }))
}

/// Given a set of IRIs, a database connection, and a target table,
/// return the label for the IRI.
///
/// TODO: example
/// TODO: doc test
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
// ############ property map ######################
// ################################################

/// Given an IRI, a database connection, and a target table,
/// return a map from the subject's properties to their values
/// (encoded as a JSON Object).
///
/// TODO: example
/// TODO: doc test
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

/// Given a row in from an LDTab database,
/// return a JSON Object mapping the predicate value to the object value.
///
/// TODO: example
/// TODO: doc test
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

// ################################################
// ######## HTML view #############################
// ################################################
//
/// Given a property, its corresponding value, and a map from IRIs to labels
/// return an HTML (JSON Hiccup) encoding for the term property shape.
///
/// TODO: example
/// TODO: doc test
pub fn ldtab_value_to_html(
    property: &str,
    value: &Value,
    iri_2_label: &HashMap<String, String>,
) -> Value {
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
                    list_element.push(
                        json!(["a", {"property" : property, "resource" : value["object"]}, label ]),
                    );
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

//Given a predicate map, a label map, a starting and ending list of CURIEs,
//return the tuple: (order_vector, curie_2_value_map) where
// - the order_vector contains an order of CURIEs by labels
//   (see https://github.com/ontodev/gizmos#predicates for details)
// - the curie_2_value map is a HashMap from property CURIEs to values
pub fn sort_predicate_map_by_label(
    predicate_map: &Value,
    label_map: &HashMap<String, String>,
    starting_order: &Vec<String>,
    ending_order: &Vec<String>,
) -> (Vec<String>, HashMap<String, Value>) {
    //let mut keys = Vec::new();
    let mut keys = HashSet::new(); //this operates on CURIEs
    let mut label_2_iri = HashMap::new();
    let mut iri_2_label = HashMap::new();
    let mut key_2_value = HashMap::new();

    for (key, value) in predicate_map.as_object().unwrap() {
        let key_label = match label_map.get(key) {
            Some(x) => x,
            None => key,
        };

        keys.insert(key);
        iri_2_label.insert(key, key_label);
        label_2_iri.insert(key_label, key.clone());
        key_2_value.insert(key.clone(), value.clone());
    }

    let mut middle_order = Vec::new(); //this holds labels .. because we order by label

    for key in starting_order {
        if keys.contains(key) {
            keys.remove(key);
        }
    }
    for key in ending_order {
        if keys.contains(key) {
            keys.remove(key);
        }
    }
    for key in keys {
        let label = iri_2_label.get(key).unwrap();
        middle_order.push(label);
    }

    middle_order.sort();

    let mut order = Vec::new();
    for key in starting_order {
        order.push(key.clone());
    }
    for key in middle_order {
        let iri = label_2_iri.get(key).unwrap();
        order.push(iri.clone());
    }
    for key in ending_order {
        order.push(key.clone());
    }

    (order, key_2_value)
}

/// Given a subject, an LDTab database, and a target table,
/// return an HTML (JSON Hiccup) encoding of the term property shape.
///
/// TODO: example
/// TODO: doc test
pub async fn get_predicate_map_hiccup(
    subject: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, Error> {
    let predicate_map = get_property_map(subject, table, pool).await?;

    //extract IRIs
    let mut iris = HashSet::new();
    signature::get_iris(&predicate_map, &mut iris);

    //2. labels
    let label_map = get_label_hash_map(&iris, table, pool).await;

    let mut outer_list = Vec::new();
    outer_list.push(json!("ul"));
    outer_list.push(json!({"id":"annotations", "style" : "margin-left: -1rem;"}));

    //Give precedence to labels, definitions
    //TODO: synonyms
    //TODO: the ordering of predicates needs to be passed as a parameter
    let starting_order = vec![String::from("rdfs:label"), String::from("obo:IAO_0000115")];
    //Put comments last
    let ending_order = vec![String::from("rdfs:comment")];

    let (order, key_2_value) =
        sort_predicate_map_by_label(&predicate_map, &label_map, &starting_order, &ending_order);

    for key in order {
        //build list elements according to the specified order
        if !key_2_value.contains_key(&key) {
            //skip properties that have been specified in the desired ordering
            //but that are not found in the database
            continue;
        }

        //get property values and labels
        let value = key_2_value.get(&key).unwrap();
        let key_label = match label_map.get(&key) {
            Some(x) => x.clone(),
            None => key.clone(),
        };

        //build HTML (encoded via JSON hiccup)
        let mut outer_list_element = Vec::new();
        outer_list_element.push(json!("li"));
        outer_list_element.push(json!(["a", { "resource": key.clone() }, key_label]));

        let mut inner_list = Vec::new();
        inner_list.push(json!("ul"));
        for v in value.as_array().unwrap() {
            let v_encoding = ldtab_value_to_html(&key, v, &label_map);
            inner_list.push(json!(v_encoding));
        }
        outer_list_element.push(json!(inner_list));
        outer_list.push(json!(outer_list_element));
    }
    Ok(json!(outer_list))
}

// ################################################
// ######## putting things together ###############
// ################################################
//
/// Given a subject, an LDTab database, and a target table,
/// return an HTML (JSON Hiccup) encoding of
/// - the term property shape
/// - the prefix map
/// - the label map
/// as a JSON object
///
/// TODO: example
/// TODO: doc test
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

// ################################################
// ######## Demo for constituent parts ############
// ################################################
pub async fn demo(subject: &str, table: &str, pool: &SqlitePool) -> (Value, Value, Value, Value) {
    //build term property JSON shape
    let predicate_map = get_property_map(subject, table, pool).await.unwrap();
    let subject_map = json!({ subject: predicate_map });

    //extract IRIs in JSON shape
    let mut iris = HashSet::new();
    signature::get_iris(&subject_map, &mut iris);

    //build label & prefix maps
    let label_map = get_label_map(&iris, table, pool).await.unwrap();
    let prefix_map = get_prefix_map(&iris, pool).await.unwrap();

    let html_hiccup = get_predicate_map_hiccup(subject, table, pool)
        .await
        .unwrap();

    (subject_map, label_map, prefix_map, html_hiccup)
}
