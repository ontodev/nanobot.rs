use async_recursion::async_recursion;
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

pub fn ldtab_2_value(input: &str) -> Value {
    match from_str::<Value>(input) {
        Ok(x) => x,
        _ => json!(input),
    }
}

// ################################################
// ######## build label map ######################
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

pub fn get_iris_from_string(s: &str) -> HashSet<String> {
    let mut iris: HashSet<String> = HashSet::new();

    let value = ldtab_2_value(&s);
    match value {
        Value::String(x) => {
            iris.insert(x);
        }
        _ => {
            signature::get_iris(&value, &mut iris);
        }
    }
    iris
}

pub fn get_iris_from_subclass_map(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut iris: HashSet<String> = HashSet::new();
    for (k, v) in class_2_subclasses {
        iris.extend(get_iris_from_string(&k));
        for vv in v {
            iris.extend(get_iris_from_string(&vv));
        }
    }
    iris
}

pub fn get_iris_from_set(set: &HashSet<String>) -> HashSet<String> {
    let mut iris: HashSet<String> = HashSet::new();
    for e in set {
        iris.extend(get_iris_from_string(&e));
    }
    iris
}

// ################################################
// ######## build tree view #######################
// ################################################

pub fn check_part_of_property(value: &Value) -> bool {
    match value {
        Value::Object(x) => {
            let property = x.get("object").unwrap();
            let part_of = json!("obo:BFO_0000050"); //'part of' relation
            property.eq(&part_of)
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

pub fn check_part_of_restriction(value: &Map<String, Value>) -> bool {
    if value.contains_key("owl:onProperty")
        & value.contains_key("owl:someValuesFrom")
        & value.contains_key("rdf:type")
    {
        let property = value.get("owl:onProperty").unwrap().as_array().unwrap()[0].clone();
        let filler = value.get("owl:someValuesFrom").unwrap().as_array().unwrap()[0].clone();
        //let rdf_type = value.get("rdf:type").unwrap().as_array().unwrap()[0]; //not necessary

        check_part_of_property(&property) & check_filler(&filler)
    } else {
        false
    }
}

pub fn get_class_2_superclasses(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> HashMap<String, HashSet<String>> {
    let mut class_2_superclasses: HashMap<String, HashSet<String>> = HashMap::new();
    for (key, value) in class_2_subclasses {
        for v in value {
            match class_2_superclasses.get_mut(v) {
                Some(x) => {
                    x.insert(key.clone());
                }
                None => {
                    let mut superclasses = HashSet::new();
                    superclasses.insert(key.clone());
                    class_2_superclasses.insert(v.clone(), superclasses);
                }
            }
        }
    }
    class_2_superclasses
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
        let key_value = ldtab_2_value(k);

        let valid = match key_value {
            Value::String(_x) => true,
            Value::Object(x) => check_part_of_restriction(&x),
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

pub fn reachable(
    start: &str,
    end: &str,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> bool {
    if start.eq(end) {
        return true;
    }
    match class_2_subclasses.get(start) {
        Some(x) => {
            if x.contains(end) {
                true
            } else {
                let mut reachable_it = false;
                for next in x {
                    reachable_it = reachable_it | reachable(next, end, class_2_subclasses);
                }
                reachable_it
            }
        }
        None => false,
    }
}

//given a set of root-classes and a map from classes to subclasses,
//return a JSON encoding
//TODO: example/doc test
pub fn get_json_superclass_tree(
    entity: &str,
    level: &HashSet<String>,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> Value {
    let mut map = Map::new();
    for element in level {
        if !reachable(element, entity, class_2_subclasses) {
            continue;
        }

        let element_string = match from_str::<Value>(element) {
            Ok(x) => {
                let get_first = x.get("owl:someValuesFrom").unwrap().as_array().unwrap()[0].clone();
                let object = get_first.get("object").unwrap();
                let filler = object.as_str().unwrap();
                format!("partOf {}", filler)
            }
            _ => String::from(element),
        };

        match class_2_subclasses.get(element) {
            Some(subs) => {
                let v = get_json_superclass_tree(entity, subs, class_2_subclasses);
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

pub async fn get_sub_parts_of(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, Error> {
    let mut sub_parts = HashSet::new();
    let part_of = r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"entity"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#;
    let part_of = part_of.replace("entity", entity);

    let query = format!(
        "SELECT subject FROM {table} WHERE object='{part_of}' AND predicate='rdfs:subClassOf'",
        table = table,
        part_of = part_of
    );
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let subject: &str = row.get("subject");
        sub_parts.insert(String::from(subject));
    }

    Ok(sub_parts)
}

pub async fn get_subclasses(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, Error> {
    let mut subclasses = HashSet::new();
    let query = format!(
        "SELECT subject FROM {table} WHERE object='{entity}' AND predicate='rdfs:subClassOf'",
        table = table,
        entity = entity
    );
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let subject: &str = row.get("subject");
        subclasses.insert(String::from(subject));
    }

    Ok(subclasses)
}

#[async_recursion]
pub async fn get_class_map(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashMap<String, HashSet<String>>, Error> {
    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();

    let query = format!("WITH RECURSIVE
    superclasses( subject, object ) AS
    ( SELECT subject, object FROM {table} WHERE subject='{entity}' AND predicate='rdfs:subClassOf'
        UNION ALL
        SELECT {table}.subject, {table}.object FROM {table}, superclasses WHERE {table}.subject = superclasses.object AND {table}.predicate='rdfs:subClassOf'
     ) SELECT * FROM superclasses;", table=table, entity=entity);

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    let mut part_of_link: HashSet<String> = HashSet::new();

    for row in rows {
        //axiom structure: subject rdfs:subClassOf object
        let subject: &str = row.get("subject");
        let object: &str = row.get("object");

        let object_value = ldtab_2_value(object);

        let part_of_restriction = match object_value.clone() {
            Value::Object(x) => check_part_of_restriction(&x),
            _ => false,
        };

        if part_of_restriction {
            //NB: these unwraps are safe because we checked them before
            let part_of_filler = object_value
                .get("owl:someValuesFrom")
                .unwrap()
                .as_array()
                .unwrap()[0]
                .clone();
            let part_of_filler = part_of_filler.get("object").unwrap();
            let part_of_filler_string = String::from(part_of_filler.as_str().unwrap());
            part_of_link.insert(part_of_filler_string);
        }

        let subject_string = String::from(subject);
        let object_string = String::from(object);

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

    for link in part_of_link {
        let part_of_class_map = get_class_map(&link, table, pool).await;
        match part_of_class_map {
            Ok(x) => {
                for (key, value) in x {
                    match class_2_subclasses.get_mut(&key) {
                        Some(x) => {
                            x.extend(value.clone());
                        }
                        None => {
                            class_2_subclasses.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            Err(x) => return Err(x),
        }
    }

    Ok(class_2_subclasses)
}

pub async fn get_json_tree_view(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, Error> {
    //a class A is not a root in the tree/forest to be constructed,
    //if there exist an axiom of the form 'A rdfs:subClassOf B',
    let mut non_roots: HashSet<String> = HashSet::new();
    //given all non-root classes,
    //root classes can be identified by the set differences of all classes and non-root classes
    let mut roots: HashSet<String> = HashSet::new();

    let mut part_of_fillers: HashSet<(String, String)> = HashSet::new();

    //get map from classes to their subclasses (including the 'part of' hierarchy)
    let mut class_2_subclasses = match get_class_map(entity, table, pool).await {
        Ok(x) => x,
        Err(x) => return Err(x),
    };

    //we want to have a tree view that only displays named classes and existential restrictions
    //using hasPart. So, we need to filter out all unwanted class expressions, e.g., intersections,
    //unions, etc.
    remove_invalid_classes(&mut class_2_subclasses);

    let class_2_superclasses = get_class_2_superclasses(&class_2_subclasses);

    for (key, value) in &class_2_subclasses {
        //identify non_roots
        non_roots.extend(value.clone());

        let object_value = ldtab_2_value(key);

        let part_of_restriction = match object_value.clone() {
            Value::Object(x) => check_part_of_restriction(&x),
            _ => false,
        };

        if part_of_restriction {
            //NB: these unwraps are safe because we checked them before
            let part_of_filler = object_value
                .get("owl:someValuesFrom")
                .unwrap()
                .as_array()
                .unwrap()[0]
                .clone();
            let part_of_filler = part_of_filler.get("object").unwrap();
            let part_of_filler_string = String::from(part_of_filler.as_str().unwrap());
            if class_2_superclasses.contains_key(&part_of_filler_string) {
                //TODO: errror
                non_roots.insert(key.clone());
            }
            //collect information about part-of hierarchy: (filler,restriction)
            part_of_fillers.insert((part_of_filler_string.clone(), key.clone()));
        }
    }

    //add part-of hierarchy to subclass map
    for (filler, restriction) in part_of_fillers {
        match class_2_superclasses.get(&filler) {
            Some(x) => {
                for superclass in x {
                    match class_2_subclasses.get_mut(superclass) {
                        Some(y) => {
                            y.insert(restriction.clone());
                        }
                        None => {}
                    }
                }
            }
            None => {}
        }
    }

    //initialise roots
    for (key, _value) in &class_2_subclasses {
        if !non_roots.contains(key) {
            roots.insert(key.clone());
        }
    }

    let subclasses = get_subclasses(entity, table, pool).await;
    let sub_parts = get_sub_parts_of(entity, table, pool).await;
    //TODO: get second level

    let json_view = get_json_superclass_tree(entity, &roots, &class_2_subclasses);

    //TODO: combine tree view and label map
    let iris = get_iris_from_subclass_map(&class_2_subclasses);
    let iris_2 = get_iris_from_set(&sub_parts.as_ref().unwrap());
    let label_map = get_label_map(&iris, table, pool).await;

    Ok(json_view)
}
