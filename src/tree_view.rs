use ontodev_hiccup::hiccup;
use serde_json::{from_str, json, Map, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use wiring_rs::util::signature;

static IS_A: &'static str = "rdfs:subClassOf";
static SUBPROPERTY: &'static str = "rdfs:subPropertyOf";

#[derive(Error, Debug)]
pub enum TreeViewError {
    #[error("data base error")]
    Database(#[from] sqlx::Error),
    #[error("the data format `{0}` is not correct")]
    LDTab(String),
    #[error("the data format `{0}` is not correct")]
    TreeFormat(String),
    #[error("unknown error")]
    Unknown(String),
}

#[derive(Debug, Clone)]
pub enum OWLEntityType {
    Ontology,
    Class,
    AnnotationProperty,
    DataProperty,
    ObjectProperty,
    Individual,
    Datatype,
}

/// Given a CURIE/IRI for a property (note that we don't check the rdf:type)
/// and a connection to an LDTab database,
/// return a map capturing (transitive) information about the 'subPropertyOf' relationship.
/// The mapping is structured in a hierarchical descending manner in which
/// a key-value pair consists of an entity (the key) and a set (the value) of all
/// its immediate subclasses ('subPropertyOf' relationship).
///
/// Example:
///
/// The information captured by the axioms
///
/// axiom 1: 'a' subPropertyOf 'b'
/// axiom 2: 'c' subPropertyOf 'd'
/// axiom 3: 'd' subPropertyOf 'f'
/// axiom 4: 'e' subPropertyOf 'f'
///
/// would be represented by the map
///
/// {
///  'b' : {'a'},
///  'd' : {'c'},
///  'f' : {'d', 'e'}
/// }
pub async fn get_property_2_subproperty_map(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashMap<String, HashSet<String>>, TreeViewError> {
    let mut property_2_subproperties: HashMap<String, HashSet<String>> = HashMap::new();

    //recursive SQL query for transitive 'subProperty' relationships
    let query = format!("WITH RECURSIVE
    superproperties( subject, object ) AS
    ( SELECT subject, object FROM {table} WHERE subject='{entity}' AND predicate='rdfs:subPropertyOf'
        UNION ALL
        SELECT {table}.subject, {table}.object FROM {table}, superproperties WHERE {table}.subject = superproperties.object AND {table}.predicate='rdfs:subPropertyOf'
     ) SELECT * FROM superproperties;", table=table, entity=entity);

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    for row in rows {
        //axiom structure: subject rdfs:subPropertyOf object
        let subject: &str = row.get("subject"); //subclass
        let object: &str = row.get("object"); //superclass

        let subject_string = String::from(subject);
        let object_string = String::from(object);

        match property_2_subproperties.get_mut(&object_string) {
            Some(set_of_subclasses) => {
                //there already exists an entry in the map
                set_of_subclasses.insert(subject_string);
            }
            None => {
                //create a new entry in the map
                let mut subclasses = HashSet::new();
                subclasses.insert(subject_string);
                property_2_subproperties.insert(object_string, subclasses);
            }
        }
    }
    Ok(property_2_subproperties)
}

/// Given a CURIE/IRI of a property , and a connection to an LDTab database,
/// return the immediate children of the property w.r.t. rdfs:subPropertyOf
/// as rich JSON term tree nodes.
///
/// # Examples
///
/// Consider the (simplified) LDTab database with the following rows:
///
/// subject|predicate|object
/// b|subPropertyOf|a
/// c|subPropertyOf|a
/// d|subPropertyOf|b
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
///    }]
pub async fn get_immediate_property_children_tree(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    let mut direct_sub_properties = HashSet::new();

    let query = format!(
        "SELECT subject FROM {table} WHERE object='{entity}' AND predicate='rdfs:subPropertyOf'",
        table = table,
        entity = entity,
    );

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let subject: &str = row.get("subject");
        direct_sub_properties.insert(String::from(subject));
    }

    let mut iris = HashSet::new();
    iris.extend(get_iris_from_set(&direct_sub_properties));

    let curie_2_label = get_label_hash_map(&iris, table, pool).await?;

    let mut children = Vec::new();
    for sub in direct_sub_properties {
        let label = match curie_2_label.get(&sub) {
            Some(l) => l,
            None => &sub,
            //None => return Err(TreeViewError::TreeFormat(format!("No label for {}", sub))),
        };
        let element =
            json!({"curie" : sub, "label" : label, "property" : SUBPROPERTY, "children" : []});
        children.push(element);
    }

    let children_tree = Value::Array(children);

    Ok(children_tree)
}

/// Given a rich term tree for properties,
/// add grandchildren to children nodes.
///
/// # Examples
///
/// Consider the rich term tree
///
/// tree = {
///          "curie": "exp:a",
///          "label": "A",
///          "property": "rdfs:subPropertyOf",        
///          "children": [
///            {
///              "curie": "exp:b",
///              "label": "B",   
///              "property": "rdfs:subPropertyOf",  
///              "children": [ ]
///             }]
///        }
///
/// add_grandchildren(tree["children"], table, pool)
/// adds children nodes to all direct children in the tree, e.g.,
///
/// [{
///   "curie": "exp:a",
///   "label": "A",
///   "property": "rdfs:subPropertyOf",        
///   "children": [
///     {
///       "curie": "exp:b",
///       "label": "B",   
///       "property": "rdfs:subPropertyOf",  
///       "children": [
///         {
///           "curie": "exp:grandchild",
///           "label": "grandchild",
///           "property": "rdfs:subPropertyOf",
///           "children": [ ]              
///          }]
///      }]
/// }]
pub async fn add_property_grandchildren(
    children: &mut Value,
    table: &str,
    pool: &SqlitePool,
) -> Result<(), TreeViewError> {
    let mut children_array = match children.as_array_mut() {
        Some(array) => array,
        None => {
            return Err(TreeViewError::TreeFormat(format!(
                "No children nodes in {}",
                children.to_string()
            )))
        }
    };

    for child in children_array {
        let child_iri = match child["curie"].as_str() {
            Some(string) => string,
            None => {
                return Err(TreeViewError::TreeFormat(format!(
                    "No value for 'curie' field in {}",
                    child.to_string()
                )))
            }
        };
        let grand_children = get_immediate_property_children_tree(child_iri, table, pool).await?;
        child["children"] = grand_children;
    }
    Ok(())
}

/// Given an IRI/CURIE for a property entity (note that the rdf:type is not checked)
/// and a connection to an LDTab database, return a tree (encoded in JSON)
/// for the entity's ancestors and immediate children (and grandchildren) w.r.t.
/// rdfs:subPropertyOf and the specified relations.  
///
/// # Examples
///
/// Consider the entity obo:RO_0002131 (overlaps) and an LDTab data base zfa.db for zebrafish.
/// Then get_rich_json_tree_view(obo:RO_0002131, false, statement, zfa.db)
/// returns a tree of the form:
///
/// [{
///   "curie": "obo:RO_0002131",
///   "label": "overlaps",         
///   "property": "rdfs:subPropertyOf",
///   "children": [
///     {
///       "curie": "obo:BFO_0000051",
///       "label": "pas part",              
///       "property": "rdfs:subPropertyOf",
///       "children": [ ]
///      },
///     {
///       "curie": "obo:BFO_0000050",
///       "label": "part of",              
///       "property": "rdfs:subPropertyOf",
///       "children": [ ]
///      } ]
/// }]
pub async fn get_rich_json_property_tree_view(
    entity: &str,
    preferred_roots: bool,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    let mut property_hierarchy_map = get_property_2_subproperty_map(entity, table, pool).await?;
    let mut property_2_hierarchy_map = HashMap::new();
    property_2_hierarchy_map.insert(String::from(SUBPROPERTY), property_hierarchy_map);

    //modify ancestor information w.r.t. preferred root terms
    if preferred_roots {
        get_preferred_roots_hierarchy_maps(&mut property_2_hierarchy_map, table, pool).await?;
    }

    let roots = identify_roots(&property_2_hierarchy_map);

    //extract all IRI/CURIEs from the hierarchy maps (to query for their respective labels)
    let mut iris = HashSet::new();
    iris.insert(String::from(entity)); //add entity for root case (when ancestor tree is empty)
    for map in property_2_hierarchy_map.values() {
        iris.extend(get_iris_from_subclass_map(&map));
    }

    let curie_2_label = get_label_hash_map(&iris, table, pool).await?;

    let mut tree = build_rich_tree(&roots, &property_2_hierarchy_map, &curie_2_label)?;

    if is_empty(&tree)? {
        let label = match curie_2_label.get(entity) {
            Some(l) => l,
            None => entity,
        };
        tree = json!([{"curie": entity, "label":label, "property": SUBPROPERTY, "children" : [] }]);
    }

    //sort tree by label
    let mut sorted = sort_rich_tree_by_label(&tree)?;

    let mut immediate_children = get_immediate_property_children_tree(entity, table, pool).await?;
    add_property_grandchildren(&mut immediate_children, table, pool).await?;
    add_children(&mut sorted, &immediate_children)?;

    Ok(sorted)
}

/// Given an LDTab _JSON value, return the LDTab value associated with the provided key 'field'.
///
/// # Examples
///
/// Consider the following LDTab _JSON value
///
/// v = {"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],
///      "owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000027"}],
///      "rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]
///
/// get_ldtab_field(&v, "owl:onProperty") returns [{"datatype":"_IRI","object":"obo:RO_0002496"}].
pub fn get_ldtab_field(value: &Value, field: &str) -> Result<Value, TreeViewError> {
    match value {
        Value::Object(map) => match map.get(field) {
            Some(field_value) => Ok(field_value.clone()),
            None => {
                return Err(TreeViewError::LDTab(format!(
                    "No field {} in LDTab value {}",
                    field,
                    value.to_string()
                )))
            }
        },
        _ => {
            return Err(TreeViewError::LDTab(format!(
                "Not an LDTab object: {}",
                value.to_string()
            )))
        }
    }
}

/// Given an LDTab array, return the first element in the array.
///
/// # Examples
///
/// Consider the following LDTab array
///
/// a = [{"object":"obo:RO_0002496","object":"obo:RO_0002497","object":"obo:RO_0002498"}]
///
/// get_ldtab_array_at(&a, 0)  returns "object":"obo:RO_0002496"
pub fn get_ldtab_array_at(value: &Value, index: usize) -> Result<Value, TreeViewError> {
    match value {
        Value::Array(array) => Ok(array[index].clone()),
        _ => {
            return Err(TreeViewError::LDTab(format!(
                "Not an LDTab array: {}",
                value.to_string()
            )))
        }
    }
}

/// Given an LDTab _JSON value, return the first LDTab value in the array associated with the key 'field'.
///
/// # Examples
///
/// Consider the following LDTab _JSON value
///
/// v = {"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],
///      "owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000027"}],
///      "rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]
///
/// get_ldtab_first_field(&v, "owl:onProperty") returns {"datatype":"_IRI","object":"obo:RO_0002496"}.
pub fn get_ldtab_first_field(value: &Value, field: &str) -> Result<Value, TreeViewError> {
    let target_array = get_ldtab_field(value, field)?;
    let target = get_ldtab_array_at(&target_array, 0)?;
    Ok(target)
}

/// Given an LDTab string value, return the corresponding String
///
/// # Examples
///
/// Consider the following LDTab string s = \"obo:0000356\".
///
/// get_ldtab_value_as_string(&s) returns "obo:0000356".
pub fn get_ldtab_value_as_string(value: &Value) -> Result<String, TreeViewError> {
    match value.as_str() {
        Some(string) => Ok(String::from(string)),
        None => {
            return Err(TreeViewError::LDTab(format!(
                "Not an LDTab string: {}",
                value.to_string()
            )))
        }
    }
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
pub fn ldtab_2_value(string: &str) -> Value {
    //NB: an LDTab thick triple makes use of strings (which are not JSON strings
    //example: "this is a string" and "\"this is a JSON string\"".).
    let serde_value = match from_str::<Value>(string) {
        Ok(json) => json,
        _ => json!(string),
    };

    serde_value
}

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
/// and ldb an LDTab database.
/// Then get_label_hash_map(S, table, ldb) returns the map
/// {"obo:ZFA_0000354": "gill",
///  "rdfs:label": "label"}
/// extracted from a given table in ldb.  
pub async fn get_label_hash_map(
    curies: &HashSet<String>,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashMap<String, String>, TreeViewError> {
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
pub fn get_iris_from_ldtab_string(ldtab_string: &str) -> HashSet<String> {
    let mut iris: HashSet<String> = HashSet::new();

    let value = ldtab_2_value(&ldtab_string);
    match value {
        Value::String(iri_string) => {
            iris.insert(iri_string);
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

/// Given maps for hierarchical relations,
/// identify root classes, i.e., classes without parents.
///
/// # Examples
///
/// Consider the map
///
/// {
///   rdfs:subClassOf = { a : {b,c,d,e},
///                       e : {f, g},
///                       h : {d},
///                     }
/// }
///
/// then identify_roots identifies a and h as roots.  
pub fn identify_roots(
    class_2_relations: &HashMap<String, HashMap<String, HashSet<String>>>,
) -> HashSet<String> {
    let mut roots = HashSet::new();

    //collect all keys and values from both maps
    let mut keys = HashSet::new();
    let mut values = HashSet::new();

    for map in class_2_relations.values() {
        for (k, v) in map {
            keys.insert(k);
            values.extend(v);
        }
    }

    //check which keys do not occur in any value of any map
    for k in keys {
        if !values.contains(k) {
            roots.insert(k.clone());
        }
    }
    roots
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
            for sub in v {
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

/// Given a map from class expression (in LDTab format) to their subclasses
/// and a set of target class expressions,
/// remove any occurrence of the target class epxressions from the map
/// (without maintaining transitive relationships) .
///
/// # Examples
///
/// Consider the map
///
/// m = { a : {b,c,p some f},
///       p some f : { d, e },
///       e : {f g},
///       h : {d, p some f},
///     }
///
/// then remove_invalid_classes(m) returns the map
///
/// m = { a : {b,c},
///       e : {f g},
///       h : {d},
///     }
pub fn remove_invalid_classes(class_2_subclasses: &mut HashMap<String, HashSet<String>>) {
    let invalid = identify_invalid_classes(class_2_subclasses);

    //remove invalid keys
    for i in &invalid {
        class_2_subclasses.remove(i);
    }
    //remove invalid parts in values
    for (_k, v) in class_2_subclasses.iter_mut() {
        for i in &invalid {
            v.remove(i);
        }
    }
}

/// Given an LDTab predicate map encoded as a Serde Value, return true
/// if the Value represents the 'part-of' relation (obo:BFO_0000050).
///
/// # Examples
///
/// Consider the value
/// v = {"datatype":"_IRI","object":"obo:BFO_0000050"}
///
/// then check_part_of_property(v) returns true
pub fn check_property(value: &Value, relation: &str) -> Result<bool, TreeViewError> {
    let property = get_ldtab_field(value, "object")?;
    let relation_json = json!(relation);
    Ok(property.eq(&relation_json))
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
pub fn check_filler(value: &Value) -> Result<bool, TreeViewError> {
    let filler = get_ldtab_field(value, "object")?;
    //check whether 'filler' is a named class (represented as a JSON string)
    //as opposed to complex expression (represnted as a JSON object)
    match filler {
        Value::String(_string) => Ok(true),
        _ => Ok(false),
    }
}

/// Given an LDTab predicate map encoded, return true
/// if the Map encodes an existential restriction using a given property.
///
/// Consider the value
/// v = {"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],
///      "owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0000040"}],
///      "rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}
///
/// then check_part_of_restriction(v,"obo:BFO_0000050") returns true
pub fn check_restriction(value: &Value, relation: &str) -> Result<bool, TreeViewError> {
    let part_of_restriction = match value {
        Value::Object(map) => {
            if map.contains_key("owl:onProperty")
                & map.contains_key("owl:someValuesFrom")
                & map.contains_key("rdf:type")
            {
                let property = get_ldtab_first_field(value, "owl:onProperty")?;
                let filler = get_ldtab_first_field(value, "owl:someValuesFrom")?;

                let property_check = check_property(&property, relation)?;
                let filler_check = check_filler(&filler)?;

                property_check & filler_check
            } else {
                false
            }
        }
        _ => false,
    };
    Ok(part_of_restriction)
}

/// Given an LDTab _JSON value for an existential restriction.
///
/// # Examples
///
/// Consider the following LDTab _JSON value
///
/// v = {"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],
///      "owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000027"}],
///      "rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]
///
/// get_existential_filler_as_string(&v) returns "obo:ZFS_0000027".
pub fn get_existential_filler_as_string(
    existential_restriction: &Value,
) -> Result<String, TreeViewError> {
    let filler = get_ldtab_first_field(&existential_restriction, "owl:someValuesFrom")?;
    let filler = get_ldtab_field(&filler, "object")?;
    let filler = get_ldtab_value_as_string(&filler)?;
    Ok(filler)
}

/// Given a mapping from classes to sets of their subclasses,
/// extract and return a mapping from classes to sets of hierarchical relationships
/// for a given relation.
/// Such relations are expressed in OWL via an axiom of the form
///
/// 'entity' is-a 'part-of' some 'filler'
///
/// This information is represented via the mapping: {filler : entity}
///
/// Examples
///
/// Consider the axioms
///
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
pub fn get_relation_information(
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    relation: &str,
) -> Result<HashMap<String, HashSet<String>>, TreeViewError> {
    let mut class_2_relation: HashMap<String, HashSet<String>> = HashMap::new();

    //original axiom: S is-a part-of some filler
    //class_2_subclass map will contain: filler -> S
    for (class, subclasses) in class_2_subclasses {
        let class_value = ldtab_2_value(class);

        //check whether there is an existential restriction
        let is_restriction = check_restriction(&class_value, relation)?;

        if is_restriction {
            let filler = get_existential_filler_as_string(&class_value)?;

            //encode information in class_2_relation
            for subclass in subclasses {
                match class_2_relation.get_mut(&filler) {
                    Some(set_of_subclasses) => {
                        set_of_subclasses.insert(subclass.clone());
                    }
                    None => {
                        let mut subclasses = HashSet::new();
                        subclasses.insert(subclass.clone());
                        class_2_relation.insert(filler.clone(), subclasses);
                    }
                }
            }
        }
    }
    Ok(class_2_relation)
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
            Some(set_of_subclasses) => {
                //key exists
                for sub in subclasses {
                    if !set_of_subclasses.contains(sub) {
                        set_of_subclasses.insert(sub.clone()); //so add all elements to value
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

/// Given a CURIE/IRI for a class (note that we don't check the rdf:type)
/// and a connection to an LDTab database,
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
) -> Result<HashMap<String, HashSet<String>>, TreeViewError> {
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
            Some(set_of_subclasses) => {
                //there already exists an entry in the map
                set_of_subclasses.insert(subject_string);
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

/// Given an IRI/CURIE for an entity, a list of relations, and a connection to an LDTab database,
/// return maps capturing information about the entity's hierarchical relationships
/// (in addition to rdfs:subClassOf).
/// The mappings are structured in a hierarchical descending manner in which
/// a key-value pair consists of an entity (the key) and a set (the value) of all
/// its immediate descendants.
///
/// The relationships are defined as follows:
///  - is-a is a relationship for (transitive) ancestors of the input entity
///  - otherwise, a relationship is defined via an OWL axiom of the form: "part 'is-a' 'relation' some 'filler'"
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
/// Would be turned into the following map:
/// {
///  rdfs:subClassOf:
///           {
///             'compound organ' : {'gill'},
///             'anatomical system' : {'respiratory system'},
///           }
///
///  BFO_0000050:
///           {
///             'whole organism' : {'anatomical system'},
///             'respiratory system' : {'gill'},
///             'compound organ' : {'gill'},
///           }
/// }
pub async fn get_hierarchy_maps(
    entity: &str,
    relations: &Vec<&str>,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashMap<String, HashMap<String, HashSet<String>>>, TreeViewError> {
    //init map from relations to associated entity hierarchies
    let mut class_2_subrelations = HashMap::new();

    //is-a relations are initialised by default
    let class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();
    class_2_subrelations.insert(String::from(IS_A), class_2_subclasses);

    //initialise input relations
    for rel in relations {
        let class_2_subrelation: HashMap<String, HashSet<String>> = HashMap::new();
        class_2_subrelations.insert(String::from(rel.clone()), class_2_subrelation);
    }

    //start the search with the target entity
    let mut updates = HashSet::new();
    updates.insert(String::from(entity));

    //breadth-first search w.r.t. hierarchical relations:
    //- the search starts with is-a ancestors
    //- then, information about other relations is extracted from is-a ancestors
    //- and then is-a ancestors for extracted relation-ancestors are used in the next iteration
    //- until no more ancestors are found (this terminates because there are finitely many ancestors)
    while !updates.is_empty() {
        let mut new_relations: HashSet<String> = HashSet::new();
        for update in &updates {
            let subclasses_updates = get_class_2_subclass_map(&update, table, pool).await?;
            //NB: class_2_subrelations is guaranteed to contain IS_A
            let mut subclassof_map = class_2_subrelations.get_mut(IS_A).unwrap();
            update_hierarchy_map(&mut subclassof_map, &subclasses_updates);

            for rel in relations {
                let relation_updates = get_relation_information(&subclasses_updates, rel)?;
                //NB: class_2_subrelations is guaranteed to contain rel
                let mut rel_map = class_2_subrelations.get_mut(rel as &str).unwrap();
                update_hierarchy_map(&mut rel_map, &relation_updates);

                for relation_update in relation_updates.keys() {
                    let subclassof_map = class_2_subrelations.get(IS_A).unwrap();
                    if !subclassof_map.contains_key(relation_update) {
                        new_relations.insert(relation_update.clone());
                    }
                }
            }
        }

        //prepare next iteration of the breadth-first search
        updates.clear();
        for new in new_relations {
            updates.insert(new.clone());
        }
    }

    let mut subclassof_map = class_2_subrelations.get_mut(IS_A).unwrap();
    remove_invalid_classes(&mut subclassof_map);

    Ok(class_2_subrelations)
}

/// Given a set (root) entities,
/// a map for hierarchical relations
/// (which maps a relation to another map -- mapping entities to immediate descendants) ,
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
pub fn build_rich_tree_branch(
    to_insert: &str,
    relation: &str,
    relation_maps: &HashMap<String, HashMap<String, HashSet<String>>>,
    curie_2_label: &HashMap<String, String>,
) -> Result<Value, TreeViewError> {
    let mut children_vec: Vec<Value> = Vec::new();

    for (rel, map) in relation_maps {
        match map.get(to_insert) {
            Some(children) => {
                for c in children {
                    let v = build_rich_tree_branch(c, rel, relation_maps, curie_2_label)?;
                    children_vec.push(v);
                }
            }
            None => {}
        }
    }

    Ok(
        json!({"curie" : to_insert, "label" : curie_2_label.get(to_insert), "property" : relation, "children" : children_vec}),
    )
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
    relation_maps: &HashMap<String, HashMap<String, HashSet<String>>>,
    curie_2_label: &HashMap<String, String>,
) -> Result<Value, TreeViewError> {
    let mut json_vec: Vec<Value> = Vec::new();

    for i in to_insert {
        let mut inserted = false;
        for (rel, map) in relation_maps {
            if map.contains_key(i) {
                inserted = true;
                let branch = build_rich_tree_branch(i, rel, relation_maps, curie_2_label)?;
                json_vec.push(branch);
            }
        }

        if !inserted {
            json_vec.push(json!(String::from(i)));
        }
    }
    Ok(Value::Array(json_vec))
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
pub fn extract_label(value: &Value) -> Result<String, TreeViewError> {
    match value {
        Value::Object(map) => match map.get("label") {
            Some(label) => match label.as_str() {
                Some(string) => Ok(String::from(string)),
                None => Err(TreeViewError::TreeFormat(format!(
                    "Value for field 'label' is not a string: {}",
                    value.to_string()
                ))),
            },
            None => Err(TreeViewError::TreeFormat(format!(
                "No field 'label' in node: {}",
                value.to_string()
            ))),
        },
        _ => Err(TreeViewError::TreeFormat(format!(
            "Expected JSON object for tree node but got: {}",
            value.to_string()
        ))),
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
pub fn sort_array(array: &Vec<Value>) -> Result<Value, TreeViewError> {
    let mut labels = Vec::new();
    let mut label_2_node = HashMap::new();
    for node in array {
        let label = extract_label(node)?;
        labels.push(label.clone());
        let sorted_element = sort_rich_tree_by_label(node)?;
        label_2_node.insert(label.clone(), sorted_element);
    }

    labels.sort();

    let mut res = Vec::new();
    for label in labels {
        //NB: label_2_node was created in this function
        //and is guaranteed to include 'label'
        let node = label_2_node.get(&label).unwrap();
        res.push(node.clone());
    }

    Ok(Value::Array(res))
}

pub fn sort_object(object: &Map<String, Value>) -> Result<Value, TreeViewError> {
    //serde objects are sorted by keys:
    //"By default the map is backed by a BTreeMap."
    //However, this order does not (necessarily) match
    //a lexicographical order by labels
    let mut map = Map::new();

    //sort nested values
    for (key, value) in object.iter() {
        let sorted_value = sort_rich_tree_by_label(value)?;
        map.insert(key.clone(), sorted_value);
    }
    Ok(Value::Object(map))
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
pub fn sort_rich_tree_by_label(tree: &Value) -> Result<Value, TreeViewError> {
    match tree {
        Value::Array(a) => Ok(sort_array(a)?),
        Value::Object(o) => Ok(sort_object(o)?),
        Value::String(_s) => Ok(tree.clone()),
        _ => Err(TreeViewError::TreeFormat(format!(
            "Expected a tree, list of nodes, or string but got: {}",
            tree.to_string()
        ))),
    }
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
) -> Result<HashSet<String>, TreeViewError> {
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
) -> Result<HashSet<String>, TreeViewError> {
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

/// Given an entity, a vector of relations, and a connection to an LDTab database,
/// return the set of immediate (named) descendants w.r.t. the specified relations
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
/// then get_direct_sub_parts returns the set {b} for the part-of relation.
pub async fn get_direct_sub_relations(
    entity: &str,
    relation: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<HashSet<String>, TreeViewError> {
    let mut sub_relations = HashSet::new();

    //RDF representation of an OWL existential restriction
    let restriction = r#"{"owl:onProperty":[{"datatype":"_IRI","object":"relation"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"entity"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#;
    let restriction = restriction.replace("entity", entity);
    let restriction = restriction.replace("relation", relation);

    let query = format!(
        "SELECT subject FROM {table} WHERE object='{restriction}' AND predicate='rdfs:subClassOf'",
        table = table,
        restriction = restriction,
    );

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let subject: &str = row.get("subject");

        //filter for named classes
        match ldtab_2_value(&subject) {
            Value::String(_x) => {
                sub_relations.insert(String::from(subject));
            }
            _ => {}
        };
    }
    Ok(sub_relations)
}

/// Given an entity, a vector of relations, and a connection to an LDTab database,
/// return the immediate children of the entity w.r.t. the specified relationships (and is-a)
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
    relations: &Vec<&str>,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    let mut direct_sub_relations = Vec::new();

    let mut iris = HashSet::new();

    let direct_subclasses = get_direct_named_subclasses(entity, table, pool).await?;
    iris.extend(get_iris_from_set(&direct_subclasses));

    for n in 0..relations.len() {
        let direct_sub = get_direct_sub_relations(entity, relations[n], table, pool).await?;
        iris.extend(get_iris_from_set(&direct_sub));
        direct_sub_relations.push(direct_sub);
    }

    //get labels for curies
    let curie_2_label = get_label_hash_map(&iris, table, pool).await?;

    let mut children = Vec::new();
    for sub in direct_subclasses {
        let label = match curie_2_label.get(&sub) {
            Some(l) => l,
            None => &sub,
            //None => return Err(TreeViewError::TreeFormat(format!("No label for {}", sub))),
        };
        let element = json!({"curie" : sub, "label" : label, "property" : IS_A, "children" : []});
        children.push(element);
    }

    for n in 0..relations.len() {
        for sub in &direct_sub_relations[n] {
            let label = match curie_2_label.get(sub) {
                Some(l) => l,
                None => &sub,
                //None => return Err(TreeViewError::TreeFormat(format!("No label for {}", sub))),
            };
            let element =
                json!({"curie" : sub, "label" : label, "property" : relations[n], "children" : []});
            children.push(element);
        }
    }

    let children_tree = Value::Array(children);

    let sorted: Value = sort_rich_tree_by_label(&children_tree)?;
    Ok(sorted)
}

/// Given a rich term tree,
/// add grandchildren to children nodes.
///
/// # Examples
///
/// Consider the rich term tree for "zebrafish anatomical entity"
/// that currently only includes children but no grandchildren
///
/// tree = {
///          "curie": "obo:ZFA_0100000",
///          "label": "zebrafish anatomical entity",
///          "property": "rdfs:subClassOf",        
///          "children": [
///            {
///              "curie": "obo:ZFA_0000272",
///              "label": "respiratory system",   
///              "property": "rdfs:subClassOf",  
///              "children": [ ]
///             }]
///        }
///
/// add_grandchildren(tree["children"], relations, table, pool)
/// adds children nodes to all direct children in the tree, e.g.,
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
pub async fn add_grandchildren(
    children: &mut Value,
    relations: &Vec<&str>,
    table: &str,
    pool: &SqlitePool,
) -> Result<(), TreeViewError> {
    let mut grand_children = match children.as_array_mut() {
        Some(array) => array,
        None => {
            return Err(TreeViewError::TreeFormat(format!(
                "No children nodes in {}",
                children.to_string()
            )))
        }
    };

    for child in grand_children {
        let child_iri = match child["curie"].as_str() {
            Some(string) => string,
            None => {
                return Err(TreeViewError::TreeFormat(format!(
                    "No value for 'curie' field in {}",
                    child.to_string()
                )))
            }
        };
        let grand_children =
            get_immediate_children_tree(child_iri, &relations, table, pool).await?;
        child["children"] = grand_children;
    }
    Ok(())
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
pub fn add_children(tree: &mut Value, children: &Value) -> Result<(), TreeViewError> {
    match tree {
        Value::Object(_x) => {
            let tree_children = match tree["children"].as_array_mut() {
                Some(array) => array,
                None => {
                    return Err(TreeViewError::TreeFormat(format!(
                        "Couldn't access field 'children' in {}",
                        tree.to_string()
                    )))
                }
            };

            if tree_children.is_empty() {
                tree["children"] = children.clone();
            } else {
                //descend into first child
                add_children(&mut tree_children[0], children)?;
            }
            Ok(())
        }
        Value::Array(x) => {
            if x.is_empty() {
                //do nothing
            } else {
                //descend
                add_children(&mut x[0], children)?;
            }
            Ok(())
        }
        _ => Err(TreeViewError::TreeFormat(format!(
            "Expected array of child nodes or a node but got {}",
            tree.to_string()
        ))),
    }
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
) -> Result<HashSet<String>, TreeViewError> {
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
    relation_maps: &mut HashMap<String, HashMap<String, HashSet<String>>>,
    table: &str,
    pool: &SqlitePool,
) -> Result<(), TreeViewError> {
    //query for preferred roots
    let preferred_roots = get_preferred_roots(table, pool).await?;

    //collect all transitive ancestors
    let mut preferred_root_ancestor = HashSet::new();
    let mut current = HashSet::new();
    current.extend(preferred_roots);
    let mut next = HashSet::new();
    while !current.is_empty() {
        for preferred in &current {
            for (_rel, map) in relation_maps.iter() {
                if map.contains_key(preferred) {
                    for (key, value) in map {
                        if value.contains(preferred) {
                            preferred_root_ancestor.insert(key.clone());
                            next.insert(key.clone());
                        }
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
        for (_rel, map) in relation_maps.iter_mut() {
            map.remove(&ancestor);
        }
    }
    Ok(())
}

pub fn is_empty(tree: &Value) -> Result<bool, TreeViewError> {
    match tree {
        Value::Array(array) => return Ok(array.is_empty()),
        _ => {
            return Err(TreeViewError::TreeFormat(format!(
                "Expected array of root nodes {}",
                tree.to_string()
            )))
        }
    }
}

//pub fn build_root_node(

/// Given an IRI/CURIE for an entity, a list of relations (other than rdfs:subClassOf),
/// and a connection to an LDTab database, return a tree (encoded in JSON)
/// for the entity's ancestors and immediate children (and grandchildren) w.r.t.
/// rdfs:subClassOf and the specified relations.  
///
/// # Examples
///
/// Consider the entity obo:ZFA_0000354 (gill) and an LDTab data base zfa.db for zebrafish.
/// Then get_rich_json_tree_view(obo:ZFA_0000354, ["obo:BFO_0000050"], false, statement, zfa.db)
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
//TODO: this is only relevant for classes
pub async fn get_rich_json_tree_view(
    entity: &str,
    relations: &Vec<&str>,
    preferred_roots: bool,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    //get the entity's ancestor information w.r.t. subsumption and relations
    let mut relation_maps = get_hierarchy_maps(entity, &relations, table, &pool).await?;

    //modify ancestor information w.r.t. preferred root terms
    if preferred_roots {
        get_preferred_roots_hierarchy_maps(&mut relation_maps, table, pool).await?;
    }

    let roots = identify_roots(&relation_maps);

    //extract all IRI/CURIEs from the hierarchy maps (to query for their respective labels)
    let mut iris = HashSet::new();
    iris.insert(String::from(entity)); //add entity for root case (when ancestor tree is empty)
    for map in relation_maps.values() {
        iris.extend(get_iris_from_subclass_map(&map));
    }
    let curie_2_label = get_label_hash_map(&iris, table, pool).await?;

    let mut tree = build_rich_tree(&roots, &relation_maps, &curie_2_label)?;

    //if there are no ancestors for entity, then 'tree' will be an empty list
    //so, we create a root note for the entity that we can attach children to
    if is_empty(&tree)? {
        let label = match curie_2_label.get(entity) {
            Some(l) => l,
            None => entity,
        };
        tree = json!([{"curie": entity, "label":label, "property": IS_A, "children" : [] }]);
    }

    //sort tree by label
    let mut sorted = sort_rich_tree_by_label(&tree)?;

    //get direct children ...
    let mut children = get_immediate_children_tree(entity, &relations, table, pool).await?;

    //... then and grandchildren ...
    add_grandchildren(&mut children, relations, table, pool).await?;

    //... and then add these to the tree in the first occurrence of the input entity
    add_children(&mut sorted, &children)?;

    Ok(sorted)
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
pub fn tree_2_hiccup_direct_children(
    parent: &str,
    direct_children: &Value,
) -> Result<Value, TreeViewError> {
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
                let curie = match child["curie"].as_str() {
                    Some(string) => string,
                    None => {
                        return Err(TreeViewError::TreeFormat(format!(
                            "No value for field 'curie' in {}",
                            child.to_string()
                        )))
                    }
                };
                let grand_children_html = tree_2_hiccup_direct_children(curie, &child["children"])?;
                res_element.push(grand_children_html);

                res.push(Value::Array(res_element));
            }
            Ok(Value::Array(res))
        }
        _ => Err(TreeViewError::TreeFormat(format!(
            "Expected array of child nodes but got {}",
            direct_children.to_string()
        ))),
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
pub fn tree_2_hiccup_descendants(
    entity: &str,
    parent: &str,
    descendants: &Value,
) -> Result<Value, TreeViewError> {
    let mut res = Vec::new();
    res.push(json!("ul"));

    match descendants {
        Value::Array(children) => {
            for child in children {
                let mut res_elements = Vec::new();
                res_elements.push(json!("li"));

                res_elements.push(json!(["a", {"resource" : child["curie"], "about": parent, "rev":child["property"] }, child["label"] ]));

                node_2_hiccup(entity, &child, &mut res_elements)?;

                res.push(Value::Array(res_elements));
            }
            Ok(Value::Array(res))
        }
        _ => Err(TreeViewError::TreeFormat(format!(
            "Expected array of child nodes but got {}",
            descendants.to_string()
        ))),
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
///
/// Then node_2_hiccup only wraps
///
/// returns ["ul" hiccup] because there are no more children nodes to be added.
pub fn node_2_hiccup(
    entity: &str,
    node: &Value,
    hiccup: &mut Vec<Value>,
) -> Result<(), TreeViewError> {
    let curie = match node["curie"].as_str() {
        Some(string) => string,
        None => {
            return Err(TreeViewError::TreeFormat(format!(
                "No value for 'curie' field in {}",
                node.to_string()
            )))
        }
    };
    if curie.eq(entity) {
        //base case for direct children
        let direct_children = tree_2_hiccup_direct_children(curie, &node["children"])?;
        hiccup.push(direct_children);
        Ok(())
    } else {
        //recursive call for nested children
        let descendants = tree_2_hiccup_descendants(entity, curie, &node["children"])?;
        hiccup.push(descendants);
        Ok(())
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
pub fn tree_2_hiccup(entity: &str, tree: &Value) -> Result<Value, TreeViewError> {
    let mut tree_hiccup = Vec::new();
    tree_hiccup.push(json!("ul"));

    match tree {
        //the tree consist of an array of root notes (i.e. it might be a forest)
        Value::Array(roots) => {
            for root in roots {
                let mut node_hiccup = Vec::new();

                node_hiccup.push(json!("li"));

                node_hiccup.push(json!(["a", {"resource" : root["curie"] }, root["label"] ]));

                node_2_hiccup(entity, &root, &mut node_hiccup)?;

                tree_hiccup.push(Value::Array(node_hiccup));
            }
            Ok(Value::Array(tree_hiccup))
        }
        _ => Err(TreeViewError::TreeFormat(format!(
            "Expected array of root nodes but got {}",
            tree.to_string()
        ))),
    }
}

/// Given a CURIE/IRI of a class entity (note that we don't check the rdf:type),
/// a vector of relations, and an LDTab database,
/// return a term tree for the entity.
///
/// # Examples
///
/// Consider the entity obo:ZFA_0000354 and an
/// LDTab database with information about the ZFA ontology.
/// Then, get_hiccup_term_tre(obo:ZFA_0000354, [], false, statement, zfa)
/// will return a term tree of the following form (only an excerpt is shown):
///
/// ["ul",
///   ["li",
///     ["a", {"resource": "owl:Class"}, "Class"],
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
pub async fn get_hiccup_class_tree(
    entity: &str,
    relations: &Vec<&str>,
    preferred_roots: bool,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    let tree = get_rich_json_tree_view(entity, relations, preferred_roots, table, pool).await?;

    let roots = tree_2_hiccup(entity, &tree)?;

    let mut res = Vec::new();
    //a term tree is displayed below owl:Class by definition
    res.push(json!("ul"));
    let class = json!(["a", {"resource" : "owl:Class"}, "Class"]);
    res.push(json!(["li", class, roots]));

    Ok(Value::Array(res))
}

/// Given a CURIE/IRI of a property entity, a vector of relations, and an LDTab database,
/// return a term tree for the entity.
///
/// # Examples
///
/// Consider the entity obo:RO_0002131 and an
/// LDTab database with information about the ZFA ontology.
/// Then, get_hiccup_term_tre(obo:RO_0002131, false, statement, zfa)
/// will return a term tree of the following form (only an excerpt is shown):
///
/// ["ul",
///   ["li",
///     ["a", {"resource": "owl:ObjectProperty"}, "Object Property"],
///     ["ul",
///       ["li",
///         ["a",{"resource": "obo:RO_0002131"},"overlaps"],
///         ["ul",
///           ["li",
///             ["a",{
///                 "resource": "obo:BFO_0000051",
///                 "about": "obo:RO_0002131",
///                 "rev": "rdfs:subPropertyOf"},
///               "has part"
///             ],
///             ["ul",
///              ["li",
///                ["a",{
///                    "resource": "obo:BFO_0000050",
///                    "about": "obo:RO_0002131",
///                    "rev": "rdfs:subPropertyOf"
///                  },
///                  "part of"
///                ],
///             ...  
pub async fn get_hiccup_property_tree(
    entity: &str,
    case: OWLEntityType,
    preferred_roots: bool,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    let tree = get_rich_json_property_tree_view(entity, preferred_roots, table, pool).await?;

    let roots = tree_2_hiccup(entity, &tree)?;

    let mut res = Vec::new();
    //a term tree is displayed below owl:Class by definition
    res.push(json!("ul"));

    let property = match case {
        OWLEntityType::AnnotationProperty => {
            json!(["a", {"resource" : "owl:AnnotationProperty"}, "Annotation Property"])
        }
        OWLEntityType::DataProperty => {
            json!(["a", {"resource" : "owl:DataProperty"}, "Data Property"])
        }
        OWLEntityType::ObjectProperty => {
            json!(["a", {"resource" : "owl:ObjectProperty"}, "Object Property"])
        }
        _ => {
            return Err(TreeViewError::Unknown(format!(
                "Expected property type but got {:?}",
                case
            )))
        }
    };
    res.push(json!(["li", property, roots]));

    Ok(Value::Array(res))
}

/// Build database query for top level entities.
pub async fn build_top_level_query(case: OWLEntityType, table: &str, pool: &SqlitePool) -> String {
    let mut top = "";
    let mut relation = "";
    let mut rdf_type = "";

    match case {
        OWLEntityType::Class => {
            top = "owl:Thing";
            relation = IS_A;
            rdf_type = "owl:Class"
        }
        OWLEntityType::ObjectProperty => {
            top = "owl:topObjectProperty";
            relation = SUBPROPERTY;
            rdf_type = "owl:ObjectProperty";
        }
        OWLEntityType::DataProperty => {
            top = "owl:topDataProperty";
            relation = SUBPROPERTY;
            rdf_type = "owl:DatatypeProperty";
        }
        OWLEntityType::AnnotationProperty => {
            top = "owl:topAnnotationProperty";
            relation = SUBPROPERTY;
            rdf_type = "owl:AnnotationProperty";
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
    query
}

/// Return a hiccup-style list of all the top-level nodes.
///
/// # Examples
///
/// The call get_hiccup_top_hierarchy("statement", &zfa_connection)
/// returns (only an excerpt is shown)
///
/// ["ul",
///   ["li","Class",
///     ["ul", { "id": "children" },
///       ["li",
///         ["a",
///           {
///             "resource": "obo:ZFA_0000460",
///             "rev": "rdfs:subClassOf"
///           },
///           "Mauthner axon"
///         ]
///       ],
///       ["li",
///         ["a",
///           {
///             "resource": "obo:ZFS_0100000",
///             "rev": "rdfs:subClassOf"
///           },
///           "Stages"
///         ]
///       ],
///       ,
///      ...
///     ]
///   ]
/// ]
pub async fn get_hiccup_top_class_hierarchy(
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    let query = build_top_level_query(OWLEntityType::Class, table, pool).await;
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    //collect entities
    let mut entities = HashSet::new();
    for row in rows {
        let subject: &str = row.get("subject");
        entities.insert(String::from(subject));
    }

    let entity_2_label = get_label_hash_map(&entities, table, pool).await?;

    //build term trees for top level nodes
    let mut top_hierarchy_nodes = Vec::new();
    for entity in &entities {
        let mut root_tree = match entity_2_label.get(entity) {
            Some(label) => {
                json!({"curie":entity, "label":label, "property" : IS_A, "children" : []})
            }
            None => {
                json!({"curie":entity, "label":entity, "property" : IS_A, "children" : []})
            }
        };

        //add children & grandchildren
        let mut children = get_immediate_children_tree(entity, &vec![IS_A], table, pool).await?;
        add_grandchildren(&mut children, &vec![IS_A], table, pool).await?;
        add_children(&mut root_tree, &children)?;

        top_hierarchy_nodes.push(root_tree);
    }

    //add top level nodes as children of "owl:Class"
    let owl_class_children = Value::Array(top_hierarchy_nodes);
    let top_hierarchy_tree = json!([{"curie":"owl:Class", "label":"Class", "property":IS_A, "children": owl_class_children }]);
    let sorted = sort_rich_tree_by_label(&top_hierarchy_tree)?;
    let hiccup = tree_2_hiccup("owl:Class", &sorted)?;
    Ok(hiccup)
}

/// Return a hiccup-style list of all the top-level nodes for properties.
///
/// # Examples
///
/// The call get_hiccup_top_hierarchy("statement", &zfa_connection)
/// returns (only an excerpt is shown)
///
/// ["ul",
///   ["li","Object Property",
///     ["ul", { "id": "children" },
///       ["li",
///         ["a",
///           {
///             "resource": "obo:RO_0002131",
///             "rev": "rdfs:subPropertyOf"
///           },
///           "overlaps"
///         ]
///       ],
///      ...
///     ]
///   ]
/// ]
pub async fn get_hiccup_top_property_hierarchy(
    case: OWLEntityType,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    match case.clone() {
        OWLEntityType::AnnotationProperty => {}
        OWLEntityType::DataProperty => {}
        OWLEntityType::ObjectProperty => {}
        _ => {
            return Err(TreeViewError::Unknown(format!(
                "Expected property type but got {:?}",
                case
            )))
        }
    }

    let query = build_top_level_query(case.clone(), table, pool).await;
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    //collect entities
    let mut entities = HashSet::new();
    for row in rows {
        let subject: &str = row.get("subject");
        entities.insert(String::from(subject));
    }

    let entity_2_label = get_label_hash_map(&entities, table, pool).await?;

    //build term trees for top level nodes
    let mut top_hierarchy_nodes = Vec::new();
    for entity in &entities {
        let mut root_tree = match entity_2_label.get(entity) {
            Some(label) => {
                json!({"curie":entity, "label":label, "property" : SUBPROPERTY, "children" : []})
            }
            None => {
                json!({"curie":entity, "label":entity, "property" : SUBPROPERTY, "children" : []})
            }
        };

        //add children & grandchildren
        let mut children = get_immediate_property_children_tree(entity, table, pool).await?;
        add_property_grandchildren(&mut children, table, pool).await?;
        add_children(&mut root_tree, &children)?;

        top_hierarchy_nodes.push(root_tree);
    }

    let owl_property_children = Value::Array(top_hierarchy_nodes);
    let mut top_hierarchy_tree = json!("to be modified");
    match case {
        OWLEntityType::AnnotationProperty => {
            top_hierarchy_tree = json!([{"curie":"owl:AnnotationProperty", "label":"Annotation Property", "property":SUBPROPERTY, "children": owl_property_children }]);
        }
        OWLEntityType::DataProperty => {
            top_hierarchy_tree = json!([{"curie":"owl:DataProperty", "label":"Data Property", "property":SUBPROPERTY, "children": owl_property_children }]);
        }
        OWLEntityType::ObjectProperty => {
            top_hierarchy_tree = json!([{"curie":"owl:ObjectProperty", "label":"Object Property", "property":SUBPROPERTY, "children": owl_property_children }]);
        }
        _ => {
            return Err(TreeViewError::Unknown(format!(
                "Expected property type but got {:?}",
                case
            )))
        }
    }

    let sorted = sort_rich_tree_by_label(&top_hierarchy_tree)?;
    let hiccup = tree_2_hiccup("owl:Class", &sorted)?;
    Ok(hiccup)
}

pub fn get_list_encoding(case: OWLEntityType) -> Value {
    match case {
        OWLEntityType::Ontology => {
            json!(["ul", ["li", ["a", {"resource":"owl:Ontology"}, "Ontology" ]]])
        }
        OWLEntityType::Class => json!(["ul", ["li", ["a", {"resource":"owl:Class"}, "Class" ]]]),
        OWLEntityType::AnnotationProperty => {
            json!(["ul", ["li", ["a", {"resource":"owl:AnnotationProperty"}, "Annotation Property" ]]])
        }
        OWLEntityType::DataProperty => {
            json!(["ul", ["li", ["a", {"resource":"owl:DataProperty"}, "Data Property" ]]])
        }
        OWLEntityType::ObjectProperty => {
            json!(["ul", ["li", ["a", {"resource":"owl:ObjectProperty"}, "Object Property"]]])
        }
        OWLEntityType::Individual => {
            json!(["ul", ["li", ["a", {"resource":"owl:Individual"}, "Individual" ]]])
        }
        OWLEntityType::Datatype => {
            json!(["ul", ["li", ["a", {"resource":"owl:Datatype"}, "Datatype" ]]])
        }
    }
}

/// Given an IRI/CURIE for a top level entity in the term tree view,
/// determine its rdf:type and return the associated OWLEntityType.
pub async fn get_type(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<OWLEntityType, TreeViewError> {
    let query = format!(
        "SELECT subject, predicate, object FROM {table} WHERE subject='{entity}' AND predicate='rdf:type'",table=table, entity=entity);
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    for row in rows {
        let entity: &str = row.get("subject");
        let rdf_type: &str = row.get("object");
        match rdf_type {
            "owl:Ontology" => return Ok(OWLEntityType::Ontology),
            "owl:Class" => return Ok(OWLEntityType::Class),
            "owl:AnnotationProperty" => return Ok(OWLEntityType::AnnotationProperty),
            "owl:ObjectProperty" => return Ok(OWLEntityType::ObjectProperty),
            "owl:DataProperty" => return Ok(OWLEntityType::DataProperty),
            "owl:Individual" => return Ok(OWLEntityType::Individual),
            "owl:NamedIndividual" => return Ok(OWLEntityType::Individual),
            "rdfs:Datatype" => return Ok(OWLEntityType::Datatype),
            _ => {}
        }
    }
    return Err(TreeViewError::LDTab(format!(
        "No suitable rdf:type for {} in LDTab table {}",
        entity, table
    )));
}

/// Given an IRI/CURIE for an entity,
/// return an HTML view of its associated term tree.
pub async fn get_html_term_tree(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<String, TreeViewError> {
    let hiccup = get_hiccup_term_tree(entity, table, pool).await?;
    let html = hiccup::render(&hiccup, 0);
    Ok(html)
}

/// Given an IRI/CURIE for an entity,
/// return a hiccup-style list for its associated term tree.
pub async fn get_hiccup_term_tree(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<Value, TreeViewError> {
    let mut list_view = Vec::new();

    list_view.push(json!("ul"));
    list_view.push(get_list_encoding(OWLEntityType::Ontology));
    list_view.push(get_list_encoding(OWLEntityType::Class));
    list_view.push(get_list_encoding(OWLEntityType::AnnotationProperty));
    list_view.push(get_list_encoding(OWLEntityType::DataProperty));
    list_view.push(get_list_encoding(OWLEntityType::ObjectProperty));
    list_view.push(get_list_encoding(OWLEntityType::Individual));
    list_view.push(get_list_encoding(OWLEntityType::Datatype));

    //top hierarchy
    match entity {
        "owl:Class" => {
            let term_tree = get_hiccup_top_class_hierarchy(table, pool).await?;
            list_view[2] = term_tree;
            return Ok(Value::Array(list_view));
        }
        "owl:AnnotationProperty" => {
            let term_tree =
                get_hiccup_top_property_hierarchy(OWLEntityType::AnnotationProperty, table, pool)
                    .await?;
            list_view[3] = term_tree;
            return Ok(Value::Array(list_view));
        }
        "owl:DataProperty" => {
            let term_tree =
                get_hiccup_top_property_hierarchy(OWLEntityType::DataProperty, table, pool).await?;
            list_view[3] = term_tree;
            return Ok(Value::Array(list_view));
        }
        "owl:ObjectProperty" => {
            let term_tree =
                get_hiccup_top_property_hierarchy(OWLEntityType::ObjectProperty, table, pool)
                    .await?;
            list_view[5] = term_tree;
            return Ok(Value::Array(list_view));
        }
        _ => {}
    }

    let rdf_type = get_type(entity, table, pool).await?;
    match rdf_type {
        OWLEntityType::Class => {
            //TODO: use a config struct?
            //use part-of by default
            let relations = vec!["obo:BFO_0000050"];
            //don't use preferred root nodes by default
            let term_tree = get_hiccup_class_tree(entity, &relations, false, table, pool).await?;
            list_view[2] = term_tree;
        }
        OWLEntityType::AnnotationProperty => {
            let term_tree = get_hiccup_property_tree(
                entity,
                OWLEntityType::AnnotationProperty,
                false,
                table,
                pool,
            )
            .await?;
            list_view[3] = term_tree;
        }
        OWLEntityType::DataProperty => {
            let term_tree =
                get_hiccup_property_tree(entity, OWLEntityType::DataProperty, false, table, pool)
                    .await?;
            list_view[4] = term_tree;
        }
        OWLEntityType::ObjectProperty => {
            let term_tree =
                get_hiccup_property_tree(entity, OWLEntityType::ObjectProperty, false, table, pool)
                    .await?;
            list_view[5] = term_tree;
        }

        _ => {}
    }

    Ok(Value::Array(list_view))
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
