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
pub fn ldtab_2_value(string: &str) -> Value {
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
pub fn build_label_query_for(curies: &HashSet<String>, table: &str) -> String {
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

/// Given an LDTab string,
/// return all IRIs/CURIEs ocurring in the string
///
/// Examples
///
/// Consider the LDTab string
///
/// s =  {"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],
///       "owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0000040"}],
///       "rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}
///
/// then get_iris_from_ldtab_string(s) returns the set
///
/// {"owl:onProperty", "obo:BFO_0000050", "owl:someValuesFrom","obo:ZFA_0000040", "rdf:type", "owl:Restriction"}
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

/// Given a map from entities (encoded as LDTab strings -- this could be a string or a JSON string)
/// to their respective subclasses,
/// return the set of all occuring entities (encoded as strings)
///
/// # Examples
///
/// Consider the map M
///
/// {
///   exp:A : {exp:B, exp:C},
///   exp:B : {exp:D}
/// }
///
/// Then get_iris_from_subclass_map(M) returns the set
///
/// {exp:A, exp:B, exp:C, exp:D}
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

/// Given a set of entities (encoded as LDTab strings -- this could be a string or a JSON string)
/// return the set of entities (encoded as strings).
///
/// # Examples
///
/// Consider the set S =  { exp:A, \"exp:B\", exp:C}
///
/// Then get_iris_from_set(S) returns the set
///
/// { exp:A, exp:B, exp:C }
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

/// Given an LDTab predicate map encoded as a Serde Value, return true
/// if the Value represents the 'part-of' relation (obo:BFO_0000050).
///
/// # Examples
///
/// Consider the value
/// v = {"datatype":"_IRI","object":"obo:BFO_0000050"}
///
/// then check_part_of_property(v) returns true
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

/// Given an LDTab predicate map encoded as a Serde Value, return true
/// if the Value represents an atomic entity (as opposed to another nested
/// Serde Value representing, e.g., an anonymous class expression.
///
/// # Examples
///
/// Consider the value
///
/// v_1 = {"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],
///        "owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0000040"}],
///        "rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}
///
/// v_2 = {"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],
///        "owl:someValuesFrom":[{"datatype":"_JSON","object":{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000030"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}}],
///        "rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}
///
/// then check_filler(v_1) returns true  and check_filler(v_2) returns false
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

/// Given an LDTab predicate map encoded, return true
/// if the Map encodes an existential restriction using the 'part-of' property
///
/// Consider the value
/// v = {"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],
///      "owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0000040"}],
///      "rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}
///
/// then check_part_of_restriction(v) returns true
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

/// Given a class expression and
/// a mutable map from class expression (in LDTab format) to their subclasses,
/// remove any occurrence of the target class epxression from the map
/// while maintaining transitive subclass relationships.
///
/// # Examples
///
/// Consider the class expression "p some f" and the map
/// m = { a : {b,c,p some f},
///       p some f : { d, e },
///       e : {f g},
///       h : {d, p some f},
///     }
///
/// then remove_invalid_class('p some f', m) returns the map
///
/// m = { a : {b,c,d,e},
///       e : {f g},
///       h : {d},
///     }
/// TODO: this is not used?
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

/// Given a map from class expression (in LDTab format) to their subclasses
/// and a set of target class expressions,
/// remove any occurrence of the target class epxressions from the map
/// (without maintaining transitive relationships) .
///
/// # Examples
///
/// Consider the class expression "p some f" and the map
///
/// m = { a : {b,c,p some f},
///       p some f : { d, e },
///       e : {f g},
///       h : {d, p some f},
///     }
///
/// then remove_invalid_classes('p some f', m) returns the map
///
/// m = { a : {b,c},
///       e : {f g},
///       h : {d},
///     }
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

/// Given a map from classes to (direct) subclasses in LDTab format,
/// identify anonymous class expressions.
/// (These anonymous class expressions are unwanted for the HTML view.)
///
/// # Examples
///
/// Consider the class expression "p some f" and the map
///
/// m = { a : {b,c,p some f},
///       p some f : { d, e },
///       e : {f g},
///       h : {d, r some g},
///     }
///
/// Then the set {p some f, r some g} is returned
///  as the set of identified invalid classes.
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

/// Given two maps from classes to subclasses,
/// insert the information from the second map ('updates') to the first ('to_update').
///
/// # Examples
///
/// Consider the maps
///
/// m_1 = { a : {b,c,d},
///         e : {f, g},
///         h : {d},
///       },
///
/// m_2 = { d : {g,h},
///         e : {i, j},
///       }
///
/// Then return the map
///
/// m = { a : {b,c,d},
///       d : {g,h}
///       e : {f, g, i, j},
///       h : {d},
///     },
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

/// Given two maps for subclass and parthood relations,
/// identify root classes, i.e., classes without parents.
///
/// # Examples
///
/// Consider the map
///
/// m = { a : {b,c,d,e},
///       e : {f, g},
///       h : {d},
///     }
///
/// then identify_roots identifies a and h as roots.  
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
/// Would be turned into the following maps:
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

/// Given a set (root) entities,
/// a map from entities to superclasses,
/// a map from entities to part-of ancestors,
/// a map from entities to labels,
/// return a term tree (encoded in JSON)
/// representing information about its subsumption and parthood relations
/// that is related via the 'is-a' relationship to some ancestor.
///
/// # Examples
///
/// Consider the tree
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",         
///   "property": "rdfs:subClassOf",               
///   "children": [
///     {
///       "curie": "obo:ZFA_0000272",
///       "label": "respiratory system",               
///       "property": "rdfs:subClassOf",              <= this is an 'is-a' branch for
///       "children": [                                  the ancestor "obo:ZFA_0100000"
///         {                                            ("zebrafish anatomical entity")
///           "curie": "obo:ZFA_0000354",
///           "label": "gill",
///           "property": "obo:BFO_0000050",      
///           "children": [ ]                        
///          }]
///      }]
/// }]
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

/// Given a set (root) entities,
/// a map from entities to superclasses,
/// a map from entities to part-of ancestors,
/// a map from entities to labels,
/// return a term tree (encoded in JSON)
/// representing information about its subsumption and parthood relations
/// that is related via the 'part-of' relationship to some ancestor.
///
/// # Examples
///
/// Consider the tree
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",         
///   "property": "rdfs:subClassOf",               
///   "children": [
///     {
///       "curie": "obo:ZFA_0000272",
///       "label": "respiratory system",               
///       "property": "rdfs:subClassOf",
///       "children": [                
///         {                         
///           "curie": "obo:ZFA_0000354",
///           "label": "gill",
///           "property": "obo:BFO_0000050",          <= this is a 'part-of' branch for
///           "children": [ ]                            the ancestor "obo:ZFA_0000272"
///          }]                                          ("respiratory system")
///      }]
/// }]
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

/// Given a node in a term tree, return its associated label.
///
/// Examples
///
/// Given the node
///
///  {
///   "curie": "obo:ZFA_00002722",
///   "label": "respiratory system",
///   "property": "rdfs:subClassOf",  
///   "children": [  ]
///  }
///
/// return "respiratory system"
pub fn extract_label(v: &Value) -> String {
    match v {
        Value::Object(_x) => String::from(v["label"].as_str().unwrap()),
        Value::String(x) => String::from(x), //use IRI instead of label (TODO: this case shouldn't occur)
        _ => String::from("error"),
    }
}

/// Given an array of nodes in a term tree, return the array sorted by node labels.
///
/// # Examples
///
/// Consider the following array of term tree node
///
///   [
///     {
///       "curie": "obo:ZFA_00002722",
///       "label": "respiratory system_B",  <- B
///       "property": "rdfs:subClassOf",  
///       "children": [  ]
///      },
///      {
///       "curie": "obo:ZFA_00002721",
///       "label": "respiratory system_C", <- C
///       "property": "rdfs:subClassOf",
///       "children": [  ]
///      },
///      {
///       "curie": "obo:ZFA_00002723",
///       "label": "respiratory system_A", <- A
///       "property": "rdfs:subClassOf",
///       "children": [  ]
///      }]
///
///   This array will be sorted by labels as follows:
///
///   [
///     {
///       "curie": "obo:ZFA_00002723",
///       "label": "respiratory system_A",  <- A
///       "property": "rdfs:subClassOf",  
///       "children": [  ]
///      },
///      {
///       "curie": "obo:ZFA_00002722",
///       "label": "respiratory system_B", <- B
///       "property": "rdfs:subClassOf",  
///       "children": [  ]
///      },
///      {
///       "curie": "obo:ZFA_00002721",
///       "label": "respiratory system_C", <- C
///       "property": "rdfs:subClassOf",
///       "children": [  ]
///      }]
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
    //However, this order does not (necessarily) match
    //a lexicographical order by labels
    let mut map = Map::new();

    //sort nested values
    for (key, value) in v.iter() {
        let sorted_value = sort_rich_tree_by_label(value);
        map.insert(key.clone(), sorted_value);
    }
    Value::Object(map)
}

/// Given a rich term tree (encoded with JSON),
/// sort the tree lexicographically w.r.t. entity labels
/// (this means sorting JSON arrays and JSON objects by keys).
///
/// Examples
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",
///   "property": "rdfs:subClassOf",        
///   "children": [
///     {
///       "curie": "obo:ZFA_00002722",
///       "label": "respiratory system_B",  <- B
///       "property": "rdfs:subClassOf",  
///       "children": [  ]
///      },
///      {
///       "curie": "obo:ZFA_00002721",
///       "label": "respiratory system_C", <- C
///       "property": "rdfs:subClassOf",
///       "children": [  ]
///      },
///      {
///       "curie": "obo:ZFA_00002723",
///       "label": "respiratory system_A", <- A
///       "property": "rdfs:subClassOf",
///       "children": [  ]
///      }]
/// }]
///
/// will be turned into
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",
///   "property": "rdfs:subClassOf",        
///   "children": [
///     {
///       "curie": "obo:ZFA_00002723",
///       "label": "respiratory system_A",  <- A
///       "property": "rdfs:subClassOf",  
///       "children": [  ]
///      },
///      {
///       "curie": "obo:ZFA_00002722",
///       "label": "respiratory system_B", <- B
///       "property": "rdfs:subClassOf",  
///       "children": [  ]
///      },
///      {
///       "curie": "obo:ZFA_00002721",
///       "label": "respiratory system_C", <- C
///       "property": "rdfs:subClassOf",
///       "children": [  ]
///      }]
/// }]
pub fn sort_rich_tree_by_label(tree: &Value) -> Value {
    match tree {
        Value::Array(a) => sort_array(a),
        Value::Object(o) => sort_object(o),
        _ => tree.clone(),
    }
}

/// Given a set (root) entities,
/// a map from entities to superclasses,
/// a map from entities to part-of ancestors,
/// a map from entities to labels,
/// return a term tree (encoded in JSON) representing information about its subsumption and parthood relations.
///
/// # Examples
///
/// Consider the entity obo:ZFA_0000354 (gill),
/// a map for subclasses {obo:ZFA_0100000 : {obo:ZFA_0000272}},
/// a map for part-of relations {obo:ZFA_0000272 : {obo:ZFA_0000354}},
/// a map for labels {obo:ZFA_0100000 : zebrafish anatomical entity,
///                   obo:ZFA_0000272 : respiratory system,
///                   obo:ZFA_0000354 : gill },
///
/// Then the function get_rich_json_tree_view returns a tree of the form:
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",         <= ancestor of obo:ZFA_0000354 (gill)
///   "property": "rdfs:subClassOf",               (related to ancestor via subclass-of by default)
///   "children": [
///     {
///       "curie": "obo:ZFA_0000272",
///       "label": "respiratory system",              <= ancestor of obo:ZFA_0000354 (gill)
///       "property": "rdfs:subClassOf",                 (related to ancestor via subclass-of)
///       "children": [
///         {
///           "curie": "obo:ZFA_0000354",
///           "label": "gill",
///           "property": "obo:BFO_0000050",      <= obo:ZFA_0000354 (gill)
///           "children": [ ]                        (related to ancestor via part-of)
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

    //iterate over starting entities in which the tree will be rooted ...
    for i in to_insert {
        //... and build branches w.r.t. is-a and part-of relationships
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

        //... or insert entity as a leaf node
        if !class_2_subclasses.contains_key(i) & !class_2_parts.contains_key(i) {
            json_vec.push(json!(String::from(i)));
        }
    }
    Value::Array(json_vec)
}

/// Given a rich term tree,
/// add children to the first occurrence of their respective parents in the tree.
///
/// # Examples
///
/// Consider the rich term tree
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",
///   "property": "rdfs:subClassOf",        
///   "children": [
///     {
///       "curie": "obo:ZFA_0000272",
///       "label": "respiratory system",   
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
///
/// and children/descendants for obo:ZFA_0000354:
///
/// [
///   {
///     "curie": "obo:ZFA_0000716",
///     "label": "afferent branchial artery",
///     "property": "obo:BFO_0000050",
///     "children": [
///       {
///         "curie": "obo:ZFA_0005012",
///         "label": "afferent filamental artery",
///         "property": "obo:BFO_0000050",
///         "children": []
///       },
///       {
///         "curie": "obo:ZFA_0005014",
///         "label": "recurrent branch afferent branchial artery",
///         "property": "obo:BFO_0000050",
///         "children": []
///       }
///     ]
///   },
///   {
///     "curie": "obo:ZFA_0000319",
///     "label": "branchiostegal membrane",
///     "property": "obo:BFO_0000050",
///     "children": []
///   },
/// ]
///
/// Then return
///
/// [{
///   "curie": "obo:ZFA_0100000",
///   "label": "zebrafish anatomical entity",
///   "property": "rdfs:subClassOf",        
///   "children": [
///     {
///       "curie": "obo:ZFA_0000272",
///       "label": "respiratory system",   
///       "property": "rdfs:subClassOf",  
///       "children": [
///         {
///           "curie": "obo:ZFA_0000354",
///           "label": "gill",
///           "property": "obo:BFO_0000050",
///           "children": [
///                        {
///                          "curie": "obo:ZFA_0000716",
///                          "label": "afferent branchial artery",
///                          "property": "obo:BFO_0000050",
///                          "children": [
///                            {
///                              "curie": "obo:ZFA_0005012",
///                              "label": "afferent filamental artery",
///                              "property": "obo:BFO_0000050",
///                              "children": []
///                            },
///                            {
///                              "curie": "obo:ZFA_0005014",
///                              "label": "recurrent branch afferent branchial artery",
///                              "property": "obo:BFO_0000050",
///                              "children": []
///                            }
///                          ]
///                        },
///                        {
///                          "curie": "obo:ZFA_0000319",
///                          "label": "branchiostegal membrane",
///                          "property": "obo:BFO_0000050",
///                          "children": []
///                        }]
///      }]
/// }]
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

/// Given an entity and a connection to an LDTab database,
/// return the immediate children of the entity w.r.t. the relationships is-a and part-of.
/// as rich JSON term tree nodes.
///
/// # Examples
///
/// Consider the (simplified) LDTab database with the following rows:
///
/// subject|predicate|object
/// b|is-a|a
/// c|is-a|a
/// d|is-a|'part-of' some a
/// (... rdfs:label information for a,b,c, ...)
///
///   then get_immediate_children_tree for a returns
///
///  [{
///    "curie": "b",
///    "label": "b_label",
///    "property": "rdfs:subClassOf",
///    "children": []
///    },
///    {
///     "curie": "c",
///     "label": "c_label",
///     "property": "rdfs:subClassOf",
///     "children": []
///    },
///    {
///     "curie": "d",
///     "label": "d_label",
///     "property": "obo:BFO_0000050",
///     "children": []
///    }]
pub async fn get_immediate_children_tree(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, sqlx::Error> {
    //get the entity's immediate descendents w.r.t. subsumption and parthood relations
    let direct_subclasses = get_direct_named_subclasses(entity, table, pool).await?;
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

/// Given an LDTab database,
/// return the set of preferred root terms in the database
///
/// # Examples
///
/// Consider the (simplified) LDTab database with the following rows:
///
/// subject|predicate|object
/// o|obo:IAO_0000700|a
/// o|obo:IAO_0000700|d
///
/// then get_preferred_roots returns the set {a,d}.
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

/// Given a map from classes to their subclasses,
/// a map from classes to their part-of ancestors,
/// an LDTab database connection,
/// modify the two ancestor maps so that they are rooted in preferred roots (if possible)
///
/// # Examples
///
/// Consider the map
///
/// m = {a : {d},
///      d : {e,f},
///      f : {g}}
///
/// and assume that d is a preferred root.
/// Then get_preferred_roots_hierarchy_maps modifies m as follows:
///
/// m = {d : {e,f},
///      f : {g}}
pub async fn get_preferred_roots_hierarchy_maps(
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

/// Given an entity and a connection to an LDTab database,
/// return the set of immediate (named) descendants w.r.t. its subsumption.
///
/// # Examples
///
/// Consider the (simplified) LDTab database with the following rows:
///
/// subject|predicate|object
/// b|rdfs:subClassOf|a
/// c|rdfs:subClassOf|a
/// r some d|rdfs:subClassOf|a
///
/// then get_direct_named_subclasses returns the set {b,c}.
pub async fn get_direct_named_subclasses(
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

/// Given an entity and a connection to an LDTab database,
/// return the set of immediate descendants w.r.t. the subsumption relation
///
/// # Examples
///
/// Consider the (simplified) LDTab database with the following rows:
///
/// subject|predicate|object
/// b|rdfs:subClassOf|a
/// c|rdfs:subClassOf|a
/// r some d|rdfs:subClassOf|a
///
/// then get_direct_named_subclasses returns the set {b,c,r some d}.
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

/// Given an entity and a connection to an LDTab database,
/// return the set of immediate (named) descendants w.r.t. the parthood relation
///
/// # Examples
///
/// Consider the (simplified) LDTab database with the following rows:
///
/// subject|predicate|object
/// b|rdfs:subClassOf|part-of some a
/// c|rdfs:subClassOf|a
/// r some d|rdfs:subClassOf|part-of some a
///
/// then get_direct_sub_parts returns the set {b}.
pub async fn get_direct_sub_parts(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, sqlx::Error> {
    let mut sub_parts = HashSet::new();

    //RDF representation of an OWL existential restriction
    //using the property part-of (obo:BFO_0000050)
    let part_of = r#"{"owl:onProperty":[{"datatype":"_IRI","object":"relation"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"entity"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#;
    let part_of = part_of.replace("entity", entity);
    let part_of = part_of.replace("relation", PART_OF);

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

/// Given an entity and the branches of decsendants in a term tree,
/// return the hiccup style encoding of the tree.
///
/// Consider the following input
///
/// # Example
///
/// parent   =  obo:ZFA_0000211
///
/// children = [{"curie":"obo:ZFA_0005015",
///              "label":"afferent lamellar arteriole",
///               "property":"obo:BFO_0000050",
///               "children":[]},
///             {"curie":"obo:ZFA_0005019",
///              "label":"efferent lamellar arteriole",
///              "property":"obo:BFO_0000050",
///              "children":[]}]
///
///  then tree_2_hiccup_direct_children returns
///
/// ["ul" {"id" : "children"}
///  ["li" ["a", {"resource" : "obo:ZFA_0005015, "about": obo:ZFA_0000211, "rev":"obo:BFO_0000050" }, "afferent lamellar arteriole"] ]
///  ["li" ["a", {"resource" : "obo:ZFA_0005019, "about": obo:ZFA_0000211, "rev":"obo:BFO_0000050" }, "efferent lamellar arteriole"] ]
/// ]
//TODO: Return Result
pub fn tree_2_hiccup_direct_children(parent: &str, direct_children: &Value) -> Value {
    let mut res = Vec::new();
    res.push(json!("ul"));
    res.push(json!({"id" : "children"}));

    match direct_children {
        Value::Array(children) => {
            for child in children {
                let mut res_element = Vec::new();
                res_element.push(json!("li"));

                res_element.push(json!(["a", {"resource" : child["curie"], "about": parent, "rev":child["property"] }, child["label"] ]));

                //encode grand children
                let grand_children_html = tree_2_hiccup_direct_children(
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

/// Given an entity, its parent entiy, and the branches of decsendants in the term tree,
/// return the hiccup style encoding of the tree.
///
/// # Example
///
/// Consider the following input
///
/// entity      = obo:ZFA_0000354
/// parent      = obo:ZFA_0001094
/// descendants = [
///                {"curie":"obo:ZFA_0001439","label":"anatomical system","property":"obo:BFO_0000050","children":[{"curie":"obo:ZFA_0000272","label":"respiratory system","property":"rdfs:subClassOf","children":[{"curie":"obo:ZFA_0000354","label":"gill","property":"obo:BFO_0000050","children":[]}]}]},
///                {"curie":"obo:ZFA_0000496","label":"compound organ","property":"obo:BFO_0000050","children":[{"curie":"obo:ZFA_0000354","label":"gill","property":"rdfs:subClassOf","children":[]}]}]
///
/// Then tree_2_hiccup_descendants returns
/// ["ul"
///   ["li" ["a", {"resource" : "obo:ZFA_0001439", "about": "obo:ZFA_0001094", "rev": "obo:BFO_0000050"}, "anatomical system" ]
///         [ 'recursive encoding for children' -> node_2_hiccup ... ]
///   ]
///   ["li" ["a", {"resource" : "obo:ZFA_0000496", "about": "obo:ZFA_0001094", "rev": "obo:BFO_0000050"}, "compound organ" ]
///         [ 'recursive encoding for children' -> node_2_hiccup ... ]
///   ]
/// ]
/// TODO: Return Result
pub fn tree_2_hiccup_descendants(entity: &str, parent: &str, descendants: &Value) -> Value {
    let mut res = Vec::new();
    res.push(json!("ul"));

    match descendants {
        Value::Array(children) => {
            for child in children {
                let mut res_elements = Vec::new();
                res_elements.push(json!("li"));

                res_elements.push(json!(["a", {"resource" : child["curie"], "about": parent, "rev":child["property"] }, child["label"] ]));

                node_2_hiccup(entity, &child, &mut res_elements);

                res.push(Value::Array(res_elements));
            }
            Value::Array(res)
        }
        _ => json!("ERROR"), //TODO: encode error
    }
}

/// Given an entity, its corresponding node in a branch of a term tree,
/// and a hiccup-style encoding of the entity's ancestor tree,
/// return the hiccup style encoding of the tree.
///
/// # Example
///
/// Consider the following input:
///
/// entity = obo:ZFA_0000354,
/// node   = {"curie":"obo:ZFA_0000354","label":"gill","property":"obo:BFO_0000050","children":[]},
/// hiccup = ["li",["a",{"resource":"obo:ZFA_0000354","about":"obo:ZFA_0000272","rev":"obo:BFO_0000050"},"gill"]]
//
/// Then node_2_hiccup only wraps
///
/// returns ["ul" hiccup] because there are no more children nodes to be added.
/// TODO: return result + error handling
pub fn node_2_hiccup(entity: &str, node: &Value, hiccup: &mut Vec<Value>) {
    if node["curie"].as_str().unwrap().eq(entity) {
        //base case for direct children
        hiccup.push(tree_2_hiccup_direct_children(
            node["curie"].as_str().unwrap(),
            &node["children"],
        ));
    } else {
        //recursive call for nested children
        hiccup.push(tree_2_hiccup_descendants(
            entity,
            node["curie"].as_str().unwrap(),
            &node["children"],
        ));
    }
}

/// Given an entity and its (rich json) term tree,
/// return the hiccup style encoding of the tree.
///
/// # Examples
///
/// Consider the entity obo:0obo:ZFA_00003540 and its
/// term tree

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
///
/// then return the following hiccup-style list (only an excerpt is shown)
///
/// ["ul",
///   ["li", "Ontology"],
///   ["li",
///     ["a", {"resource": "owl:Class"}, "owl:Class"],
///     ["ul",
///       ["li",
///         ["a",{"resource": "obo:ZFA_0100000"},"zebrafish anatomical entity"],
///         ["ul",
///           ["li",
///             ["a",{
///                 "resource": "obo:ZFA_0000272",
///                 "about": "obo:ZFA_0100000",
///                 "rev": "rdfs:subClassOf"},
///               "respiratory system"
///             ],
///             ["ul",
///              ["li",
///                ["a",{
///                    "resource": "obo:ZFA_0001512",
///                    "about": "obo:ZFA_0000272",
///                    "rev": "rdfs:subClassOf"
///                  },
///                  "anatomical group"
///                ],
///             ...  
pub fn tree_2_hiccup(entity: &str, tree: &Value) -> Value {
    //TODO: return Result and add error handling
    let mut tree_hiccup = Vec::new();
    tree_hiccup.push(json!("ul"));

    match tree {
        //the tree consist of an array of root notes (i.e. it might be a forest)
        Value::Array(roots) => {
            for root in roots {
                let mut node_hiccup = Vec::new();

                node_hiccup.push(json!("li"));

                node_hiccup.push(json!(["a", {"resource" : root["curie"] }, root["label"] ]));

                node_2_hiccup(entity, &root, &mut node_hiccup);

                tree_hiccup.push(Value::Array(node_hiccup));
            }
            Value::Array(tree_hiccup)
        }
        _ => json!("ERROR"), //TODO: encode error
    }
}

/// Given a CURIE/IRI of an entity and an LDTab database,
/// return a term tree for the entity.
///
/// # Examples
///
/// Consider the entity obo:ZFA_0000354 and an
/// LDTab database with information about the ZFA ontology.
/// Then, get_hiccup_term_tre(obo:ZFA_0000354, false, statement, zfa)
/// will return a term tree of the following form (only an excerpt is shown):
///
/// ["ul",
///   ["li", "Ontology"],
///   ["li",
///     ["a", {"resource": "owl:Class"}, "owl:Class"],
///     ["ul",
///       ["li",
///         ["a",{"resource": "obo:ZFA_0100000"},"zebrafish anatomical entity"],
///         ["ul",
///           ["li",
///             ["a",{
///                 "resource": "obo:ZFA_0000037",
///                 "about": "obo:ZFA_0100000",
///                 "rev": "rdfs:subClassOf"},
///               "anatomical structure"
///             ],
///             ["ul",
///              ["li",
///                ["a",{
///                    "resource": "obo:ZFA_0001512",
///                    "about": "obo:ZFA_0000037",
///                    "rev": "rdfs:subClassOf"
///                  },
///                  "anatomical group"
///                ],
///             ...  
pub async fn get_hiccup_term_tree(
    entity: &str,
    preferred_roots: bool,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, sqlx::Error> {
    let tree = get_rich_json_tree_view(entity, preferred_roots, table, pool).await?;

    let roots = tree_2_hiccup(entity, &tree);

    let mut res = Vec::new();
    res.push(json!("ul"));
    res.push(json!(["li", "Ontology"]));
    let class = json!(["a", {"resource" : "owl:Class"}, "owl:Class"]);
    res.push(json!(["li", class, roots]));

    Ok(Value::Array(res))
}

/// Given a target OWL type, e.g., "Class", "Object Property", and "Datatype Property",
/// return a hiccup-style list of all the top-level nodes of that type.
///
/// # Examples
///
/// The call get_hiccup_top_hierarchy("Object Property", "statement", &zfa_connection)
/// returns (only an excerpt is shown)
///
/// ["ul",
///   ["li", "Ontology"],
///   ["li","Object Property",
///     ["ul", { "id": "children" },
///       ["li",
///         ["a",
///           {
///             "resource": "obo:RO_0002131",
///             "rev": "rdfs:subClassOf"
///           },
///           "overlaps"
///         ]
///       ],
///       ["li",
///         ["a",
///           {
///             "resource": "obo:RO_0002150",
///             "rev": "rdfs:subClassOf"
///           },
///           "continuous with"
///         ]
///       ],
///       ,
///      ...
///     ]
///   ]
/// ]
pub async fn get_hiccup_top_hierarchy(
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

    //build HTML view
    let mut res = Vec::new();
    res.push(json!("ul"));
    res.push(json!(["li", "Ontology"]));

    let mut children_list = Vec::new();
    children_list.push(json!("ul"));
    children_list.push(json!({"id" : "children"}));

    let mut entities = HashSet::new();
    let mut ent_vec = Vec::new(); //workaround to ensure deterministic output (for testing purposes)

    //collect entities
    for row in rows {
        let subject: &str = row.get("subject");
        entities.insert(String::from(subject));
        ent_vec.push(String::from(subject));
    }

    //get labels for entities
    let entity_2_label = get_label_hash_map(&entities, table, pool).await.unwrap();

    for subject in ent_vec {
        match entity_2_label.get(&subject) {
            Some(x) => {
                children_list.push(
                    json!(["li", ["a", {"resource":subject, "rev" : "rdfs:subClassOf"}, x  ]]),
                );
            }
            None => {
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
