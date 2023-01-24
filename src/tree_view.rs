use serde_json::{from_str, json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

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

pub fn check_has_part_property(value: &Value) -> bool {
    match value {
        Value::Object(x) => {
            let property = x.get("object").unwrap();
            let has_part = json!("obo:BFO_0000051"); //'has part' relation
            property.eq(&has_part)
        }
        _ => false,
    }
}

pub fn check_filler(value: &Value) -> bool {
    match value {
        Value::Object(x) => {
            let filler = x.get("object").unwrap();
            match filler {
                Value::String(_x) => true,
                _ => false,
            }
        }
        _ => false,
    }
}

pub fn check_has_part_restriction(value: &Map<String, Value>) -> bool {
    if value.contains_key("owl:onProperty")
        & value.contains_key("owl:someValuesFrom")
        & value.contains_key("rdf:type")
    {
        let property = value.get("owl:onProperty").unwrap().as_array().unwrap()[0].clone();
        let filler = value.get("owl:someValuesFrom").unwrap().as_array().unwrap()[0].clone();
        //let rdf_type = value.get("rdf:type").unwrap().as_array().unwrap()[0]; //not necessary

        check_has_part_property(&property) & check_filler(&filler)
    } else {
        false
    }
}

pub fn remove_invalid_class(
    target: &str,
    class_2_subclasses: &mut HashMap<String, HashSet<String>>,
) {
    let values = class_2_subclasses.remove(target).unwrap();
    for (_key, value) in class_2_subclasses {
        if value.contains(target) {
            value.remove(target);
            for v in &values {
                value.insert(v.clone());
            }
        }
    }
}

//TODO: doc string + doc test
//we want to have a tree view that only displays named classes and existential restrictions
//using hasPart. So, we need to filter out all unwanted class expressions, e.g., intersections,
//unions, etc.
pub fn remove_invalid_classes(class_2_subclasses: &mut HashMap<String, HashSet<String>>) {
    let mut invalid: HashSet<String> = HashSet::new();
    for k in class_2_subclasses.keys() {
        //NB: an LDTab thick triple makes use of strings (which are not JSON strings
        //example: "this is a string" and "\"this is a JSON string\"".).
        let key_value = match from_str::<Value>(k) {
            Ok(x) => x,
            _ => json!(k),
        };

        let valid = match key_value {
            Value::String(_x) => true,
            Value::Object(x) => check_has_part_restriction(&x),
            _ => false,
        };

        if !valid {
            invalid.insert(k.clone());
        }
    }

    for k in invalid {
        remove_invalid_class(&k, class_2_subclasses);
    }
}

//given a set of root-classes and a map from classes to subclasses,
//return a JSON encoding
//TODO: example/doc test
pub fn get_json_tree(
    level: &HashSet<String>,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> Value {
    let mut map = Map::new();
    for element in level {
        let element_string = match from_str::<Value>(element) {
            Ok(x) => {
                let get_first = x.get("owl:someValuesFrom").unwrap().as_array().unwrap()[0].clone();
                let object = get_first.get("object").unwrap();
                let filler = object.as_str().unwrap();
                format!("hasPart {}", filler)
            }
            _ => String::from(element),
        };

        match class_2_subclasses.get(element) {
            Some(subs) => {
                let v = get_json_tree(subs, class_2_subclasses);
                map.insert(element_string, v);
            }
            None => {
                let v = json!("owl:Nothing");
                map.insert(element_string, v);
            }
        }
    }

    Value::Object(map)
}

pub async fn get_json_tree_view(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, Error> {
    //create existential restriction with hasPart
    //NB: we are relying on LDTab triples being sorted for string comparisons
    let has_part_subject = r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000051"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"entity"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#;
    let has_part_subject = has_part_subject.replace("entity", entity);

    //recursive query computing the transitive closure of rdfs:subClassOf in an LDTab database
    let query = format!("WITH RECURSIVE
    superclasses( subject, object ) AS
    ( SELECT subject, object FROM {table} WHERE subject='{entity}' AND predicate='rdfs:subClassOf'
        UNION ALL
       SELECT subject, object FROM {table} WHERE subject='{has_part}' AND predicate='rdfs:subClassOf'
        UNION ALL
        SELECT {table}.subject, {table}.object FROM {table}, superclasses WHERE {table}.subject = superclasses.object AND {table}.predicate='rdfs:subClassOf'
     ),
    subclasses( subject, object ) AS
    ( SELECT subject, object FROM {table} WHERE object='{entity}' AND predicate='rdfs:subClassOf'
        UNION ALL
       SELECT subject, object FROM {table} WHERE object='{has_part}' AND predicate='rdfs:subClassOf'
        UNION ALL
        SELECT {table}.subject, {table}.object FROM {table}, subclasses WHERE {table}.object = subclasses.subject AND {table}.predicate='rdfs:subClassOf'
     )
  SELECT * FROM superclasses
  UNION ALL 
  SELECT * FROM subclasses;", table=table, entity=entity, has_part=has_part_subject);

    get_json_tree_view_by_query(pool, &query).await
}

pub async fn get_json_tree_view_by_query(pool: &SqlitePool, query: &str) -> Result<Value, Error> {
    let rows: Vec<SqliteRow> = sqlx::query(query).fetch_all(pool).await?;

    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();
    let mut classes: HashSet<String> = HashSet::new();
    //a class A is not a root in the tree/forest to be constructed,
    //if there exist an axiom of the form 'A rdfs:subClassOf B',
    let mut non_roots: HashSet<String> = HashSet::new();
    //given all non-root classes,
    //root classes can be identified by the set differences of all classes and non-root classes
    let mut roots: HashSet<String> = HashSet::new();

    for row in rows {
        //axiom structure: subject rdfs:subClassOf object
        let subject: &str = row.get("subject");
        let object: &str = row.get("object");

        let subject_string = String::from(subject);
        let object_string = String::from(object);

        //collect classes
        classes.insert(subject_string.clone());
        classes.insert(object_string.clone());
        //identify non-root classes
        non_roots.insert(subject_string.clone());

        //add subclass information into class_2_subclasses map
        match class_2_subclasses.get_mut(&object_string) {
            Some(x) => {
                x.insert(subject_string);
            }
            None => {
                let mut subclasses = HashSet::new();
                subclasses.insert(subject_string);
                class_2_subclasses.insert(object_string, subclasses);
            }
        }
    }

    //we want to have a tree view that only displays named classes and existential restrictions
    //using hasPart. So, we need to filter out all unwanted class expressions, e.g., intersections,
    //unions, etc.
    remove_invalid_classes(&mut class_2_subclasses);

    //identify 'valid' roots ('valid' being understood as defined above)
    let keys: HashSet<String> = class_2_subclasses.clone().into_keys().collect();
    for c in classes {
        //check all collected classes
        if !non_roots.contains(&c) & keys.contains(&c) {
            //set difference + validity check
            roots.insert(c.clone());
        }
    }

    let json_view = get_json_tree(&roots, &class_2_subclasses);

    Ok(json_view)
}
