use serde_json::{from_str, json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use wiring_rs::util::signature;

static PART_OF: &'static str = "obo:BFO_0000050";
static IS_A: &'static str = "rdfs:subClassOf";

#[derive(Debug)]
pub struct LabelNotFound;

//An LDTab string is either a JSON string or a string.
//In the case of a JSON string, the input string can be parsed as a Value.
//In the case of a string, the input string needs to be converted to a Serde Value
//
pub fn ldtab_2_value(input: &str) -> Value {
    match from_str::<Value>(input) {
        Ok(x) => x,        //JSON string was parsed to a serde Value
        _ => json!(input), //normal string is converted to a serde Value
    }
}

// ################################################
// ######## build label map ######################
// ################################################

///Given a set of CURIEs and an LDTab database,
///return a JSON object that maps CURIEs to their labels
///
///TODO: code example
pub async fn get_label_map(iris: &HashSet<String>, table: &str, pool: &SqlitePool) -> Value {
    let entity_2_label = get_label_hash_map(iris, table, pool).await;

    json!({ "@labels": entity_2_label })
}

///Given a set of CURIEs and an LDTab database,
///return a map from CURIEs to their respective labels
///
///TODO: code example
pub async fn get_label_hash_map(
    iris: &HashSet<String>,
    table: &str,
    pool: &SqlitePool,
) -> HashMap<String, String> {
    let mut entity_2_label = HashMap::new();

    for i in iris {
        let label = get_label(&i, table, pool).await;
        match label {
            Ok(x) => {
                entity_2_label.insert(i.clone(), x);
            }
            Err(_x) => {} //TODO how should missing labels be treated?
        };
    }

    entity_2_label
}

///Given an entity's CURIE and an LDTab database,
///return the entity's label
///
///TODO: code example
pub async fn get_label(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<String, LabelNotFound> {
    let query = format!(
        "SELECT * FROM {} WHERE subject='{}' AND predicate='rdfs:label'",
        table, entity
    );
    let rows: Vec<SqliteRow> = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(|_| LabelNotFound)?;
    //NB: this should be a singleton
    for row in rows {
        //let subject: &str = row.get("subject");
        let label: &str = row.get("object");
        return Ok(String::from(label));
    }

    Err(LabelNotFound)
}

///Given an LDTab JSON string,
///return IRIs and CURIEs that occur in the string
///
///TODO: code example
pub fn get_iris_from_ldtab_string(s: &str) -> HashSet<String> {
    let mut iris: HashSet<String> = HashSet::new();

    let value = ldtab_2_value(&s);
    match value {
        Value::String(x) => {
            iris.insert(x);
        }
        _ => {
            //use wiring_rs to extract IRIs recursively
            signature::get_iris(&value, &mut iris);
        }
    }
    iris
}

///Given a map from entities to their respective subclasses,
///return all IRIs that occur in the map.
///
///TODO: code example
pub fn get_iris_from_subclass_map(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut iris: HashSet<String> = HashSet::new();
    for (k, v) in class_2_subclasses {
        iris.extend(get_iris_from_ldtab_string(&k));
        for subclass in v {
            iris.extend(get_iris_from_ldtab_string(&subclass));
        }
    }
    iris
}

///Given a set of strings, return all IRIs that occur in the set.
///
///TODO: code example
pub fn get_iris_from_set(set: &HashSet<String>) -> HashSet<String> {
    let mut iris: HashSet<String> = HashSet::new();
    for e in set {
        iris.extend(get_iris_from_ldtab_string(&e));
    }
    iris
}

// ################################################
// ######## build tree view #######################
// ################################################

///Given an LDTab predicate map encoded as a Serde Value, return true
///if the Value represents the 'part-of' relation (obo:BFO_0000050).
///
///TODO: code example
pub fn check_part_of_property(value: &Value) -> bool {
    match value {
        Value::Object(x) => {
            let property = x.get("object").unwrap();
            let part_of = json!(PART_OF); //'part of' relation
            property.eq(&part_of)
        }
        _ => false,
    }
}

///Given an LDTab predicate map encoded as a Serde Value, return true
///if the Value represents an atomic entity (as opposed to another nested
///Serde Value representing, e.g., an anonymous class expression.
///
///TODO: code example
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

///Given an LDTab predicate map encoded, return true
///if the Map encodes an existential restriction using the 'part-of' property
///
///TODO: code example
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

///Given a map from class expression (in LDTab format) to their subclasses
///and a target class expression,
///remove any occurrence of the target class epxression from the map
///while maintaining transitive subclass relationships.
///
///TODO: example
pub fn remove_invalid_class(
    target: &str,
    class_2_subclasses: &mut HashMap<String, HashSet<String>>,
) {
    //remove mapping [target : {subclass_1, subclass_2, ..., subclass_n}]
    let values = class_2_subclasses.remove(target).unwrap();
    for (_key, value) in class_2_subclasses {
        if value.contains(target) {
            //replace 'target' with {subclass_1, subclass_2, ..., subclass_n}
            value.remove(target);
            for v in &values {
                value.insert(v.clone());
            }
        }
    }
}

///Given a map from class expression (in LDTab format) to their subclasses
///and a set of target class expressions,
///remove any occurrence of the target class epxressions from the map
///while maintaining transitive subclass relationships.
///
///TODO: example
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

///Given a map from classes to (direct) subclasses in LDTab format,
///identify anonymous class expressions. These anonymous class expressions
///are unwanted for the HTML view.
///
///TODO: doc string + doc test
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

///Given two maps from classes to subclasses,
///insert the information from the second map ('updates') to the first ('to_update').
///TODO: code example
pub fn update_hierarchy_map(
    to_update: &mut HashMap<String, HashSet<String>>,
    updates: &HashMap<String, HashSet<String>>,
) {
    for (class, subclasses) in updates {
        match to_update.get_mut(class) {
            Some(x) => {
                //key exists
                for sub in subclasses {
                    if !x.contains(sub) {
                        x.insert(sub.clone()); //so add all elements to value
                    }
                }
            }
            None => {
                //key does not exist
                //so clone the whole entry
                to_update.insert(class.clone(), subclasses.clone());
            }
        }
    }
}

///Given two maps for subclass and parthood relations,
///identify root classes, i.e., classes without parents.
///
///TODO: code example
pub fn identify_roots(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut roots = HashSet::new();

    //collect all keys and values from both maps
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

    //check which keys do not occur in any value of any map
    for k in keys {
        if !values.contains(k) {
            roots.insert(k.clone());
        }
    }
    roots
}

///Given a CURIE for an entity and a connection to an LDTab database,
///return maps capturing information about the relationships 'is-a' and 'part-of'.
///The mappings are structured in a hierarchical descending manner in which
///a key-value pair consists of an entity (the key) and a set (the value) of all
///its immediate subclasses ('is-a' relationship) or its parthoods ('part-of' relationship).
///
///
///TODO: text example
///TODO: code example
pub async fn get_hierarchy_maps(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<
    (
        HashMap<String, HashSet<String>>,
        HashMap<String, HashSet<String>>,
    ),
    sqlx::Error,
> {
    //both maps are build by iteratively querying for combinations of 'is-a'
    //and 'part-of' relationships.
    //TODO: explain the way these two relations are combined
    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();
    let mut class_2_parts: HashMap<String, HashSet<String>> = HashMap::new();

    let mut updates = HashSet::new();
    updates.insert(String::from(entity));

    //We assume the 'is-a' and 'part-of' relations to be acyclic.
    //Since both relations can be interpreted in terms of a partial order,
    //we can collect information about both relationships via a
    //breadth-first traversal according to the partial order.
    //Note that this removes the necessity for recursive function calls.
    while !updates.is_empty() {
        let mut new_parts: HashSet<String> = HashSet::new();

        for update in &updates {
            //recursive SQL query to get all 'is-a' ancestors
            let subclasses_updates = get_class_2_subclass_map(&update, table, pool).await?;
            update_hierarchy_map(&mut class_2_subclasses, &subclasses_updates);

            //extract information about 'part-of' relationships
            let parts_updates = get_part_of_information(&subclasses_updates);
            update_hierarchy_map(&mut class_2_parts, &parts_updates);

            //collect fillers of 'part-of' restrictions
            //on which we will 'recurse' in the next iteration, i.e.,
            //querying for all 'is-a' relationships,
            //extracting 'part-of' relations,
            //and recurse until no 'part-of' relations are found.
            for part in parts_updates.keys() {
                if !class_2_subclasses.contains_key(part) {
                    new_parts.insert(part.clone());
                }
            }
        }

        //prepare filler of part-of relations for next iteration/recursion
        updates.clear();
        for new in new_parts {
            updates.insert(new.clone());
        }
    }

    //We only want to return information about named entities.
    //So, we filter out 'invalid' entities, e.g., anonymous class expressions
    let invalid = identify_invalid_classes(&class_2_subclasses);
    remove_invalid_classes(&mut class_2_subclasses, &invalid);

    Ok((class_2_subclasses, class_2_parts))
}

///Given a CURIE for an entity and a connection to an LDTab database,
///return a map capturing (transitive) information about the 'is-a' relationship.
///The mapping is structured in a hierarchical descending manner in which
///a key-value pair consists of an entity (the key) and a set (the value) of all
///its immediate subclasses ('is-a' relationship).
///
///TODO: text example
///TODO: code example
pub async fn get_class_2_subclass_map(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashMap<String, HashSet<String>>, sqlx::Error> {
    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();

    //recursive SQL query for transitive 'is-a' relationships
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

///Given a mapping from classes to sets of their subclasses,
///extract and return a mapping from classes to sets of their parthoods.
///In particular, whenever there is an subclass mapping of the form
///
///entity -> part-of some filler
///
///then add a 'part-of' map of the form
///
///filler -> entity
///
///TODO: text example
///TODO: code example
pub fn get_part_of_information(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> HashMap<String, HashSet<String>> {
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
    class_2_parts
}

///TODO:
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

///Given a set of entities, and maps for subclass and parthood relations,
///return an encoding of the term tree via JSON objects.
///
///TODO: example
///TODO: code example
pub fn build_tree(
    to_insert: &HashSet<String>,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_parts: &HashMap<String, HashSet<String>>,
) -> Value {
    let mut json_map = Map::new();
    for i in to_insert {
        //handle 'is-a' case
        if class_2_subclasses.contains_key(i) {
            match build_is_a_branch(i, class_2_subclasses, class_2_parts) {
                Value::Object(x) => {
                    json_map.extend(x);
                }
                _ => {}
            }
        }
        //handle 'part-of' case
        if class_2_parts.contains_key(i) {
            match build_part_of_branch(i, class_2_subclasses, class_2_parts) {
                Value::Object(x) => {
                    json_map.extend(x);
                }
                _ => {}
            }
        }

        //leaf case
        //TODO: remove use of owl:Nothing?
        if !class_2_subclasses.contains_key(i) & !class_2_parts.contains_key(i) {
            json_map.insert(String::from(i), Value::String(String::from("owl:Nothing")));
        }
    }
    Value::Object(json_map)
}

///Given a CURIE for an entity and a connection to an LDTab database,
///return a tree (encoded in JSON) for the entity that displays information about
///the relationships 'is-a' as well as 'part-of'
///
///TODO: text example
///TODO: code example
pub async fn get_json_tree_view(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    //extract information about an entities 'is-a' and 'part-of' relationships
    let (class_2_subclasses, class_2_parts) =
        get_hierarchy_maps(entity, table, &pool).await.unwrap();

    //organise the information in a (rooted) tree (or forest)
    let roots = identify_roots(&class_2_subclasses, &class_2_parts);
    build_tree(&roots, &class_2_subclasses, &class_2_parts)
}

///Given a tree (encoded in JSON) representing information about
///the relationships 'is-a' as well as 'part-of' as well as
///a mapo from CURIEs to their labels,
///return the tree with CURIEs replaced with labels.
///
///TODO: text example
///TODO: code example
pub fn build_labelled_tree(tree: &Value, label_map: &HashMap<String, String>) -> Value {
    let mut json_map = Map::new();

    match tree {
        Value::Object(x) => {
            for (k, v) in x {
                if k.starts_with("partOf ") {
                    //TODO rethink encoding of part-of relation
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

///Given a CURIE for an entity and a connection to an LDTab database,
///return a tree (encoded in JSON) for the entity that displays information about
///the relationships 'is-a' as well as 'part-of'. The returned tree uses labels instead of CURIEs.
///
///TODO: text example
///TODO: code example
pub async fn get_labelled_json_tree_view(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    //get information about subsumption and parthood relations
    let (class_2_subclasses, class_2_parts) =
        get_hierarchy_maps(entity, table, &pool).await.unwrap();

    //root entities for subsumption and parthood relations
    let roots = identify_roots(&class_2_subclasses, &class_2_parts);

    //extract CURIEs/IRIs
    let mut iris = HashSet::new();
    iris.extend(get_iris_from_subclass_map(&class_2_subclasses));
    iris.extend(get_iris_from_subclass_map(&class_2_parts));

    //get map from CURIEs/IRIs to labels
    let label_hash_map = get_label_hash_map(&iris, table, pool).await;

    //build term tree
    let tree = build_tree(&roots, &class_2_subclasses, &class_2_parts);

    //replace CURIEs with labels
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

        //children_vec.push(json!("owl:Nothing"));
        //
        //json_map.insert(
        //    String::from(to_insert),
        //    Value::String(String::from("owl:Nothing")),
        //);
        //Value::Object(json_map)
    }

    json!({"curie" : to_insert, "label" : curie_2_label.get(to_insert), "property" : IS_A, "children" : children_vec})
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
        //children_vec.push(json!("owl:Nothing"));
        //json_map.insert(
        //    format!("partOf {}", to_insert),
        //    Value::String(String::from("owl:Nothing")),
        //);
        //Value::Object(json_map);
    }

    json!({"curie" : to_insert, "label" : curie_2_label.get(to_insert), "property" : PART_OF, "children" : children_vec})
}

pub fn extract_label(v: &Value) -> String {
    match v {
        Value::Object(_x) => String::from(v["label"].as_str().unwrap()),
        Value::String(x) => String::from(x), //use IRI instead of label
        _ => String::from("error"),
    }
}

pub fn sort_array(array: &Vec<Value>) -> Value {
    let mut labels = Vec::new();
    let mut label_2_element = HashMap::new();
    for element in array {
        //let label  = element["label"].as_str().unwrap();
        let label = extract_label(element);
        labels.push(label.clone());
        let sorted_element = sort_rich_tree_by_label(element);
        label_2_element.insert(label.clone(), sorted_element);
    }

    labels.sort();

    let mut res = Vec::new();
    for label in labels {
        let element = label_2_element.get(&label).unwrap();
        res.push(element.clone());
    }

    Value::Array(res)
}

pub fn sort_object(v: &Map<String, Value>) -> Value {
    //serde objects are sorted by keys:
    //"By default the map is backed by a BTreeMap."
    //TODO: think about this
    let mut map = Map::new();

    //sort nested values
    for (key, value) in v.iter() {
        let sorted_value = sort_rich_tree_by_label(value);
        map.insert(key.clone(), sorted_value);
    }
    Value::Object(map)
}

pub fn sort_rich_tree_by_label(tree: &Value) -> Value {
    match tree {
        Value::Array(a) => sort_array(a),
        Value::Object(o) => sort_object(o),
        _ => tree.clone(),
    }
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
            json_vec.push(json!(String::from(i)));
        }
    }
    Value::Array(json_vec)
}

pub fn add_children(tree: &mut Value, children: &Value) {
    match tree {
        Value::Object(_x) => {
            let tree_children = tree["children"].as_array_mut().unwrap();

            if tree_children.is_empty() {
                tree["children"] = children.clone();
            } else {
                //descend into first child
                add_children(&mut tree_children[0], children);
            }
        }
        Value::Array(x) => {
            if x.is_empty() {
                //do nothing
            } else {
                //descend
                add_children(&mut x[0], children);
            }
        }
        _ => {} //TODO: ERROR
    }
}

pub async fn get_immediate_children_tree(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    //get the entity's immediate descendents w.r.t. subsumption and parthood relations
    let direct_subclasses = get_direct_sub_hierarchy_maps(entity, table, pool)
        .await
        .unwrap();
    let direct_part_ofs = get_direct_sub_parts(entity, table, pool).await.unwrap();

    //get labels
    let mut iris = HashSet::new();
    iris.extend(get_iris_from_set(&direct_subclasses));
    iris.extend(get_iris_from_set(&direct_part_ofs));

    //get labels for curies
    let curie_2_label = get_label_hash_map(&iris, table, pool).await;

    let mut children = Vec::new();
    for sub in direct_subclasses {
        let element = json!({"curie" : sub, "label" : curie_2_label.get(&sub).unwrap(), "property" : IS_A, "children" : []});
        children.push(element);
    }

    for sub in direct_part_ofs {
        let element = json!({"curie" : sub, "label" : curie_2_label.get(&sub).unwrap(), "property" : PART_OF, "children" : []});
        children.push(element);
    }

    Value::Array(children)
}

///Given a CURIE of an entity and a connection to an LDTab database,
///return a term tree (encoded in JSON) representing information about its subsumption and parthood relations
///
///TODO: example
pub async fn get_rich_json_tree_view(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    //get the entity's ancestor information w.r.t. subsumption and parthood relations
    let (class_2_subclasses, class_2_parts) =
        get_hierarchy_maps(entity, table, &pool).await.unwrap();

    let roots = identify_roots(&class_2_subclasses, &class_2_parts);

    //extract CURIEs/IRIs
    let mut iris = HashSet::new();
    iris.extend(get_iris_from_subclass_map(&class_2_subclasses));
    iris.extend(get_iris_from_subclass_map(&class_2_parts));

    //get labels for curies
    let curie_2_label = get_label_hash_map(&iris, table, pool).await;

    //build ancestor tree
    let mut tree = build_rich_tree(&roots, &class_2_subclasses, &class_2_parts, &curie_2_label);

    //TODO: you first need to sort .. then add children (which also need to be sorted)
    //otherwise, children might not get added to the lexicographically first occurence
    //(which makes the output not deterministic)

    //add branch of immediate children to first occurrence of entity in the ancestor tree
    let children = get_immediate_children_tree(entity, table, pool).await;
    add_children(&mut tree, &children);

    //sort tree by label
    let sorted = sort_rich_tree_by_label(&tree);
    sorted
}

///Given a CURIE of an entity and a connection to an LDTab database,
///return the set of immediate (named) descendants w.r.t. its subsumption and parthood relations
///
///TODO: example
pub async fn get_direct_sub_hierarchy_maps(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, sqlx::Error> {
    let subclasses = get_direct_subclasses(entity, table, pool).await?;

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

///Given a CURIE of an entity and a connection to an LDTab database,
///return the set of immediate descendants w.r.t. the subsumption relation
///
///TODO: example
pub async fn get_direct_subclasses(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, sqlx::Error> {
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

///Given a CURIE of an entity and a connection to an LDTab database,
///return the set of immediate descendants w.r.t. the parthood relation
///
///TODO: example
pub async fn get_direct_sub_parts(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, sqlx::Error> {
    let mut sub_parts = HashSet::new();

    //RDF representation of an OWL existential restriction
    //using the property part-of (obo:BFO_0000050)
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

///Given a simple term tree, return a human-readable representation
///in Markdown.
//TODO: Return Result
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

///Given a CURIE of an entity and an LDTab database,
///return a human-readable representation in Markdown.
pub async fn get_text_view(entity: &str, table: &str, pool: &SqlitePool) -> String {
    //get term tree (encoded in JSON)
    let labelled_json_tree = get_labelled_json_tree_view(entity, table, pool).await;
    //transform JSON to Markdown
    json_tree_2_text(&labelled_json_tree, 0)
}

//#################################################################
//####################### HTML view (JSON hiccup) #################
//#################################################################

//TODO: Return Result
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

//TODO: Return Result
pub fn tree_2_html_hiccup_descendants(entity: &str, parent: &str, value: &Value) -> Value {
    let mut res = Vec::new();
    res.push(json!("ul"));

    match value {
        Value::Array(children) => {
            for child in children {
                let mut res_elements = Vec::new();
                res_elements.push(json!("li"));

                res_elements.push(json!(["a", {"resource" : child["curie"], "about": parent, "rev":child["property"] }, child["label"] ]));

                encode_element(entity, &child, &mut res_elements);

                res.push(Value::Array(res_elements));
            }
            Value::Array(res)
        }
        _ => json!("ERROR"), //TODO: encode error
    }
}

pub fn encode_element(entity: &str, value: &Value, res: &mut Vec<Value>) {
    if value["curie"].as_str().unwrap().eq(entity) {
        //base case
        res.push(tree_2_html_hiccup_children(
            value["curie"].as_str().unwrap(),
            &value["children"],
        ));
    } else {
        //recurse
        res.push(tree_2_html_hiccup_descendants(
            entity,
            value["curie"].as_str().unwrap(),
            &value["children"],
        ));
    }
}

//TODO: Return Result
pub fn tree_2_html_hiccup_roots(entity: &str, value: &Value) -> Value {
    let mut res = Vec::new();
    res.push(json!("ul"));

    match value {
        Value::Array(roots) => {
            for root in roots {
                let mut res_elements = Vec::new();

                res_elements.push(json!("li"));

                res_elements.push(json!(["a", {"resource" : root["curie"] }, root["label"] ]));

                encode_element(entity, &root, &mut res_elements);

                res.push(Value::Array(res_elements));
            }
            Value::Array(res)
        }
        _ => json!("ERROR"), //TODO: encode error
    }
}

//TODO: Return Result
///Given a CURIE of an entity and an LDTab database,
///return an HTML view of a term tree.
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
