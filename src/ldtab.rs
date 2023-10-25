use ontodev_hiccup::hiccup;
use serde_json::{from_str, json, Map, Value};
use sqlx::any::{AnyPool, AnyRow};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use wiring_rs::ldtab_2_ofn::class_translation::translate;
use wiring_rs::ofn_2_rdfa::class_translation::translate as rdfa_translate;
use wiring_rs::ofn_typing::translation::type_ofn;
use wiring_rs::util::parser::parse_thick_triple_object;
use wiring_rs::util::signature;

#[derive(Debug)]
pub enum SerdeError {
    NotAMap(String),
    NotAnObject(String),
}

#[derive(Debug)]
pub enum LDTabError {
    DataFormatViolation(String),
}

#[derive(Debug)]
pub enum Error {
    SerdeError(SerdeError),
    SQLError(sqlx::Error),
    LDTabError(LDTabError),
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

impl From<LDTabError> for Error {
    fn from(e: LDTabError) -> Self {
        Error::LDTabError(e)
    }
}

/// Given a str for a CURIE/IRI, return the CURIE as a String,
/// or return the IRI without angle brackets.
///
/// # Examples
///
/// encode_iri("obo:RO_0002131") returns "obo:RO_0002131"
/// encode_iri(<http://purl.obolibrary.org/obo/ZFA_0000496>) returns "http://purl.obolibrary.org/obo/ZFA_0000496"
pub fn encode_iri(entity: &str) -> String {
    if entity.starts_with("<") && entity.ends_with(">") {
        let entity_len = entity.len();
        entity[1..entity_len - 1].to_string()
    } else {
        String::from(entity)
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
    pool: &AnyPool,
) -> Result<HashMap<String, String>, sqlx::Error> {
    let prefixes = get_prefixes(&curies);
    let query = build_prefix_query_for(&prefixes);
    let rows: Vec<AnyRow> = sqlx::query(&query).fetch_all(pool).await?;
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
    pool: &AnyPool,
) -> Result<Value, sqlx::Error> {
    let prefix_2_base = get_prefix_hash_map(curies, pool).await?;
    Ok(json!({ "@prefixes": prefix_2_base }))
}

// ################################################
// ############### label map ######################
// ################################################

/// Given a set of CURIEs/IRIs, return a query string for an LDTab database
/// that yields a map from CURIEs/IRIs to their respective rdfs:labels.
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

/// Given a set of CURIEs/IRIs, and an LDTab database,
/// return a map from CURIEs/IRIs to their respective rdfs:labels.
///
/// # Example
///
/// Let S = {obo:ZFA_0000354, rdfs:label} be a set of CURIEs
/// and Ldb an LDTab database.
/// Then get_label_hash_map(S, table, Ldb) returns the map
/// {"obo:ZFA_0000354": "gill",
///  "rdfs:label": "label"}
/// extracted from a given table in Ldb.  
pub async fn get_label_hash_map(
    curies: &HashSet<String>,
    table: &str,
    pool: &AnyPool,
) -> Result<HashMap<String, String>, sqlx::Error> {
    let mut entity_2_label = HashMap::new();
    let query = build_label_query_for(&curies, table);
    let rows: Vec<AnyRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let entity: &str = row.get("subject");
        let label: &str = row.get("object");
        entity_2_label.insert(String::from(entity), String::from(label));
    }
    Ok(entity_2_label)
}

/// Given a set of CURIEs/IRIs and an LDTab database,
/// return a mapping of CURIEs/IRIs to their labels in a JSON object.
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
    pool: &AnyPool,
) -> Result<Value, sqlx::Error> {
    let entity_2_label = get_label_hash_map(iris, table, pool).await?;
    Ok(json!({ "@labels": entity_2_label }))
}

// ################################################
// ############ property map ######################
// ################################################

/// Given a CURIE/IRI for an entity, an LDTAb database, and a target table,
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
pub async fn get_property_map(subject: &str, table: &str, pool: &AnyPool) -> Result<Value, Error> {
    let query = format!(
        "SELECT * FROM {table} WHERE subject='{subject}'",
        table = table,
        subject = subject
    );
    let rows: Vec<AnyRow> = sqlx::query(&query).fetch_all(pool).await?;

    let predicates_2_values: Vec<(String, Value)> = rows
        .iter()
        .map(|row| ldtab_row_2_predicate_json_shape(row)) //returns a tuple: (predicate,json_value)
        .collect();

    let mut predicate_map = Map::new();
    for (p, v) in predicates_2_values {
        if predicate_map.contains_key(&p) {
            let array = predicate_map.get_mut(&p).unwrap();
            let array = array.as_array_mut().unwrap();
            array.push(v);
        } else {
            let element = vec![v];
            predicate_map.insert(p.clone(), json!(element));
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
/// Return {"rdfs:label":{"object":"gill","datatype":"xsd:string"}}.
fn ldtab_row_2_predicate_json_shape(row: &AnyRow) -> (String, Value) {
    let predicate: &str = row.get("predicate");
    let object: &str = row.get("object");
    let datatype: &str = row.get("datatype");
    let annotation: &str = row.get("annotation");

    let object_json_shape = object_2_json_shape(object, datatype, annotation);

    (String::from(predicate), json!(object_json_shape))
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
// ############### type map #######################
// ################################################

/// Given a set of CURIEs/IRIs, return a query string for an LDTab database
/// that yields a map from CURIEs/IRIs to their respective rdfs:labels.
///
/// # Examples
///
/// Let S = {obo:ZFA_0000354, obo:ZFA_0000272} be a set of CURIEs.
/// Then build_label_query_for(S,table) returns the query
/// SELECT subject, predicate, object FROM table WHERE subject IN ('obo:ZFA_0000354',obo:ZFA_0000272) AND predicate='rdf:type'
fn build_type_query_for(curies: &HashSet<String>, table: &str) -> String {
    let quoted_curies: HashSet<String> = curies.iter().map(|x| format!("'{}'", x)).collect();
    let joined_quoted_curies = itertools::join(&quoted_curies, ",");
    let query = format!(
        "SELECT subject, predicate, object FROM {table} WHERE subject IN ({curies}) AND predicate='rdf:type'",
        table=table,
        curies=joined_quoted_curies
    );
    query
}

/// Given a set of CURIEs/IRIs, and an LDTab database,
/// return a map from CURIEs/IRIs to their respective rdfs:labels.
///
/// # Example
///
/// Let S = {obo:ZFA_0000354, rdfs:label} be a set of CURIEs
/// and Ldb an LDTab database.
/// Then get_label_hash_map(S, table, Ldb) returns the map
/// {"obo:ZFA_0000354": "gill",
///  "rdfs:label": "label"}
/// extracted from a given table in Ldb.  
async fn get_type_hash_map(
    curies: &HashSet<String>,
    table: &str,
    pool: &AnyPool,
) -> Result<HashMap<String, HashSet<String>>, sqlx::Error> {
    let mut entity_2_type: HashMap<String, HashSet<String>> = HashMap::new();
    let query = build_type_query_for(&curies, table);
    let rows: Vec<AnyRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let entity: &str = row.get("subject");
        let rdf_type: &str = row.get("object");
        if entity_2_type.contains_key(entity) {
            let types: &mut HashSet<String> = entity_2_type.get_mut(entity).unwrap();
            types.insert(String::from(rdf_type));
        } else {
            entity_2_type.insert(String::from(entity), HashSet::new());
            let types: &mut HashSet<String> = entity_2_type.get_mut(entity).unwrap();
            types.insert(String::from(rdf_type));
        }
    }
    Ok(entity_2_type)
}

// ################################################
// ######## HTML view #############################
// ################################################
//

/// Given a property, a value, and a map from CURIEs/IRIs to labels,
/// return a hiccup-style list encoding an hyperlink.
///
/// Examples
///
/// ldtab_iri_2_hiccup("rdf:type, "owl:Class", {"owl:Class":"Class"})
///
/// returns
///
/// ["a",{"property":"rdf:type","resource":"owl:Class"},"Class"].  
fn ldtab_iri_2_hiccup(
    property: &str,
    value: &Value,
    iri_2_label: &HashMap<String, String>,
) -> Value {
    //get object
    let entity = value["object"].as_str().unwrap();

    //get label
    let label = match iri_2_label.get(entity) {
        Some(y) => y.clone(),
        None => String::from(entity),
    };
    //hiccup-style encoding
    json!(["a", {"property" : property, "resource" : value["object"]}, encode_iri(&label) ])
}

/// Given a property, an LDTab value with type _JSON,
/// a map from CURIEs/IRIs to labels, and
/// a map from CURIEs/IRIs to rdf:types,
/// return a hiccup-style list encoding of
/// the LDTab value's RDFa representation.
///
/// Examples
///
/// Consider the following as given input:
///
/// property = rdfs:subClassOf
/// value =  {"object":{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0000272"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]},"datatype":"_JSON"}
/// iri_2_label = {"obo:ZFA_0000272":"respiratory system"}
/// iri_2_type = {"obo:ZFA_0000272":{"owl:Class"}}
///
/// Then this ldtab_json_2_hiccup(&property, &value, &iri_2_label, &iri_2_type) returns:
///
/// ["span",{"property":"rdfs:subClassOf","typeof":"owl:Restriction"},["a",{"property":"owl:onProperty","resource":"obo:BFO_0000050"},"obo:BFO_0000050"]," some ",["a",{"property":"owl:someValuesFrom","resource":"obo:ZFA_0000272"},"respiratory system"]]
fn ldtab_json_2_hiccup(
    property: &str,
    value: &Value,
    iri_2_label: &HashMap<String, String>,
    iri_2_type: &HashMap<String, HashSet<String>>,
) -> Value {
    let object = value["object"].clone(); //unpack object
    let owl = parse_thick_triple_object(&object.to_string()); //parse as owl
    let ofn = translate(&owl);
    let ofn_typed = type_ofn(&ofn, iri_2_type);
    let ofn_rdfa = rdfa_translate(&ofn_typed, iri_2_label, Some(property));
    ofn_rdfa
}

fn ldtab_literal_2_hiccup(value: &Value) -> Value {
    value["object"].clone()
}

fn ldtab_datatype_2_hiccup(datatype: &str) -> Value {
    json!(["sup", {"class" : "text-black-50"}, ["a", {"resource": datatype}, datatype]])
}

fn ldtab_annotation_2_hiccup(
    annotation: &Map<String, Value>,
    iri_2_label: &HashMap<String, String>,
) -> Value {
    let mut outer_list = Vec::new();
    outer_list.push(json!("ul"));

    for (key, value) in annotation {
        let mut outer_list_element = Vec::new();
        outer_list_element.push(json!("li"));

        //get label
        let label = match iri_2_label.get(key) {
            Some(y) => y.clone(),
            None => String::from(key),
        };

        outer_list_element.push(json!(["small", ["a", { "resource": key }, label]]));

        let mut inner_list = Vec::new();
        inner_list.push(json!("ul"));
        match value {
            Value::Array(vec) => {
                for v in vec {
                    let mut inner_list_element = Vec::new();
                    inner_list_element.push(json!("li"));
                    let datatype = &v["datatype"];

                    match datatype {
                        Value::String(x) => match x.as_str() {
                            "_IRI" => {
                                inner_list_element.push(ldtab_iri_2_hiccup(key, v, iri_2_label));
                            }
                            "_JSON" => {} //TODO nested annotations
                            _ => {
                                inner_list_element.push(ldtab_literal_2_hiccup(v));
                                inner_list_element.push(ldtab_datatype_2_hiccup(x.as_str()));
                            }
                        },
                        _ => {}
                    };
                    inner_list.push(Value::Array(inner_list_element));
                }
            }
            _ => {}
        }
        outer_list_element.push(Value::Array(inner_list));
        outer_list.push(Value::Array(outer_list_element));
    }
    Value::Array(outer_list)
}

/// Given a property, a value, and a map from CURIEs/IRIs to labels
/// return a hiccup-style list encoding of the term property shape using:
///
/// - RDFa in the case of LDTab values of type _JSON (see ldtab_json_2_hiccup)
/// - hyperlinks for LDTab values of type _IRI (see ldtab_iri_2_hiccup)
/// - plain HTML for LDTab values of other types, i.e. RDF literals, (see ldtab_literal_2_hiccup),
///   which are rendered with their datatype as a superscript (see ldtab_datatype_2_hiccup)
fn ldtab_value_2_hiccup(
    property: &str,
    value: &Value,
    iri_2_label: &HashMap<String, String>,
    iri_2_type: &HashMap<String, HashSet<String>>,
) -> Result<Value, Error> {
    let mut list_element = Vec::new();
    list_element.push(json!("li"));

    let datatype = &value["datatype"];
    let annotation = &value["annotation"];

    match datatype {
        Value::String(x) => match x.as_str() {
            "_IRI" => {
                list_element.push(ldtab_iri_2_hiccup(property, value, iri_2_label));
            }
            "_JSON" => {
                list_element.push(ldtab_json_2_hiccup(
                    property,
                    value,
                    iri_2_label,
                    iri_2_type,
                ));
            }
            _ => {
                list_element.push(ldtab_literal_2_hiccup(value));
                list_element.push(ldtab_datatype_2_hiccup(x.as_str()));
            }
        },
        _ => {
            //TODO (depends on LDTab input -- which should be validated)
            return Err(Error::LDTabError(LDTabError::DataFormatViolation(format!(
                "Given Value: {}",
                datatype.to_string()
            ))));
        }
    };

    match annotation {
        Value::Object(x) => list_element.push(ldtab_annotation_2_hiccup(x, iri_2_label)),
        _ => {} //there is no annotation -- so do nothing
    }

    Ok(Value::Array(list_element))
}

/// Given a predicate map, a label map, a starting and ending list of predicates,
/// return the tuple: (order_vector, predicate_2_value_map) where
///  - the order_vector contains an order of predicates by labels
///    (see https://github.com/ontodev/gizmos#predicates for details)
///  - the predicate_2_value map is a HashMap from predicates to values
///
/// Examples
///
/// Let
/// pm = {
///       "rdf:type":[{"object":"owl:Class","datatype":"_IRI"}],
///       "rdfs:label":[{"object":"gill","datatype":"xsd:string"}],
///       "obo:IAO_0000115":[{"object":"Compound organ"}],
///     }
/// be a predicate map and
///  lm = {
///        "rdf:type":"type",
///        "rdfs:label":"label",
///        "obo:IAO_0000115":"definition",
///       }
/// a label map.
///
/// The lexicographical order of labels is ["definition", "label", "type"].
/// However, sort_predicate_map_by_label(pm, lm, ["label"], ["definition])
/// returns the tuple (["label", "type", "definition"], pm)
///  where pm is encoded as a HashMap instead of a Value.  
fn sort_predicate_map_by_label(
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
    pool: &AnyPool,
    predicate_order_start: &Vec<String>,
    predicate_order_end: &Vec<String>,
) -> Result<Value, Error> {
    let predicate_map = get_property_map(subject, table, pool).await?;

    //extract IRIs
    let mut iris = HashSet::new();
    signature::get_iris(&predicate_map, &mut iris);

    //2. labels
    let label_map = get_label_hash_map(&iris, table, pool).await?;

    //3. types
    let type_map = get_type_hash_map(&iris, table, pool).await?;

    let mut outer_list = Vec::new();
    outer_list.push(json!("ul"));
    outer_list.push(json!({"id":"annotations", "style" : "margin-left: -1rem;"}));

    let (order, key_2_value) = sort_predicate_map_by_label(
        &predicate_map,
        &label_map,
        &predicate_order_start,
        &predicate_order_end,
    );

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
        outer_list_element.push(json!(["a", { "resource": key.clone() }, encode_iri(&key_label)]));

        let mut inner_list = Vec::new();
        inner_list.push(json!("ul"));
        for v in value.as_array().unwrap() {
            let v_encoding = ldtab_value_2_hiccup(&key, v, &label_map, &type_map)?;
            inner_list.push(json!(v_encoding));
        }
        outer_list_element.push(json!(inner_list));
        outer_list.push(json!(outer_list_element));
    }
    Ok(json!(outer_list))
}

pub async fn get_predicate_map_html(
    subject: &str,
    table: &str,
    pool: &AnyPool,
    predicate_order_start: &Vec<String>,
    predicate_order_end: &Vec<String>,
) -> Result<String, Error> {
    //handle top level
    if subject.eq("owl:Class")
        || subject.eq("owl:AnnotationProperty")
        || subject.eq("owl:DataProperty")
        || subject.eq("owl:ObjectProperty")
        || subject.eq("owl:Individual")
        || subject.eq("rdfs:Datatype")
    {
        let hiccup = json!(["ul", ["p", {"class":"lead"}, "Hello! This is an ontology browser."], ["p", "An ontology is a terminology system designed for both humans and machines to read. Click the links on the left to browse the hierarchy of terms. Terms have parent terms, child terms, annotations, and logical axioms. The page for each term is also machine-readable using RDFa."]]);
        let html = match hiccup::render(&hiccup) {
            Ok(x) => x,
            Err(x) => x,
        };

        return Ok(html);
    }

    let hiccup = get_predicate_map_hiccup(
        subject,
        table,
        pool,
        predicate_order_start,
        predicate_order_end,
    )
    .await?;
    let html = match hiccup::render(&hiccup) {
        Ok(x) => x,
        Err(x) => x,
    };
    Ok(html)
}

// ################################################
// ######## putting things together ###############
// ################################################

/// Given a subject, an LDTab database, and a target table,
/// return an HTML (JSON Hiccup) encoding of
/// - the term property shape
/// - the prefix map
/// - the label map
/// as a JSON object
///
/// Examples:
///
/// Let ldb be an LDTab database containing informmation about the subject "obo:ZFA_0000354"
/// in the table statement, then get_subject_map(&subject , &table, &pool) returns
///
///  {"obo:ZFA_0000354":
///    {
///      "oboInOwl:id":[{"object":"ZFA:0000354","datatype":"xsd:string"}],
///      "rdf:type":[{"object":"owl:Class","datatype":"_IRI"}],
///      "rdfs:label":[{"object":"gill","datatype":"xsd:string"}],
///      "oboInOwl:hasOBONamespace":[{"object":"zebrafish_anatomy","datatype":"xsd:string"}]
///    }
///  
///   "@labels":
///     {
///       "obo:ZFA_0000354":"gill"
///     },
///  
///   "@prefixes":
///     {
///       "owl":"http://www.w3.org/2002/07/owl#",
///       "rdf":"http://www.w3.org/1999/02/22-rdf-syntax-ns#",
///       "rdfs":"http://www.w3.org/2000/01/rdf-schema#",
///       "obo":"http://purl.obolibrary.org/obo/"
///     }
///    }
pub async fn get_subject_map(subject: &str, table: &str, pool: &AnyPool) -> Result<Value, Error> {
    //1. subject map
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::any::{AnyPool, AnyPoolOptions};
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
        let pool: AnyPool = AnyPoolOptions::new()
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
        let pool: AnyPool = AnyPoolOptions::new()
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
        let pool: AnyPool = AnyPoolOptions::new()
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
        let pool: AnyPool = AnyPoolOptions::new()
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

    #[test]
    fn test_build_type_query_for() {
        let mut curies = HashSet::new();
        curies.insert(String::from("obo:ZFA_0000354"));
        curies.insert(String::from("rdfs:label"));

        let query = build_type_query_for(&curies, "statement");

        //NB: the order of arguments for the SQLite IN operator is not unique
        let expected_alternative_a = "SELECT subject, predicate, object FROM statement WHERE subject IN ('rdfs:label','obo:ZFA_0000354') AND predicate='rdf:type'";
        let expected_alternative_b = "SELECT subject, predicate, object FROM statement WHERE subject IN ('obo:ZFA_0000354','rdfs:label') AND predicate='rdf:type'";

        let check = query.eq(&expected_alternative_a) || query.eq(&expected_alternative_b);
        assert!(check);
    }

    #[tokio::test]
    async fn test_get_type_hash_map() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: AnyPool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let table = "statement";

        let mut curies = HashSet::new();
        curies.insert(String::from("obo:ZFA_0000354"));

        let type_hash_map = get_type_hash_map(&curies, &table, &pool).await.unwrap();

        let mut expected = HashMap::new();
        let mut types: HashSet<String> = HashSet::new();
        types.insert(String::from("owl:Class"));
        expected.insert(String::from("obo:ZFA_0000354"), types);

        assert_eq!(type_hash_map, expected);
    }

    #[tokio::test]
    async fn test_get_property_map() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: AnyPool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let table = "statement";

        let property_map = get_property_map("obo:ZFA_0000354", &table, &pool)
            .await
            .unwrap();
        let expected = json!({"obo:IAO_0000115":[{"object":"Compound organ that consists of gill filaments, gill lamellae, gill rakers and pharyngeal arches 3-7. The gills are responsible for primary gas exchange between the blood and the surrounding water.","datatype":"xsd:string","annotation":{"oboInOwl:hasDbXref":[{"datatype":"xsd:string","meta":"owl:Axiom","object":"http:http://www.briancoad.com/Dictionary/DicPics/gill.htm"}]}}],"oboInOwl:hasDbXref":[{"object":"TAO:0000354","datatype":"xsd:string"}],"rdfs:subClassOf":[{"object":{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0000272"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]},"datatype":"_JSON"},{"object":{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002497"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000044"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]},"datatype":"_JSON"},{"object":{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002202"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0001107"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]},"datatype":"_JSON"},{"object":"obo:ZFA_0000496","datatype":"_IRI"},{"object":{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000000"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]},"datatype":"_JSON"}],"oboInOwl:id":[{"object":"ZFA:0000354","datatype":"xsd:string"}],"rdf:type":[{"object":"owl:Class","datatype":"_IRI"}],"rdfs:label":[{"object":"gill","datatype":"xsd:string"}],"oboInOwl:hasOBONamespace":[{"object":"zebrafish_anatomy","datatype":"xsd:string"}],"oboInOwl:hasExactSynonym":[{"object":"gills","datatype":"xsd:string","annotation":{"oboInOwl:hasSynonymType":[{"datatype":"_IRI","meta":"owl:Axiom","object":"obo:zfa#PLURAL"}]}}]});

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

    #[test]
    fn test_ldtab_json_2_hiccup() {
        let mut label_map = HashMap::new();
        label_map.insert(
            String::from("obo:ZFA_0000272"),
            String::from("respiratory system"),
        );

        let mut type_map: HashMap<String, HashSet<String>> = HashMap::new();
        let mut types = HashSet::new();
        types.insert(String::from("owl:Class"));
        type_map.insert(String::from("obo:ZFA_0000272"), types);

        let hiccup = ldtab_json_2_hiccup(
            "rdfs:subClassOf",
            &json!({"object":{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0000272"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]},"datatype":"_JSON"}),
            &label_map,
            &type_map,
        );

        let expected = json!(["span",{"property":"rdfs:subClassOf","typeof":"owl:Restriction"},["a",{"property":"owl:onProperty","resource":"obo:BFO_0000050"},"obo:BFO_0000050"]," some ",["a",{"property":"owl:someValuesFrom","resource":"obo:ZFA_0000272"},"respiratory system"]]);
        assert_eq!(hiccup, expected);
    }

    #[tokio::test]
    async fn test_ldtab_row_2_predicate_json_shape() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: AnyPool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let query = "SELECT * FROM statement WHERE predicate='rdfs:label'";
        let rows: Vec<AnyRow> = sqlx::query(&query).fetch_all(&pool).await.unwrap();
        let row = &rows[0]; //NB: there is a unique row (with rdfs:label)
        let json_shape = ldtab_row_2_predicate_json_shape(row);
        let expected = (
            String::from("rdfs:label"),
            json!({"object":"gill","datatype":"xsd:string"}),
        );
        assert_eq!(json_shape, expected);
    }

    #[tokio::test]
    async fn test_sort_predicate_map_by_label() {
        let connection = "src/resources/test_data/zfa_excerpt.db";
        let connection_string = format!("sqlite://{}?mode=rwc", connection);
        let pool: AnyPool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .unwrap();

        let subject = "obo:ZFA_0000354";
        let table = "statement";

        let property_map = get_property_map(&subject, &table, &pool).await.unwrap();

        let mut label_map = HashMap::new();
        label_map.insert(String::from("rdf:type"), String::from("type"));
        label_map.insert(String::from("rdfs:label"), String::from("label"));
        label_map.insert(String::from("rdfs:subClassOf"), String::from("subsumption"));
        label_map.insert(String::from("obo:IAO_0000115"), String::from("definition"));

        let starting_order = vec![String::from("rdfs:label"), String::from("obo:IAO_0000115")];
        let ending_order = vec![String::from("rdfs:subClassOf")];

        let (a, _b) =
            sort_predicate_map_by_label(&property_map, &label_map, &starting_order, &ending_order);
        let expected_ordrer = vec![
            "rdfs:label",
            "obo:IAO_0000115",
            "oboInOwl:hasDbXref",
            "oboInOwl:hasExactSynonym",
            "oboInOwl:hasOBONamespace",
            "oboInOwl:id",
            "rdf:type",
            "rdfs:subClassOf",
        ];

        assert_eq!(a, expected_ordrer);
    }
}
