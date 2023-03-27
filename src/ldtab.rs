use itertools::Itertools;
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

/// Given a set of CURIEs, return the set of all used prefixes.
///
/// # Example
///
/// Let s = {obo:example1, owl:example2, rdfs:example3} be a set of CURIEs.
/// Then get_prefixes(s) returns the set {obo, owl, rdfs}.
fn get_prefixes(curies: &HashSet<String>) -> HashSet<String> {
    //LDTab does not distinguish between CURIEs and IRIs -- both are typed with "_IRI".
    //This may lead to unexpected behavior if IRIs are passed instead of a CURIEs.
    let prefixes: HashSet<String> = curies
        .iter()
        .map(|x| {
            let split: Vec<&str> = x.split(":").collect();
            String::from(split[0])
        })
        .collect();

    return prefixes;
}

/// Given a set of prefixes, return a query string for an LDTab database
/// that yields a map from prefixes to their respective bases.
///
/// # Examples
///
/// Let S = {obo, owl, rdf} be a set of prefixes.
/// Then build_prefix_query_for(S) returns the query
/// "SELECT prefix, base FROM prefix WHERE prefix IN ('obo','owl','rdf')".
/// Note that the set of prefixes is not ordered.
/// So, the prefixes are listed in an arbitrary order in the SQLite IN operator.  
fn build_prefix_query_for(prefixes: &HashSet<String>) -> String {
    let quoted_prefixes: HashSet<String> = prefixes.iter().map(|x| format!("'{}'", x)).collect();
    let joined_quoted_prefixes = itertools::join(&quoted_prefixes, ",");
    let query = format!(
        "SELECT prefix, base FROM prefix WHERE prefix IN ({prefixes})",
        prefixes = joined_quoted_prefixes
    );
    query
}

/// Given a set of CURIEs, and an LDTab database,
/// return a map from prefixes to their respective IRI bases.
///
/// # Examples
///
/// Let S = {obo:ZFA_0000354, rdfs:label} be a set of CURIEs
/// and Ldb an LDTab database.
/// Then get_prefix_hash_map(S, Ldb) returns the map
/// {"obo": "http://purl.obolibrary.org/obo/",
///  "rdfs": "http://www.w3.org/2000/01/rdf-schema#"}.  
async fn get_prefix_hash_map(
    curies: &HashSet<String>,
    pool: &SqlitePool,
) -> Result<HashMap<String, String>, sqlx::Error> {
    let prefixes = get_prefixes(&curies);
    let query = build_prefix_query_for(&prefixes);
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    let mut prefix_2_base = HashMap::new();
    for r in rows {
        //NB: there is only one row (becase 'prefix' is a primary key)
        let prefix: &str = r.get("prefix");
        let base: &str = r.get("base");
        prefix_2_base.insert(String::from(prefix), String::from(base));
    }
    Ok(prefix_2_base)
}

/// Given a set of CURIEs and an LDTab database,
/// return a mapping of prefixes to their IRI bases in a JSON object.
///
/// # Examples
///
/// Let S = {obo:ZFA_0000354, rdfs:label} be a set of CURIEs
/// and Ldb an LDTab database.
/// Then get_prefix_map(S) returns the JSON object
///
/// {"@prefixes":
///     {"obo":"http://purl.obolibrary.org/obo/",
///      "rdfs":"http://www.w3.org/2000/01/rdf-schema#"}
/// }
pub async fn get_prefix_map(
    curies: &HashSet<String>,
    pool: &SqlitePool,
) -> Result<Value, sqlx::Error> {
    let prefix_2_base = get_prefix_hash_map(curies, pool).await?;
    Ok(json!({ "@prefixes": prefix_2_base }))
}

// ################################################
// ############### label map ######################
// ################################################

/// Given a set of CURIEs, return a query string for an LDTab database
/// that yields a map from CURIEs to their respective rdfs:labels.
///
/// # Examples
///
/// Let S = {obo:ZFA_0000354, obo:ZFA_0000272} be a set of CURIEs.
/// Then build_label_query_for(S,table) returns the query
/// SELECT subject, predicate, object FROM table WHERE subject IN ('obo:ZFA_0000354',obo:ZFA_0000272) AND predicate='rdfs:label'
fn build_label_query_for(curies: &HashSet<String>, table: &str) -> String {
    let quoted_curies: HashSet<String> = curies.iter().map(|x| format!("'{}'", x)).collect();
    let joined_quoted_curies = itertools::join(&quoted_curies, ",");
    let query = format!(
        "SELECT subject, predicate, object FROM {table} WHERE subject IN ({curies}) AND predicate='rdfs:label'",
        table=table,
        curies=joined_quoted_curies
    );
    query
}

/// Given a set of CURIEs, and an LDTab database,
/// return a map from CURIEs to their respective rdfs:labels.
///
/// # Example
///
/// Let S = {obo:ZFA_0000354, rdfs:label} be a set of CURIEs
/// and Ldb an LDTab database.
/// Then get_label_hash_map(S, table, Ldb) returns the map
/// {"obo:ZFA_0000354": "gill",
///  "rdfs:label": "label"}
/// extracted from a given table in Ldb.  
async fn get_label_hash_map(
    curies: &HashSet<String>,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashMap<String, String>, sqlx::Error> {
    let mut entity_2_label = HashMap::new();
    let query = build_label_query_for(&curies, table);
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let entity: &str = row.get("subject");
        let label: &str = row.get("object");
        entity_2_label.insert(String::from(entity), String::from(label));
    }
    Ok(entity_2_label)
}

/// Given a set of CURIEs and an LDTab database,
/// return a mapping of CURIEs to their labels in a JSON object.
///
/// # Examples
///
/// Let S = {obo:ZFA_0000354, rdfs:label} be a set of CURIEs
/// and Ldb an LDTab database.
/// Then get_prefix_map(S) returns the JSON object
///
/// {"@labels":
///     {"obo:ZFA_0000354":"gill",
///      "obo:ZFA_0000272":"respiratory system"}
/// }

pub async fn get_label_map(
    iris: &HashSet<String>,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, sqlx::Error> {
    let entity_2_label = get_label_hash_map(iris, table, pool).await?;
    Ok(json!({ "@labels": entity_2_label }))
}

// ################################################
// ############ property map ######################
// ################################################

/// Given a CURIE for an entity, an LDTAb database, and a target table,
/// return a map from the entity's properties to their LDTab values.
///
/// # Examples
/// Let zfa.db be an LDTab database containing information about "ZFA_0000354".
/// Then get_property_map(ZFA_0000354, statements, zfa.db)
/// returns
///
/// {"oboInOwl:hasDbXref":[{"object":"TAO:0000354","datatype":"xsd:string"}],
///  "oboInOwl:id":[{"object":"ZFA:0000354","datatype":"xsd:string"}],
///  "rdf:type":[{"object":"owl:Class","datatype":"_IRI"}],
///  "rdfs:label":[{"object":"gill","datatype":"xsd:string"}],
///  "oboInOwl:hasOBONamespace":[{"object":"zebrafish_anatomy","datatype":"xsd:string"}]
///  ... }
pub async fn get_property_map(
    subject: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, Error> {
    let query = format!(
        "SELECT * FROM {table} WHERE subject='{subject}'",
        table = table,
        subject = subject
    );
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    let predicates: Vec<Value> = rows
        .iter()
        .map(|row| ldtab_row_2_predicate_json_shape(row))
        .collect();

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

/// Convert LDTab strings to serde Values.
/// LDTab makes use of both strings and JSON strings.
/// For example values in the 'subject' column of an LDTab database are
/// conventional strings -- NOT JSON strings.
///
/// # Examples
///
/// 1. "obo:ZFA_0000354" is a string.
/// 2. "\"obo:ZFA_0000354\"" is a JSON string.  
/// 3. ldtab_string_2_serde_value("obo:ZFA_0000354") returns Value::String("obo:ZFA_0000354")
/// 4. ldtab_string_2_serde_value("\"obo:ZFA_0000354\"") returns Value::String("obo:ZFA_0000354")
/// 5. ldtab_string_2_serde_value("{\"a\":\"b\"}") returns Value::Object {"a": Value::String("b")}
fn ldtab_string_2_serde_value(string: &str) -> Value {
    //NB: an LDTab thick triple makes use of strings (which are not JSON strings
    //example: "this is a string" and "\"this is a JSON string\"".).
    let serde_value = match from_str::<Value>(string) {
        Ok(x) => x,
        _ => json!(string),
    };

    serde_value
}

/// Given a row in from an LDTab database, return its JSON encoding.
///
/// Examples
///
/// Let (subject=ZFA_0000354, predicate=rdfs:label, object="gill", datatype="xsd:string") a row.
/// Return {"rdfs:label":[{"object":"gill","datatype":"xsd:string"}]}.
fn ldtab_row_2_predicate_json_shape(row: &SqliteRow) -> Value {
    let predicate: &str = row.get("predicate");
    let object: &str = row.get("object");
    let datatype: &str = row.get("datatype");
    let annotation: &str = row.get("annotation");

    let object_json_shape = object_2_json_shape(object, datatype, annotation);

    json!({ predicate: vec![object_json_shape] })
}

/// Given an object, datatype, and an annotation, return an LDTab JSON shape.
fn object_2_json_shape(object: &str, datatype: &str, annotation: &str) -> Value {
    let object_value = ldtab_string_2_serde_value(object);

    let json_shape = match from_str::<Value>(annotation) {
        Ok(annotation_value) => {
            json!({"object" : object_value, "datatype" : datatype, "annotation" : annotation_value})
        }
        _ => json!({"object" : object_value, "datatype" : datatype}),
    };

    json_shape
}

// ################################################
// ######## HTML view #############################
// ################################################
//

/// Given a property, a value, and a map from CURIEs/IRIs to labels,
/// return a hiccup-style list encoding an hyperlink.
///
/// Example
/// TODO
fn ldtab_iri_2_hiccup(
    property: &str,
    value: &Value,
    iri_2_label: &HashMap<String, String>,
) -> Value {
    let entity = value["object"].as_str().unwrap();
    let label = match iri_2_label.get(entity) {
        Some(y) => y.clone(),
        None => String::from(entity),
    };
    json!(["a", {"property" : property, "resource" : value["object"]}, label ])
}

/// Given a property, a value, and a map from CURIEs/IRIs to labels,
/// return a hiccup-style list encoding a nested LDTab expression.
///
/// Example
/// TODO
fn ldtab_json_2_hiccup(
    property: &str,
    value: &Value,
    iri_2_label: &HashMap<String, String>,
) -> Value {
    //TODO: encode Manchester (wiring_rs currently only provides translations for triples - not objects)
    value.clone()
}

/// Given a property, a value, and a map from CURIEs to labels
/// return a hiccup-style list encoding of the term property shape.
///
/// TODO: example
/// TODO: doc test
fn ldtab_value_2_hiccup(
    property: &str,
    value: &Value,
    iri_2_label: &HashMap<String, String>,
) -> Value {
    //TODO: return error
    let mut list_element = Vec::new();
    list_element.push(json!("li"));

    let datatype = &value["datatype"];

    match datatype {
        Value::String(x) => match x.as_str() {
            "_IRI" => {
                list_element.push(ldtab_iri_2_hiccup(property, value, iri_2_label));
            }
            "_JSON" => {
                list_element.push(ldtab_json_2_hiccup(property, value, iri_2_label));
            }
            _ => {
                list_element.push(value["object"].clone());
                list_element.push(json!(["sup", {"class" : "text-black-50"}, ["a", {"resource": x.as_str()}, x.as_str()]]));
            }
        },
        _ => {
            json!("ERROR");
        } //TODO
    };
    Value::Array(list_element)
}

/// Given a predicate map, a label map, a starting and ending list of predicates,
/// return the tuple: (order_vector, predicate_2_value_map) where
///  - the order_vector contains an order of predicates by labels
///    (see https://github.com/ontodev/gizmos#predicates for details)
///  - the predicate_2_value map is a HashMap from predicates to values
///
/// Examples
/// TODO
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
/// return an HTML encoding (JSON Hiccup) of the term property shape.
///
/// # Examples
///
/// Let zfa.db be an LDTab database containing information about "ZFA_0000354".
/// Then  get_predicate_map_hiccup(ZFA_0000354, statement, zfa.db) returns the following:
///
/// ["ul",{"id":"annotations","style":"margin-left: -1rem;"},
///   ["li",["a",{"resource":"rdfs:label"},"rdfs:label"],
///         ["ul",
///           ["li","gill",["sup",{"class":"text-black-50"},["a",{"resource":"xsd:string"},"xsd:string"]]]
///         ]
///   ],
///   ["li",["a",{"resource":"oboInOwl:hasDbXref"},"oboInOwl:hasDbXref"],
///         ["ul",
///          ["li","TAO:0000354",["sup",{"class":"text-black-50"},["a",{"resource":"xsd:string"},"xsd:string"]]]
///         ]
///   ],
///   ...  
///   ...  
///   ...
/// ]
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
    let label_map = get_label_hash_map(&iris, table, pool).await.unwrap();

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
            let v_encoding = ldtab_value_2_hiccup(&key, v, &label_map);
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn test_get_prefixes() {
        let mut curies = HashSet::new();
        curies.insert(String::from("rdfs:label"));
        curies.insert(String::from("obo:example"));

        let prefixes = get_prefixes(&curies);

        let mut expected = HashSet::new();
        expected.insert(String::from("rdfs"));
        expected.insert(String::from("obo"));

        assert_eq!(prefixes, expected);
    }

    #[test]
    fn test_build_prefix_query_for() {
        let mut prefixes = HashSet::new();
        prefixes.insert(String::from("rdf"));
        prefixes.insert(String::from("rdfs"));

        let query = build_prefix_query_for(&prefixes);
        //Note: the order of arguments for the SQLite IN operator is not unique
        let expected_alternative_a =
            String::from("SELECT prefix, base FROM prefix WHERE prefix IN ('rdf','rdfs')");
        let expected_alternative_b =
            String::from("SELECT prefix, base FROM prefix WHERE prefix IN ('rdfs','rdf')");
        let check = query.eq(&expected_alternative_a) || query.eq(&expected_alternative_b);
        assert!(check);
    }

    #[tokio::test]
    async fn test_get_prefix_hash_map() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: SqlitePool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let mut curies = HashSet::new();
        curies.insert(String::from("obo:ZFA_0000354"));
        curies.insert(String::from("rdfs:label"));

        let prefix_hash_map = get_prefix_hash_map(&curies, &pool).await.unwrap();

        let mut expected = HashMap::new();
        expected.insert(
            String::from("obo"),
            String::from("http://purl.obolibrary.org/obo/"),
        );
        expected.insert(
            String::from("rdfs"),
            String::from("http://www.w3.org/2000/01/rdf-schema#"),
        );
        assert_eq!(prefix_hash_map, expected);
    }

    #[tokio::test]
    async fn test_get_prefix_map() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: SqlitePool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let mut curies = HashSet::new();
        curies.insert(String::from("obo:ZFA_0000354"));
        curies.insert(String::from("rdfs:label"));
        let prefix_map = get_prefix_map(&curies, &pool).await.unwrap();
        let expected_prefix_map = json!({"@prefixes":{"obo":"http://purl.obolibrary.org/obo/","rdfs":"http://www.w3.org/2000/01/rdf-schema#"}});
        assert_eq!(prefix_map, expected_prefix_map);
    }

    #[test]
    fn test_build_label_query_for() {
        let mut curies = HashSet::new();
        curies.insert(String::from("obo:ZFA_0000354"));
        curies.insert(String::from("rdfs:label"));

        let query = build_label_query_for(&curies, "statement");

        //NB: the order of arguments for the SQLite IN operator is not unique
        let expected_alternative_a = "SELECT subject, predicate, object FROM statement WHERE subject IN ('rdfs:label','obo:ZFA_0000354') AND predicate='rdfs:label'";
        let expected_alternative_b = "SELECT subject, predicate, object FROM statement WHERE subject IN ('obo:ZFA_0000354','rdfs:label') AND predicate='rdfs:label'";

        let check = query.eq(&expected_alternative_a) || query.eq(&expected_alternative_b);
        assert!(check);
    }

    #[tokio::test]
    async fn test_get_label_hash_map() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: SqlitePool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let table = "statement";

        let mut curies = HashSet::new();
        curies.insert(String::from("obo:ZFA_0000354"));

        let label_hash_map = get_label_hash_map(&curies, &table, &pool).await.unwrap();

        let mut expected = HashMap::new();
        expected.insert(String::from("obo:ZFA_0000354"), String::from("gill"));

        assert_eq!(label_hash_map, expected);
    }

    #[tokio::test]
    async fn test_get_label_map() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: SqlitePool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let table = "statement";

        let mut curies = HashSet::new();
        curies.insert(String::from("obo:ZFA_0000354"));
        let label_map = get_label_map(&curies, &table, &pool).await.unwrap();
        let expected_label_map = json!({"@labels":{"obo:ZFA_0000354":"gill"}});
        assert_eq!(label_map, expected_label_map);
    }

    #[tokio::test]
    async fn test_get_property_map() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: SqlitePool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let table = "statement";

        let property_map = get_property_map("obo:ZFA_0000354", &table, &pool)
            .await
            .unwrap();
        let expected = json!({"obo:IAO_0000115":[{"object":"Compound organ that consists of gill filaments, gill lamellae, gill rakers and pharyngeal arches 3-7. The gills are responsible for primary gas exchange between the blood and the surrounding water.","datatype":"xsd:string","annotation":{"<http://www.geneontology.org/formats/oboInOwl#hasDbXref>":[{"datatype":"xsd:string","meta":"owl:Axiom","object":"http:http://www.briancoad.com/Dictionary/DicPics/gill.htm"}]}}],"<http://www.geneontology.org/formats/oboInOwl#hasDbXref>":[{"object":"TAO:0000354","datatype":"xsd:string"}],"rdfs:subClassOf":[{"object":{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000000"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]},"datatype":"_JSON"}],"<http://www.geneontology.org/formats/oboInOwl#id>":[{"object":"ZFA:0000354","datatype":"xsd:string"}],"rdf:type":[{"object":"owl:Class","datatype":"_IRI"}],"rdfs:label":[{"object":"gill","datatype":"xsd:string"}],"<http://www.geneontology.org/formats/oboInOwl#hasOBONamespace>":[{"object":"zebrafish_anatomy","datatype":"xsd:string"}],"<http://www.geneontology.org/formats/oboInOwl#hasExactSynonym>":[{"object":"gills","datatype":"xsd:string","annotation":{"<http://www.geneontology.org/formats/oboInOwl#hasSynonymType>":[{"datatype":"_IRI","meta":"owl:Axiom","object":"obo:zfa#PLURAL"}]}}]});

        assert_eq!(property_map, expected);
    }

    #[test]
    fn test_ldtab_string_2_serde_value_conventional_string() {
        let subject = "obo:ZFA_0000354";
        let ldtab_encoding = ldtab_string_2_serde_value(subject);
        let expected = Value::String(String::from(subject));
        assert_eq!(ldtab_encoding, expected);
    }

    #[test]
    fn test_ldtab_string_2_serde_value_json_string() {
        let subject = "\"obo:ZFA_0000354\"";
        let ldtab_encoding = ldtab_string_2_serde_value(subject);
        let expected = Value::String(String::from("obo:ZFA_0000354"));
        assert_eq!(ldtab_encoding, expected);
    }

    #[tokio::test]
    async fn test_ldtab_row_2_predicate_json_shape() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: SqlitePool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let table = "statement";
        let query = "SELECT * FROM statement WHERE predicate='rdfs:label'";
        let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(&pool).await.unwrap();
        let row = &rows[0]; //NB: there is a unique row (with rdfs:label)
        let json_shape = ldtab_row_2_predicate_json_shape(row);
        let expected = json!({"rdfs:label":[{"object":"gill","datatype":"xsd:string"}]});
        assert_eq!(json_shape, expected);
    }
}
