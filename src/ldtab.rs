use serde_json::{from_str, json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

// ################################################
// ######## wiring utility functions ##############
// ################################################
// TODO: put these in wiring_rs
pub fn get_iris_from_object(ldtab_object: &Map<String, Value>, iris: &mut HashSet<String>) {
    if ldtab_object.contains_key("datatype") {
        match ldtab_object.get("datatype") {
            Some(x) => {
                //get datatype ...
                match x {
                    Value::String(y) => {
                        //... as a string ...
                        match y.as_str() {
                            //... to check its value
                            "_IRI" => {
                                let object = ldtab_object.get("object").unwrap();
                                iris.insert(String::from(object.as_str().unwrap()));
                            }
                            "_JSON" => {
                                let object = ldtab_object.get("object").unwrap();
                                get_iris(object, iris);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            _ => panic!(), //TODO: error handling
        }
    } else {
        for v in ldtab_object.values() {
            get_iris(&v, iris);
        }
    }
}

pub fn get_iris_from_array(ldtab_array: &Vec<Value>, iris: &mut HashSet<String>) {
    for a in ldtab_array {
        get_iris(&a, iris);
    }
}

pub fn get_iris(ldtab_thick_triple_object: &Value, iris: &mut HashSet<String>) {
    match ldtab_thick_triple_object {
        Value::Array(a) => get_iris_from_array(&a, iris),
        Value::Object(o) => get_iris_from_object(&o, iris),
        _ => {}
    }
}

pub fn ldtab_get_iris(row: &SqliteRow) -> HashSet<String> {
    let subject: &str = row.get("subject");
    let predicate: &str = row.get("predicate");
    let object: &str = row.get("object");
    let datatype: &str = row.get("datatype");

    //NB: an LDTab thick triple makes use of strings (which are not JSON strings
    //example: "this is a string" and "\"this is a JSON string\"".).
    let object_value = match from_str::<Value>(object) {
        Ok(x) => x,
        _ => json!(object),
    };

    //add datatype to object
    let object_datatype = json!({"object" : object_value, "datatype" : datatype});

    let mut iris = HashSet::new();
    get_iris(&object_datatype, &mut iris);
    iris.insert(String::from(subject));
    iris.insert(String::from(predicate));

    iris
}

// ################################################
// ######## putting things toghether ##############
// ################################################
pub async fn get_json_map(subject: &str, pool: &SqlitePool) -> Result<Value, sqlx::Error> {
    let properties = get_subject_map(subject, &pool).await;
    let labels = get_label_map(subject, &pool).await;
    let prefixes = get_prefix_map(subject, &pool).await;

    let mut json_map = Map::new();
    if let Ok(object) = properties {
        match object {
            Value::Object(mut map) => json_map.append(&mut map),
            _ => panic!(),
        }
    }

    if let Ok(object) = labels {
        match object {
            Value::Object(mut map) => json_map.append(&mut map),
            _ => panic!(),
        }
    }

    if let Ok(object) = prefixes {
        match object {
            Value::Object(mut map) => json_map.append(&mut map),
            _ => panic!(),
        }
    }

    Ok(Value::Object(json_map))
}

// ################################################
// ######## build prefix map ######################
// ################################################
pub async fn get_prefix_map(subject: &str, pool: &SqlitePool) -> Result<Value, sqlx::Error> {
    //get triples for subject
    let query = format!("SELECT * FROM statement WHERE subject='{}'", subject);
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    //get all iris
    let mut all_iris = HashSet::new();
    for row in rows {
        let iris = ldtab_get_iris(&row);
        all_iris.extend(iris);
    }

    //collect all iris
    let prefixes: HashSet<&str> = all_iris
        .iter()
        .map(|x| {
            let h = x.as_str();
            let split: Vec<&str> = h.split(":").collect();
            split[0]
        })
        .collect();

    let mut json_map = Map::new();
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

pub async fn get_label_map(subject: &str, pool: &SqlitePool) -> Result<Value, sqlx::Error> {
    //get triples for subject
    let query = format!("SELECT * FROM statement WHERE subject='{}'", subject);
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    let mut entity_2_label = HashMap::new();

    //get labels for all subjects
    for row in rows {
        let labels = get_labels(&row, pool).await;
        entity_2_label.extend(labels);
    }

    //merge label maps
    let mut json_map = Map::new();
    for (k, v) in entity_2_label {
        json_map.insert(k, json!(v));
    }

    Ok(json!({ "@labels": json_map }))
}

pub async fn get_labels(
    ldtab_thick_triple: &SqliteRow,
    pool: &SqlitePool,
) -> HashMap<String, String> {
    let iris = ldtab_get_iris(ldtab_thick_triple);
    let mut entity_2_label = HashMap::new();

    for iri in iris {
        match get_label(&iri, pool).await {
            Ok((entity, label)) => {
                entity_2_label.insert(entity, label);
            }
            _ => {}
        };
    }

    entity_2_label
}

pub async fn get_label(entity: &str, pool: &SqlitePool) -> Result<(String, String), sqlx::Error> {
    let query = format!(
        "SELECT * FROM statement WHERE subject='{}' AND predicate='rdfs:label'",
        entity
    );
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    //NB: this should be a singleton
    for row in rows {
        let subject: &str = row.get("subject");
        let label: &str = row.get("object");
        return Ok((String::from(subject), String::from(label)));
    }

    Err(sqlx::Error::RowNotFound)
}

// ################################################
// ######## build property map ####################
// ################################################
pub async fn get_subject_map(subject: &str, pool: &SqlitePool) -> Result<Value, sqlx::Error> {
    let query = format!("SELECT * FROM statement WHERE subject='{}'", subject);
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    let predicates: Vec<Value> = rows.iter().map(|row| ldtab_2_json_shape(row)).collect();

    let mut predicate_map = Map::new();
    for p in predicates {
        match p {
            Value::Object(mut x) => predicate_map.append(&mut x),
            _ => panic!(),
        }
    }

    let subject_map = json!({ subject: predicate_map });

    Ok(subject_map)
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
