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

pub fn remove_invalid_classes(
    class_2_subclasses: &mut HashMap<String, HashSet<String>>,
    invalid: &HashSet<String>,
) {
    //remove invalid keys
    for i in invalid {
        class_2_subclasses.remove(i);
    }
    //remove invalid parts in values
    for (_k, v) in class_2_subclasses.iter_mut() {
        for i in invalid {
            v.remove(i);
        }
    }
}

//TODO: doc string + doc test
//we want to have a tree view that only displays named classes and existential restrictions
//using hasPart. So, we need to filter out all unwanted class expressions, e.g., intersections,
//unions, etc.
pub fn identify_invalid_classes(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut invalid: HashSet<String> = HashSet::new();
    for (k, v) in class_2_subclasses {
        let key_value = ldtab_2_value(k);

        let valid = match key_value {
            Value::String(_x) => true,
            _ => false,
        };

        if !valid {
            //collect keys to remove
            invalid.insert(k.clone());
        } else {
            for sub in v.clone() {
                //TODO: is it necessary to clone the set here?
                let sub_value = ldtab_2_value(&sub);
                let valid = match sub_value {
                    Value::String(_x) => true,
                    _ => false,
                };
                if !valid {
                    invalid.insert(String::from(sub));
                }
            }
        }
    }
    invalid
}

pub fn update_hierarchy_map(
    to_update: &mut HashMap<String, HashSet<String>>,
    updates: &HashMap<String, HashSet<String>>,
) {
    for (class, subclasses) in updates {
        match to_update.get_mut(class) {
            Some(x) => {
                for sub in subclasses {
                    if !x.contains(sub) {
                        x.insert(sub.clone());
                    }
                }
            }
            None => {
                to_update.insert(class.clone(), subclasses.clone());
            }
        }
    }
}

pub fn identify_roots(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut roots = HashSet::new();

    let mut keys = HashSet::new();
    let mut values = HashSet::new();

    for (k, v) in class_2_subclasses {
        keys.insert(k);
        values.extend(v);
    }
    for (k, v) in class_2_parts {
        keys.insert(k);
        values.extend(v);
    }

    for k in keys {
        if !values.contains(k) {
            roots.insert(k.clone());
        }
    }
    roots
}

pub async fn get_hierarchy_maps(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<
    (
        HashMap<String, HashSet<String>>,
        HashMap<String, HashSet<String>>,
    ),
    Error,
> {
    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();
    let mut class_2_parts: HashMap<String, HashSet<String>> = HashMap::new();

    let mut updates = HashSet::new();
    updates.insert(String::from(entity));

    while !updates.is_empty() {
        let mut new_parts: HashSet<String> = HashSet::new();

        for update in &updates {
            let subclasses_updates = get_class_2_subclass_map(&update, table, pool)
                .await
                .unwrap();
            update_hierarchy_map(&mut class_2_subclasses, &subclasses_updates);

            let parts_updates = get_part_of_information(&subclasses_updates).unwrap();
            update_hierarchy_map(&mut class_2_parts, &parts_updates);

            for part in parts_updates.keys() {
                if !class_2_subclasses.contains_key(part) {
                    new_parts.insert(part.clone());
                }
            }
        }

        updates.clear();
        for new in new_parts {
            updates.insert(new.clone());
        }
    }

    let invalid = identify_invalid_classes(&class_2_subclasses);
    remove_invalid_classes(&mut class_2_subclasses, &invalid);

    Ok((class_2_subclasses, class_2_parts))
}

pub async fn get_class_2_subclass_map(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashMap<String, HashSet<String>>, Error> {
    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();

    //recursive query handles 'is-a' hierarchy
    let query = format!("WITH RECURSIVE
    superclasses( subject, object ) AS
    ( SELECT subject, object FROM {table} WHERE subject='{entity}' AND predicate='rdfs:subClassOf'
        UNION ALL
        SELECT {table}.subject, {table}.object FROM {table}, superclasses WHERE {table}.subject = superclasses.object AND {table}.predicate='rdfs:subClassOf'
     ) SELECT * FROM superclasses;", table=table, entity=entity);

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    for row in rows {
        //axiom structure: subject rdfs:subClassOf object
        let subject: &str = row.get("subject"); //subclass
        let object: &str = row.get("object"); //superclass

        let subject_string = String::from(subject);
        let object_string = String::from(object);

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
    Ok(class_2_subclasses)
}

pub fn get_part_of_information(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> Result<HashMap<String, HashSet<String>>, Error> {
    let mut class_2_parts: HashMap<String, HashSet<String>> = HashMap::new();

    //S subclassof part-of some filler
    //map will hold: filler -> S  (read: filler has-part S)
    for (class, subclasses) in class_2_subclasses {
        let class_value = ldtab_2_value(class);

        let part_of_restriction = match class_value.clone() {
            Value::Object(x) => check_part_of_restriction(&x),
            _ => false,
        };

        if part_of_restriction {
            let part_of_filler = class_value
                .get("owl:someValuesFrom")
                .unwrap()
                .as_array()
                .unwrap()[0]
                .clone();

            let part_of_filler = part_of_filler.get("object").unwrap();
            let part_of_filler_string = String::from(part_of_filler.as_str().unwrap());

            for subclass in subclasses {
                match class_2_parts.get_mut(part_of_filler.as_str().unwrap()) {
                    Some(x) => {
                        x.insert(subclass.clone());
                    }
                    None => {
                        let mut subclasses = HashSet::new();
                        subclasses.insert(subclass.clone());
                        class_2_parts.insert(part_of_filler_string.clone(), subclasses);
                    }
                }
            }
        }
    }
    Ok(class_2_parts)
}

pub fn build_is_a_branch(
    to_insert: &str,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
) -> Value {
    let mut json_map = Map::new();

    match class_2_subclasses.get(to_insert) {
        Some(is_a_children) => {
            for c in is_a_children {
                match build_is_a_branch(c, class_2_subclasses, class_2_parts) {
                    Value::Object(x) => {
                        json_map.extend(x);
                    }
                    _ => {}
                }
            }
        }
        None => {}
    }
    match class_2_parts.get(to_insert) {
        Some(part_of_children) => {
            for c in part_of_children {
                match build_part_of_branch(c, class_2_subclasses, class_2_parts) {
                    Value::Object(x) => {
                        json_map.extend(x);
                    }
                    _ => {}
                }
            }
        }
        None => {}
    }

    //leaf case
    if !class_2_subclasses.contains_key(to_insert) & !class_2_parts.contains_key(to_insert) {
        json_map.insert(
            String::from(to_insert),
            Value::String(String::from("owl:Nothing")),
        );
        Value::Object(json_map)
    } else {
        json!({ to_insert: json_map })
    }
}

pub fn build_part_of_branch(
    to_insert: &str,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
) -> Value {
    let mut json_map = Map::new();

    match class_2_subclasses.get(to_insert) {
        Some(is_a_children) => {
            for c in is_a_children {
                match build_is_a_branch(c, class_2_subclasses, class_2_parts) {
                    Value::Object(x) => {
                        json_map.extend(x);
                    }
                    _ => {}
                }
            }
        }
        None => {}
    }
    match class_2_parts.get(to_insert) {
        Some(part_of_children) => {
            for c in part_of_children {
                match build_part_of_branch(c, class_2_subclasses, class_2_parts) {
                    Value::Object(x) => {
                        json_map.extend(x);
                    }
                    _ => {}
                }
            }
        }
        None => {}
    }

    //leaf case
    if !class_2_subclasses.contains_key(to_insert) & !class_2_parts.contains_key(to_insert) {
        json_map.insert(
            format!("partOf {}", to_insert),
            Value::String(String::from("owl:Nothing")),
        );
        Value::Object(json_map)
    } else {
        json!({ format!("partOf {}", to_insert): json_map })
    }
}

pub fn build_tree(
    to_insert: &HashSet<String>,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
) -> Value {
    let mut json_map = Map::new();
    for i in to_insert {
        if class_2_subclasses.contains_key(i) {
            match build_is_a_branch(i, class_2_subclasses, class_2_parts) {
                Value::Object(x) => {
                    json_map.extend(x);
                }
                _ => {}
            }
        }
        if class_2_parts.contains_key(i) {
            match build_part_of_branch(i, class_2_subclasses, class_2_parts) {
                Value::Object(x) => {
                    json_map.extend(x);
                }
                _ => {}
            }
        }

        //leaf case
        if !class_2_subclasses.contains_key(i) & !class_2_parts.contains_key(i) {
            json_map.insert(String::from(i), Value::String(String::from("owl:Nothing")));
        }
    }
    Value::Object(json_map)
}

pub async fn get_json_tree_view(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    let (class_2_subclasses, class_2_parts) =
        get_hierarchy_maps(entity, table, &pool).await.unwrap();
    let roots = identify_roots(&class_2_subclasses, &class_2_parts);
    build_tree(&roots, &class_2_subclasses, &class_2_parts)
}

pub fn build_labelled_tree(tree: &Value, label_map: &HashMap<String, String>) -> Value {
    let mut json_map = Map::new();

    match tree {
        Value::Object(x) => {
            for (k, v) in x {
                if k.starts_with("partOf ") {
                    let curie = k.strip_prefix("partOf ").unwrap();
                    match label_map.get(curie) {
                        Some(label) => {
                            json_map.insert(
                                format!("partOf {}", label),
                                build_labelled_tree(v, label_map),
                            );
                        }
                        None => {
                            json_map.insert(k.clone(), build_labelled_tree(v, label_map));
                        }
                    }
                } else {
                    match label_map.get(k) {
                        Some(label) => {
                            json_map.insert(label.clone(), build_labelled_tree(v, label_map));
                        }
                        None => {
                            json_map.insert(k.clone(), build_labelled_tree(v, label_map));
                        }
                    }
                }
            }
        }
        Value::String(x) => {
            return Value::String(x.clone());
        }
        _ => {
            json!("ERROR");
        }
    }
    Value::Object(json_map)
}

pub async fn get_labelled_json_tree_view(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    let (class_2_subclasses, class_2_parts) =
        get_hierarchy_maps(entity, table, &pool).await.unwrap();
    let roots = identify_roots(&class_2_subclasses, &class_2_parts);

    let mut iris = HashSet::new();
    iris.extend(get_iris_from_subclass_map(&class_2_subclasses));
    iris.extend(get_iris_from_subclass_map(&class_2_parts));

    let label_hash_map = get_label_hash_map(&iris, table, pool).await;
    let tree = build_tree(&roots, &class_2_subclasses, &class_2_parts);

    build_labelled_tree(&tree, &label_hash_map)
}

//#######################                   #######################
//#######################  Rich JSON format #######################
//#######################                   #######################
pub fn build_rich_is_a_branch(
    to_insert: &str,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
    curie_2_label: &HashMap<String, String>,
) -> Value {
    let mut children_vec: Vec<Value> = Vec::new();

    match class_2_subclasses.get(to_insert) {
        Some(is_a_children) => {
            for c in is_a_children {
                match build_rich_is_a_branch(c, class_2_subclasses, class_2_parts, curie_2_label) {
                    Value::Object(x) => {
                        //json_map.extend(x);
                        children_vec.push(Value::Object(x));
                    }
                    _ => {}
                }
            }
        }
        None => {}
    }
    match class_2_parts.get(to_insert) {
        Some(part_of_children) => {
            for c in part_of_children {
                match build_rich_part_of_branch(c, class_2_subclasses, class_2_parts, curie_2_label)
                {
                    Value::Object(x) => {
                        //json_map.extend(x);
                        children_vec.push(Value::Object(x));
                    }
                    _ => {}
                }
            }
        }
        None => {}
    }

    //leaf case
    if !class_2_subclasses.contains_key(to_insert) & !class_2_parts.contains_key(to_insert) {
        children_vec.push(json!("owl:Nothing"));
        //json_map.insert(
        //    String::from(to_insert),
        //    Value::String(String::from("owl:Nothing")),
        //);
        //Value::Object(json_map)
    }

    json!({"curie" : to_insert, "label" : curie_2_label.get(to_insert), "property" : "is-a", "children" : children_vec})
}

pub fn build_rich_part_of_branch(
    to_insert: &str,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
    curie_2_label: &HashMap<String, String>,
) -> Value {
    let mut children_vec: Vec<Value> = Vec::new();

    match class_2_subclasses.get(to_insert) {
        Some(is_a_children) => {
            for c in is_a_children {
                match build_rich_is_a_branch(c, class_2_subclasses, class_2_parts, curie_2_label) {
                    Value::Object(x) => {
                        //json_map.extend(x);
                        children_vec.push(Value::Object(x));
                    }
                    _ => {}
                }
            }
        }
        None => {}
    }
    match class_2_parts.get(to_insert) {
        Some(part_of_children) => {
            for c in part_of_children {
                match build_rich_part_of_branch(c, class_2_subclasses, class_2_parts, curie_2_label)
                {
                    Value::Object(x) => {
                        //json_map.extend(x);
                        children_vec.push(Value::Object(x));
                    }
                    _ => {}
                }
            }
        }
        None => {}
    }

    //leaf case
    if !class_2_subclasses.contains_key(to_insert) & !class_2_parts.contains_key(to_insert) {
        children_vec.push(json!("owl:Nothing"));
        //json_map.insert(
        //    format!("partOf {}", to_insert),
        //    Value::String(String::from("owl:Nothing")),
        //);
        //Value::Object(json_map);
    }

    json!({"curie" : to_insert, "label" : curie_2_label.get(to_insert), "property" : "part-of", "children" : children_vec})
}

pub fn build_rich_tree(
    to_insert: &HashSet<String>,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
    curie_2_label: &HashMap<String, String>,
) -> Value {
    let mut json_vec: Vec<Value> = Vec::new();

    for i in to_insert {
        if class_2_subclasses.contains_key(i) {
            match build_rich_is_a_branch(i, class_2_subclasses, class_2_parts, curie_2_label) {
                Value::Object(x) => {
                    json_vec.push(Value::Object(x));
                }
                _ => {}
            }
        }
        if class_2_parts.contains_key(i) {
            match build_rich_part_of_branch(i, class_2_subclasses, class_2_parts, curie_2_label) {
                Value::Object(x) => {
                    json_vec.push(Value::Object(x));
                }
                _ => {}
            }
        }

        //leaf case
        if !class_2_subclasses.contains_key(i) & !class_2_parts.contains_key(i) {
            json_vec.push(json!({String::from(i) : Value::String(String::from("owl:Nothing"))}));
        }
    }
    Value::Array(json_vec)
}

pub async fn get_rich_json_tree_view(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    let (mut class_2_subclasses, mut class_2_parts) =
        get_hierarchy_maps(entity, table, &pool).await.unwrap();

    let direct_subclasses = get_direct_sub_hierarchy_maps(entity, table, pool)
        .await
        .unwrap();
    let direct_part_ofs = get_direct_sub_parts(entity, table, pool).await.unwrap();

    class_2_subclasses.insert(String::from(entity), direct_subclasses);
    class_2_parts.insert(String::from(entity), direct_part_ofs);

    let roots = identify_roots(&class_2_subclasses, &class_2_parts);

    let mut iris = HashSet::new();
    iris.extend(get_iris_from_subclass_map(&class_2_subclasses));
    iris.extend(get_iris_from_subclass_map(&class_2_parts));

    let curie_2_label = get_label_hash_map(&iris, table, pool).await;

    build_rich_tree(&roots, &class_2_subclasses, &class_2_parts, &curie_2_label)
}

pub async fn get_direct_sub_hierarchy_maps(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, Error> {
    let subclasses = get_direct_subclasses(entity, table, pool).await.unwrap();

    let mut is_a: HashSet<String> = HashSet::new();

    for s in subclasses {
        //filter for named classes
        match ldtab_2_value(&s) {
            Value::String(_x) => {
                is_a.insert(s.clone());
            }
            _ => {}
        };
    }
    Ok(is_a)
}

pub async fn get_direct_subclasses(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, Error> {
    let mut subclasses = HashSet::new();

    let query = format!(
        "SELECT subject FROM {table} WHERE object='{entity}' AND predicate='rdfs:subClassOf'",
        table = table,
        entity = entity,
    );

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let subject: &str = row.get("subject");
        subclasses.insert(String::from(subject));
    }

    Ok(subclasses)
}

pub async fn get_direct_sub_parts(
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
        part_of = part_of,
    );

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let subject: &str = row.get("subject");

        //filter for named classes
        match ldtab_2_value(&subject) {
            Value::String(_x) => {
                sub_parts.insert(String::from(subject));
            }
            _ => {}
        };
    }
    Ok(sub_parts)
}

//#################################################################
//####################### Human readable Text format (Markdown) ###
//#################################################################
pub fn json_tree_2_text(json_tree: &Value, indent: usize) -> String {
    let indentation = "\t".repeat(indent);
    let mut res = Vec::new();
    match json_tree {
        Value::Object(map) => {
            for (k, v) in map {
                res.push(format!(
                    "{}- {}{}",
                    indentation,
                    k,
                    json_tree_2_text(v, indent + 1)
                ));
            }

            let mut result = String::from("");
            for e in res {
                result = format!("{}\n{}", result, e);
            }
            result
        }
        Value::String(s) => format!("\n{}- {}", indentation, s),
        _ => String::from("error"),
    }
}

pub async fn get_text_view(entity: &str, table: &str, pool: &SqlitePool) -> String {
    let labelled_json_tree = get_labelled_json_tree_view(entity, table, pool).await;

    json_tree_2_text(&labelled_json_tree, 0)
}

//#################################################################
//####################### HTML view (JSON hiccup) #################
//#################################################################

pub fn tree_2_html_hiccup_children(parent: &str, value: &Value) -> Value {
    let mut res = Vec::new();
    res.push(json!("ul"));
    res.push(json!({"id" : "children"}));

    match value {
        Value::Array(children) => {
            for child in children {
                let mut res_element = Vec::new();
                res_element.push(json!("li"));

                res_element.push(json!(["a", {"resource" : child["curie"], "about": parent, "rev":child["property"] }, child["label"] ]));

                res.push(Value::Array(res_element));
            }
            Value::Array(res)
        }
        _ => json!("ERROR"), //TODO: encode error
    }
}

pub fn tree_2_html_hiccup_descendants(entity: &str, parent: &str, value: &Value) -> Value {
    //create new list for all children
    let mut res = Vec::new();
    res.push(json!("ul"));

    match value {
        Value::Array(children) => {
            for child in children {
                let mut res_element = Vec::new();
                res_element.push(json!("li"));

                res_element.push(json!(["a", {"resource" : child["curie"], "about": parent, "rev":child["property"] }, child["label"] ]));

                if child["curie"].as_str().unwrap().eq(entity) {
                    res_element.push(tree_2_html_hiccup_children(
                        child["curie"].as_str().unwrap(),
                        &child["children"],
                    ));
                } else {
                    //there exist children
                    res_element.push(tree_2_html_hiccup_descendants(
                        entity,
                        child["curie"].as_str().unwrap(),
                        &child["children"],
                    ));
                }

                res.push(Value::Array(res_element));
            }
            Value::Array(res)
        }
        _ => json!("ERROR"), //TODO: encode error
    }
}

pub fn tree_2_html_hiccup_roots(entity: &str, value: &Value) -> Value {
    //NB: Value is an array of the form [t_1, .., t_n]
    //each of the trees in the array need to be displayed in a list
    //
    //list for all roots
    let mut res = Vec::new();
    res.push(json!("ul"));

    match value {
        Value::Array(roots) => {
            for root in roots {
                let mut res_element = Vec::new();
                //list element for given root
                res_element.push(json!("li"));

                res_element.push(json!(["a", {"resource" : root["curie"] }, root["label"] ]));

                if root["curie"].as_str().unwrap().eq(entity) {
                    res_element.push(tree_2_html_hiccup_children(
                        root["curie"].as_str().unwrap(),
                        &root["children"],
                    ));
                } else {
                    //there exist children
                    res_element.push(tree_2_html_hiccup_descendants(
                        entity,
                        root["curie"].as_str().unwrap(),
                        &root["children"],
                    ));
                }
                res.push(Value::Array(res_element));
            }
            Value::Array(res)
        }
        _ => json!("ERROR"), //TODO: encode error
    }
}

pub async fn build_html_hiccup(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    let tree = get_rich_json_tree_view(entity, table, pool).await;

    let roots = tree_2_html_hiccup_roots(entity, &tree);

    let mut res = Vec::new();
    res.push(json!("ul"));
    res.push(json!(["li", "Ontology"]));
    let class = json!(["a", {"resource" : "owl:Class"}, "owl:Class"]);
    res.push(json!(["li", class, roots]));

    Value::Array(res)
}
