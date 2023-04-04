use serde_json::{from_str, json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use wiring_rs::util::signature;

static PART_OF: &'static str = "obo:BFO_0000050";
static IS_A: &'static str = "rdfs:subClassOf";

#[derive(Debug)]
pub struct LabelNotFound;

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
fn ldtab_2_value(string: &str) -> Value {
    //NB: an LDTab thick triple makes use of strings (which are not JSON strings
    //example: "this is a string" and "\"this is a JSON string\"".).
    let serde_value = match from_str::<Value>(string) {
        Ok(x) => x,
        _ => json!(string),
    };

    serde_value
}

// ################################################
// ######## build label map ######################
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
    let values = match class_2_subclasses.remove(target) {
        Some(x) => x,
        None => HashSet::new(), //return empty set
    };

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

/// Given a CURIE for an entity and a connection to an LDTab database,
/// return maps capturing information about the relationships 'is-a' and 'part-of'.
/// The mappings are structured in a hierarchical descending manner in which
/// a key-value pair consists of an entity (the key) and a set (the value) of all
/// its immediate subclasses ('is-a' relationship) or its parthoods ('part-of' relationship).
///
/// The two relationships are defined as follows:
///  - is-a is a relationship for (transitive) ancestors of the input entity
///  - part-of is a relationship defined via an OWL axiom of the form: "part 'is-a' 'part-of' some filler"
///
/// Example:
///
/// The axioms
///
/// axiom 1: 'gill' is-a 'compound organ'
/// axiom 2: 'gill' is-a 'part-of' some 'compound organ'
/// axiom 3: 'gill' is-a 'part-of' some 'respiratory system'
/// axiom 4: 'respiratory system' is-a 'anatomical system'
/// axiom 5: 'anatomical system' is-a 'part-of' some 'whole organism'
///
///  Would be turned into the following maps:
///
///  class_2_subclass:
///  {
///    'compound organ' : {'gill'},
///    'anatomical system' : {'respiratory system'},
///  }
///
///  class_2_parts:
///  {
///    'whole organism' : {'anatomical system'},
///    'respiratory system' : {'gill'},
///    'compound organ' : {'gill'},
///  }
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
    //both maps are build by iteratively querying for
    //combinations of is-a and part-of relationships.
    //In particular, this means querying for is-a relations
    //w.r.t. classes that are used as fillers in axioms for part-of relations.
    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();
    let mut class_2_parts: HashMap<String, HashSet<String>> = HashMap::new();

    //We assume the 'is-a' and 'part-of' relations to be acyclic.
    //Since both relations can be interpreted in terms of a partial order,
    //we can collect information about both relationships via a
    //breadth-first traversal according to the partial order.
    //(this removes the necessity for recursive function calls.)

    //start the search with the target entity
    let mut updates = HashSet::new();
    updates.insert(String::from(entity));

    //breadth-first traversal (from bottom to top)
    while !updates.is_empty() {
        let mut new_parts: HashSet<String> = HashSet::new();

        for update in &updates {
            //query database to get all 'is-a' ancestors
            let subclasses_updates = get_class_2_subclass_map(&update, table, pool).await?;
            update_hierarchy_map(&mut class_2_subclasses, &subclasses_updates);

            //extract information about 'part-of' relationships
            let parts_updates = get_part_of_information(&subclasses_updates);
            update_hierarchy_map(&mut class_2_parts, &parts_updates);

            //collect fillers of 'part-of' restrictions
            //with which we will continue the breadth-first search,i.e.
            //querying for all 'is-a' relationships,
            //extracting 'part-of' relations, etc.
            //until no 'part-of' relations are found.
            for part in parts_updates.keys() {
                if !class_2_subclasses.contains_key(part) {
                    new_parts.insert(part.clone());
                }
            }
        }

        //prepare filler of part-of relations for next iteration
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

/// Given a CURIE for an entity and a connection to an LDTab database,
/// return a map capturing (transitive) information about the 'is-a' relationship.
/// The mapping is structured in a hierarchical descending manner in which
/// a key-value pair consists of an entity (the key) and a set (the value) of all
/// its immediate subclasses ('is-a' relationship).
///
/// Example:
///
/// The information captured by the axioms
///
/// axiom 1: 'gill' is-a 'compound organ'
/// axiom 2: 'compound organ' is-a 'whole organism'
/// axiom 3: 'whole organism' is-a 'anatomical group'
/// axiom 4: 'anatomical system' is-a 'anatomical group'
///
/// would be represented by the map
///
/// {
///  'compound organ' : {'gill'},
///  'whole organism' : {'compound organism'},
///  'anatomical group' : {'whole organism', 'anatomical system'}
/// }
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
                //there already exists an entry in the map
                x.insert(subject_string);
            }
            None => {
                //create a new entry in the map
                let mut subclasses = HashSet::new();
                subclasses.insert(subject_string);
                class_2_subclasses.insert(object_string, subclasses);
            }
        }
    }
    Ok(class_2_subclasses)
}

/// Given a mapping from classes to sets of their subclasses,
/// extract and return a mapping from classes to sets of their parthoods.
/// In particular, a part-of relation expressed via an OWL axiom of the form
///
/// 'entity' is-a 'part-of' some 'filler'
///
/// then this information is represented via the following mapping:
///
/// {filler : entity}
///
/// Examples
///
/// Consider the axioms

/// axiom 1: 'gill' is-a 'part-of' some 'compound organ'
/// axiom 2: 'gill' is-a 'part-of' some 'respiratory system'
/// axiom 2: 'anatomical system' is-a 'part-of' some 'whole organism'
///
/// represented in class_2_subclasses via the following map:
///
/// {
///   'part-of' some 'compound organ' : {'gill'},
///   'part-of' some 'respiratory system' : {'gill'},
///   'part-of' some 'whole organism' : {'anatomical system'},
/// }.
///
/// The function get_part_of_information then returns the following map:
///
/// {
///    'compound organ', : {'gill'}
///    'respiratory system: {'gill'}
///    'whole organism' : {'anatomical system'},
/// }.  
pub fn get_part_of_information(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
) -> HashMap<String, HashSet<String>> {
    let mut class_2_parts: HashMap<String, HashSet<String>> = HashMap::new();

    //original axiom: S is-a part-of some filler
    //class_2_subclass map will contain: filler -> S
    for (class, subclasses) in class_2_subclasses {
        let class_value = ldtab_2_value(class);

        //check whether there is an existential restriction
        let part_of_restriction = match class_value.clone() {
            Value::Object(x) => check_part_of_restriction(&x),
            _ => false,
        };

        if part_of_restriction {
            //encode information in class_2_parts
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


/// Given a set (root) entities,
/// a map from entities to superclasses, 
/// a map from entities to part-of ancesetors, 
/// a map from entities to labels, 
/// return a term tree (encoded in JSON) representing information about its subsumption and parthood relations.
///
/// # Examples
///
/// Consider the entity obo:ZFA_0000354 (gill) and an LDTab data base zfa.db for zebrafish.
/// Then get_rich_json_tree_view(obo:ZFA_0000354, false, statement, zfa.db)
/// returns a tree of the form:
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",         <= ancestor of obo:ZFA_0000354 (gill)
///   "property": "rdfs:subClassOf",
///   "children": [
///     {
///       "curie": "obo:ZFA_0000272",
///       "label": "respiratory system",              <= ancestor of obo:ZFA_0000354 (gill)
///       "property": "rdfs:subClassOf",
///       "children": [
///         {
///           "curie": "obo:ZFA_0000354",
///           "label": "gill",
///           "property": "obo:BFO_0000050",
///           "children": [ ]
///          }]
///      }]
/// }]
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
                _ => {} //TODO: should be an error
            }
        }
        if class_2_parts.contains_key(i) {
            match build_rich_part_of_branch(i, class_2_subclasses, class_2_parts, curie_2_label) {
                Value::Object(x) => {
                    json_vec.push(Value::Object(x));
                }
                _ => {} //TODO: should be an error
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

pub async fn get_immediate_children_tree(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, sqlx::Error> {
    //get the entity's immediate descendents w.r.t. subsumption and parthood relations
    let direct_subclasses = get_direct_sub_hierarchy_maps(entity, table, pool).await?;
    let direct_part_ofs = get_direct_sub_parts(entity, table, pool).await?;

    //get labels
    let mut iris = HashSet::new();
    iris.extend(get_iris_from_set(&direct_subclasses));
    iris.extend(get_iris_from_set(&direct_part_ofs));

    //get labels for curies
    let curie_2_label = get_label_hash_map(&iris, table, pool).await?;

    let mut children = Vec::new();
    for sub in direct_subclasses {
        let element = json!({"curie" : sub, "label" : curie_2_label.get(&sub).unwrap(), "property" : IS_A, "children" : []});
        children.push(element);
    }

    for sub in direct_part_ofs {
        let element = json!({"curie" : sub, "label" : curie_2_label.get(&sub).unwrap(), "property" : PART_OF, "children" : []});
        children.push(element);
    }

    let children_tree = Value::Array(children);

    let sorted: Value = sort_rich_tree_by_label(&children_tree);
    Ok(sorted)
}

pub async fn get_preferred_roots(
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, sqlx::Error> {
    let mut preferred_roots = HashSet::new();
    let query = format!(
        "SELECT object FROM {table} WHERE predicate='obo:IAO_0000700'",
        table = table,
    );
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let object: &str = row.get("object");
        preferred_roots.insert(String::from(object));
    }

    Ok(preferred_roots)
}

async fn get_preferred_roots_hierarchy_maps(
    class_2_subclasses: &mut HashMap<String, HashSet<String>>,
    class_2_parts: &mut HashMap<String, HashSet<String>>,
    table: &str,
    pool: &SqlitePool,
) {
    //query for preferred roots
    let preferred_roots = get_preferred_roots(table, pool).await.unwrap();

    //collect all transitive ancestors
    let mut preferred_root_ancestor = HashSet::new();
    let mut current = HashSet::new();
    current.extend(preferred_roots);
    let mut next = HashSet::new();
    while !current.is_empty() {
        for preferred in &current {
            if class_2_subclasses.contains_key(preferred) {
                for (key, value) in &mut *class_2_subclasses {
                    if value.contains(preferred) {
                        preferred_root_ancestor.insert(key.clone());
                        next.insert(key.clone());
                    }
                }
            }

            if class_2_parts.contains_key(preferred) {
                for (key, value) in &mut *class_2_parts {
                    if value.contains(preferred) {
                        preferred_root_ancestor.insert(key.clone());
                        next.insert(key.clone());
                    }
                }
            }
        }
        current.clear();
        for n in &next {
            current.insert(n.clone());
        }
        next.clear();
    }

    //remove ancestors for preferred root terms
    for ancestor in preferred_root_ancestor {
        class_2_subclasses.remove(&ancestor);
        class_2_parts.remove(&ancestor);
    }
}

/// Given a CURIE of an entity and a connection to an LDTab database,
/// return a term tree (encoded in JSON) representing information about its subsumption and parthood relations.
/// The tree contains information about all available ancestor (or preferred root terms) of the target entity
/// as well as its immediate children and grandchildren.
///
/// # Examples
///
/// Consider the entity obo:ZFA_0000354 (gill) and an LDTab data base zfa.db for zebrafish.
/// Then get_rich_json_tree_view(obo:ZFA_0000354, false, statement, zfa.db)
/// returns a tree of the form:
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",         <= ancestor of obo:ZFA_0000354 (gill)
///   "property": "rdfs:subClassOf",
///   "children": [
///     {
///       "curie": "obo:ZFA_0000272",
///       "label": "respiratory system",              <= ancestor of obo:ZFA_0000354 (gill)
///       "property": "rdfs:subClassOf",
///       "children": [
///         {
///           "curie": "obo:ZFA_0000354",
///           "label": "gill",
///           "property": "obo:BFO_0000050",
///           "children": [
///             {
///               "curie": "obo:ZFA_0000716",
///               "label": "afferent branchial artery",  <= child of obo:ZFA_0000354 (gill)
///               "property": "obo:BFO_0000050",
///               "children": [
///                 {
///                   "curie": "obo:ZFA_0005012",
///                   "label": "afferent filamental artery",   <= grand child of obo:ZFA_0000354 (gill)
///                   "property": "obo:BFO_0000050",
///                   "children": []
///                 },
///               ]
///              }]
///          }]
///      }]
/// }]
pub async fn get_rich_json_tree_view(
    entity: &str,
    preferred_roots: bool,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, sqlx::Error> {
    //get the entity's ancestor information w.r.t. subsumption and parthood relations
    let (mut class_2_subclasses, mut class_2_parts) =
        get_hierarchy_maps(entity, table, &pool).await?;

    //modify ancestor information w.r.t. preferred root terms
    if preferred_roots {
        get_preferred_roots_hierarchy_maps(
            &mut class_2_subclasses,
            &mut class_2_parts,
            table,
            pool,
        )
        .await;
    }

    //get elements with no ancestors (i.e., roots)
    let roots = identify_roots(&class_2_subclasses, &class_2_parts);

    //extract CURIEs/IRIs
    let mut iris = HashSet::new();
    iris.extend(get_iris_from_subclass_map(&class_2_subclasses));
    iris.extend(get_iris_from_subclass_map(&class_2_parts));

    //get labels for CURIEs/IRIs
    let curie_2_label = get_label_hash_map(&iris, table, pool).await?;

    //build JSON tree
    let tree = build_rich_tree(&roots, &class_2_subclasses, &class_2_parts, &curie_2_label);

    //sort tree by label
    let mut sorted = sort_rich_tree_by_label(&tree);

    //get immediate children of leaf entities in the ancestor tree
    //NB: sorting the tree first ensures that the tree with added children is deterministic
    let mut children = get_immediate_children_tree(entity, table, pool).await?;

    //add childrens of children to children
    for child in children.as_array_mut().unwrap() {
        let child_iri = child["curie"].as_str().unwrap();
        let grand_children = get_immediate_children_tree(child_iri, table, pool).await?;
        child["children"] = grand_children;
    }

    //add children to the first occurrence of their respective parents in the (sorted) JSON tree
    add_children(&mut sorted, &children);

    Ok(sorted)
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

                //encode grand children
                let grand_children_html = tree_2_html_hiccup_children(
                    child["curie"].as_str().unwrap(),
                    &child["children"],
                );
                res_element.push(grand_children_html);

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
pub async fn build_html_hiccup(
    entity: &str,
    preferred_roots: bool,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, sqlx::Error> {
    let tree = get_rich_json_tree_view(entity, preferred_roots, table, pool).await?;

    let roots = tree_2_html_hiccup_roots(entity, &tree);

    let mut res = Vec::new();
    res.push(json!("ul"));
    res.push(json!(["li", "Ontology"]));
    let class = json!(["a", {"resource" : "owl:Class"}, "owl:Class"]);
    res.push(json!(["li", class, roots]));

    Ok(Value::Array(res))
}

pub async fn get_html_top_hierarchy(
    case: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, sqlx::Error> {
    let mut top = "";
    let mut relation = "";
    let mut rdf_type = "";

    match case {
        "Class" => {
            top = "owl:Thing";
            relation = "rdfs:subClassOf";
            rdf_type = "owl:Class"
        }
        "Object Property" => {
            top = "owl:topObjectProperty";
            relation = "rdfs:subPropertyOf";
            rdf_type = "owl:ObjectProperty";
        }
        "Data Property" => {
            top = "owl:topDataProperty";
            relation = "rdfs:subPropertyOf";
            rdf_type = "owl:DatatypeProperty";
        }
        _ => {}
    }

    //query for top level nodes
    let query = format!(
        "SELECT s1.subject
            FROM {table} s1
            WHERE s1.predicate = 'rdf:type'
              AND s1.object = '{rdf_type}'
              AND NOT EXISTS (
                SELECT 1
                FROM {table} s2
                WHERE s2.subject = s1.subject
                  AND s2.predicate = '{relation}'
              )
            UNION
            SELECT subject
            FROM {table}
            WHERE predicate = '{relation}'
            AND object = '{top}'",
        top = top,
        table = table,
        relation = relation,
        rdf_type = rdf_type,
    );

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;


    //go through rows 
    // -> collect set of iris for labels
    // -> build label map

    //build HTML view
    let mut res = Vec::new();
    res.push(json!("ul"));
    res.push(json!(["li", "Ontology"]));

    let mut children_list = Vec::new();
    children_list.push(json!("ul"));
    children_list.push(json!({"id" : "children"}));

    for row in rows {
        let subject: &str = row.get("subject");

        //TODO: remove these label calls
        let subject_label = get_label(subject, table, pool).await;

        match subject_label {
            Ok(x) => {
                children_list.push(
                    json!(["li", ["a", {"resource":subject, "rev" : "rdfs:subClassOf"}, x  ]]),
                );
            }
            _ => {
                children_list.push(
                    json!(["li", ["a", {"resource":subject, "rev" : "rdfs:subClassOf"},subject ]]),
                );
            }
        }
        //TODO: handle children
        //let children = get_immediate_children_tree(subject, table, pool).await.unwrap();
        //TODO: children for object properties
        //TODO: children for data properties
    }
    res.push(json!(["li", case, children_list]));
    Ok(Value::Array(res))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
    use std::collections::{HashMap, HashSet};

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
}
