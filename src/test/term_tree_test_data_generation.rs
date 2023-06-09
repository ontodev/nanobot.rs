//NB: This is a copy of tree_view.rs.
//The only difference is that it collects the results of all executed SQL queries and writes them to a file.
use crate::test::part_of_term_tree::{
    add_children, build_rich_tree, get_iris_from_set, get_iris_from_subclass_map,
    get_part_of_information, identify_invalid_classes, identify_roots, ldtab_2_value,
    remove_invalid_classes, sort_rich_tree_by_label, update_hierarchy_map,
};
use serde_json::{json, Value};
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;

static PART_OF: &'static str = "obo:BFO_0000050";
static IS_A: &'static str = "rdfs:subClassOf";

#[derive(Debug)]
pub struct LabelNotFound;

pub async fn get_preferred_roots(
    table: &str,
    pool: &SqlitePool,
) -> Result<(HashSet<String>, HashSet<String>), sqlx::Error> {
    let mut row_strings: HashSet<String> = HashSet::new();
    let mut preferred_roots = HashSet::new();
    let query = format!(
        "SELECT assertion, retraction, graph, subject, predicate, object, datatype, annotation FROM {table} WHERE predicate='obo:IAO_0000700'",
        table = table,
    );
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let assertion: u32 = row.get("assertion");
        let retraction: u32 = row.get("retraction");
        let graph: &str = row.get("graph");
        let subject: &str = row.get("subject"); //subclass
        let predicate: &str = row.get("predicate");
        let object: &str = row.get("object");
        let datatype: &str = row.get("datatype");
        let annotation: &str = row.get("annotation");

        let row_string = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            assertion, retraction, graph, subject, predicate, object, datatype, annotation
        );
        row_strings.insert(row_string);

        preferred_roots.insert(String::from(object));
    }

    Ok((preferred_roots, row_strings))
}

pub async fn get_direct_subclasses(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<(HashSet<String>, HashSet<String>), sqlx::Error> {
    let mut subclasses = HashSet::new();
    let mut row_strings: HashSet<String> = HashSet::new();

    let query = format!(
        "SELECT assertion, retraction, graph, subject, predicate, object, datatype, annotation FROM {table} WHERE object='{entity}' AND predicate='rdfs:subClassOf'",
        table = table,
        entity = entity,
    );

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let assertion: u32 = row.get("assertion");
        let retraction: u32 = row.get("retraction");
        let graph: &str = row.get("graph");
        let subject: &str = row.get("subject"); //subclass
        let predicate: &str = row.get("predicate");
        let object: &str = row.get("object");
        let datatype: &str = row.get("datatype");
        let annotation: &str = row.get("annotation");

        let row_string = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            assertion, retraction, graph, subject, predicate, object, datatype, annotation
        );
        row_strings.insert(row_string);

        subclasses.insert(String::from(subject));
    }

    Ok((subclasses, row_strings))
}

pub async fn get_direct_named_subclasses(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<(HashSet<String>, HashSet<String>), sqlx::Error> {
    let (subclasses, row_strings) = get_direct_subclasses(entity, table, pool).await?;

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
    Ok((is_a, row_strings))
}

pub async fn get_direct_sub_parts(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<(HashSet<String>, HashSet<String>), sqlx::Error> {
    let mut sub_parts = HashSet::new();

    let mut row_strings: HashSet<String> = HashSet::new();

    //RDF representation of an OWL existential restriction
    //using the property part-of (obo:BFO_0000050)
    let part_of = r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"entity"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#;
    let part_of = part_of.replace("entity", entity);

    let query = format!(
        "SELECT assertion, retraction, graph, subject, predicate, object, datatype, annotation FROM {table} WHERE object='{part_of}' AND predicate='rdfs:subClassOf'",
        table = table,
        part_of = part_of,
    );

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let assertion: u32 = row.get("assertion");
        let retraction: u32 = row.get("retraction");
        let graph: &str = row.get("graph");
        let subject: &str = row.get("subject"); //subclass
        let predicate: &str = row.get("predicate");
        let object: &str = row.get("object");
        let datatype: &str = row.get("datatype");
        let annotation: &str = row.get("annotation");

        let row_string = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            assertion, retraction, graph, subject, predicate, object, datatype, annotation
        );
        row_strings.insert(row_string);

        //filter for named classes
        match ldtab_2_value(&subject) {
            Value::String(_x) => {
                sub_parts.insert(String::from(subject));
            }
            _ => {}
        };
    }
    Ok((sub_parts, row_strings))
}

pub async fn get_immediate_children_tree(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<(Value, HashSet<String>), sqlx::Error> {
    let mut row_strings: HashSet<String> = HashSet::new();

    //get the entity's immediate descendents w.r.t. subsumption and parthood relations
    let (direct_subclasses, sub_row_strings) =
        get_direct_named_subclasses(entity, table, pool).await?;
    row_strings.extend(sub_row_strings);
    let (direct_part_ofs, part_row_strings) = get_direct_sub_parts(entity, table, pool).await?;
    row_strings.extend(part_row_strings);

    //get labels
    let mut iris = HashSet::new();
    iris.extend(get_iris_from_set(&direct_subclasses));
    iris.extend(get_iris_from_set(&direct_part_ofs));

    //get labels for curies
    let (curie_2_label, label_row_strings) = get_label_hash_map(&iris, table, pool).await?;
    row_strings.extend(label_row_strings);

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
    Ok((sorted, row_strings))
}

pub async fn get_preferred_roots_hierarchy_maps(
    class_2_subclasses: &mut HashMap<String, HashSet<String>>,
    class_2_parts: &mut HashMap<String, HashSet<String>>,
    table: &str,
    pool: &SqlitePool,
) -> HashSet<String> {
    //query for preferred roots
    let (preferred_roots, row_strings) = get_preferred_roots(table, pool).await.unwrap();

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
    row_strings
}

pub fn build_label_query_for(curies: &HashSet<String>, table: &str) -> String {
    let quoted_curies: HashSet<String> = curies.iter().map(|x| format!("'{}'", x)).collect();
    let joined_quoted_curies = itertools::join(&quoted_curies, ",");
    let query = format!(
        "SELECT assertion, retraction, graph, subject, predicate, object, datatype, annotation FROM {table} WHERE subject IN ({curies}) AND predicate='rdfs:label'",
        table=table,
        curies=joined_quoted_curies
    );
    query
}

pub async fn get_label_hash_map(
    curies: &HashSet<String>,
    table: &str,
    pool: &SqlitePool,
) -> Result<(HashMap<String, String>, HashSet<String>), sqlx::Error> {
    let mut row_strings: HashSet<String> = HashSet::new();
    let mut entity_2_label = HashMap::new();
    let query = build_label_query_for(&curies, table);
    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;
    for row in rows {
        let assertion: u32 = row.get("assertion");
        let retraction: u32 = row.get("retraction");
        let graph: &str = row.get("graph");
        let subject: &str = row.get("subject"); //subclass
        let predicate: &str = row.get("predicate");
        let object: &str = row.get("object");
        let datatype: &str = row.get("datatype");
        let annotation: &str = row.get("annotation");
        entity_2_label.insert(String::from(subject), String::from(object));

        //let row_string = format!("{},{},{},{},{},{},{},{}", assertion, retraction, graph, subject, predicate, object, datatype, annotation);
        let row_string = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            assertion, retraction, graph, subject, predicate, object, datatype, annotation
        );
        row_strings.insert(row_string);
    }
    Ok((entity_2_label, row_strings))
}

pub async fn get_class_2_subclass_map(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<(HashMap<String, HashSet<String>>, HashSet<String>), sqlx::Error> {
    let mut row_strings: HashSet<String> = HashSet::new();
    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();

    //recursive SQL query for transitive 'is-a' relationships
    let query = format!("WITH RECURSIVE
    superclasses( assertion, retraction, graph, subject, predicate, object, datatype, annotation ) AS
    ( SELECT assertion, retraction, graph, subject, predicate, object, datatype, annotation FROM {table} WHERE subject='{entity}' AND predicate='rdfs:subClassOf'
        UNION ALL
        SELECT {table}.assertion, {table}.retraction, {table}.graph, {table}.subject, {table}.predicate, {table}.object, {table}.datatype, {table}.annotation FROM {table}, superclasses WHERE {table}.subject = superclasses.object AND {table}.predicate='rdfs:subClassOf'
     ) SELECT * FROM superclasses;", table=table, entity=entity);

    let rows: Vec<SqliteRow> = sqlx::query(&query).fetch_all(pool).await?;

    for row in rows {
        //axiom structure: subject rdfs:subClassOf object
        let assertion: u32 = row.get("assertion");
        let retraction: u32 = row.get("retraction");
        let graph: &str = row.get("graph");
        let subject: &str = row.get("subject"); //subclass
        let predicate: &str = row.get("predicate");
        let object: &str = row.get("object");
        let datatype: &str = row.get("datatype");
        let annotation: &str = row.get("annotation");

        let row_string = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            assertion, retraction, graph, subject, predicate, object, datatype, annotation
        );
        row_strings.insert(row_string);

        //println!("row : {}", row_string);

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
    Ok((class_2_subclasses, row_strings))
}

pub async fn get_hierarchy_maps(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<
    (HashMap<String, HashSet<String>>, HashMap<String, HashSet<String>>, HashSet<String>),
    sqlx::Error,
> {
    //both maps are build by iteratively querying for
    //combinations of is-a and part-of relationships.
    //In particular, this means querying for is-a relations
    //w.r.t. classes that are used as fillers in axioms for part-of relations.
    let mut class_2_subclasses: HashMap<String, HashSet<String>> = HashMap::new();
    let mut class_2_parts: HashMap<String, HashSet<String>> = HashMap::new();

    let mut rows = HashSet::new();

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
            let (subclasses_updates, row_strings) =
                get_class_2_subclass_map(&update, table, pool).await?;
            rows.extend(row_strings);
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

    Ok((class_2_subclasses, class_2_parts, rows))
}

pub async fn get_label(
    entity: &str,
    table: &str,
    pool: &SqlitePool,
) -> Result<(String, HashSet<String>), LabelNotFound> {
    let mut row_strings: HashSet<String> = HashSet::new();
    let query =
        format!("SELECT * FROM {} WHERE subject='{}' AND predicate='rdfs:label'", table, entity);
    let rows: Vec<SqliteRow> =
        sqlx::query(&query).fetch_all(pool).await.map_err(|_| LabelNotFound)?;
    //NB: this should be a singleton
    for row in rows {
        //let subject: &str = row.get("subject");
        let assertion: u32 = row.get("assertion");
        let retraction: u32 = row.get("retraction");
        let graph: &str = row.get("graph");
        let subject: &str = row.get("subject"); //subclass
        let predicate: &str = row.get("predicate");
        let object: &str = row.get("object");
        let datatype: &str = row.get("datatype");
        let annotation: &str = row.get("annotation");

        let row_string = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            assertion, retraction, graph, subject, predicate, object, datatype, annotation
        );
        row_strings.insert(row_string);

        let label: &str = row.get("object");
        return Ok((String::from(label), row_strings));
    }

    Err(LabelNotFound)
}

pub async fn get_html_top_hierarchy(
    case: &str,
    table: &str,
    pool: &SqlitePool,
    output: &str,
) -> Result<Value, sqlx::Error> {
    let mut top = "";
    let mut relation = "";
    let mut rdf_type = "";
    let mut row_strings: HashSet<String> = HashSet::new();

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
        "SELECT s1.assertion, s1.retraction, s1.graph, s1.subject, s1.predicate, s1.object, s1.datatype, s1.annotation
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
            SELECT assertion, retraction, graph, subject, predicate, object, datatype, annotation
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
        let assertion: u32 = row.get("assertion");
        let retraction: u32 = row.get("retraction");
        let graph: &str = row.get("graph");
        let subject: &str = row.get("subject"); //subclass
        let predicate: &str = row.get("predicate");
        let object: &str = row.get("object");
        let datatype: &str = row.get("datatype");
        let annotation: &str = row.get("annotation");

        let row_string = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            assertion, retraction, graph, subject, predicate, object, datatype, annotation
        );
        row_strings.insert(row_string);

        //TODO: remove these label calls
        let subject_label = get_label(subject, table, pool).await;
        //row_strings.extend(label_rows);

        match subject_label {
            Ok((x, y)) => {
                children_list.push(
                    json!(["li", ["a", {"resource":subject, "rev" : "rdfs:subClassOf"}, x  ]]),
                );
                row_strings.extend(y);
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

    if !Path::new(output).exists() {
        File::create(output)?;
    }
    let mut file = OpenOptions::new().write(true).append(true).open(output).unwrap();
    for row in row_strings {
        if let Err(e) = writeln!(file, "{}", row) {
            eprintln!("Couldn't write to file: {}", e);
        }
    }

    Ok(Value::Array(res))
}

pub async fn get_rich_json_tree_view(
    entity: &str,
    preferred_roots: bool,
    table: &str,
    pool: &SqlitePool,
    output: &str,
) -> Result<Value, sqlx::Error> {
    let mut rows = HashSet::new();

    //get the entity's ancestor information w.r.t. subsumption and parthood relations
    let (mut class_2_subclasses, mut class_2_parts, row_strings) =
        get_hierarchy_maps(entity, table, &pool).await?;

    rows.extend(row_strings);

    //modify ancestor information w.r.t. preferred root terms
    //TODO: this generates rows
    if preferred_roots {
        let row_strings = get_preferred_roots_hierarchy_maps(
            &mut class_2_subclasses,
            &mut class_2_parts,
            table,
            pool,
        )
        .await;
        rows.extend(row_strings);
    }

    //get elements with no ancestors (i.e., roots)
    let roots = identify_roots(&class_2_subclasses, &class_2_parts);

    //extract CURIEs/IRIs
    let mut iris = HashSet::new();
    iris.extend(get_iris_from_subclass_map(&class_2_subclasses));
    iris.extend(get_iris_from_subclass_map(&class_2_parts));

    //get labels for CURIEs/IRIs
    let (curie_2_label, row_strings) = get_label_hash_map(&iris, table, pool).await?;
    rows.extend(row_strings);

    //build JSON tree
    let tree = build_rich_tree(&roots, &class_2_subclasses, &class_2_parts, &curie_2_label);

    //sort tree by label
    let mut sorted = sort_rich_tree_by_label(&tree);

    //get immediate children of leaf entities in the ancestor tree
    //NB: sorting the tree first ensures that the tree with added children is deterministic
    let (mut children, children_rows) = get_immediate_children_tree(entity, table, pool).await?;
    rows.extend(children_rows);

    //add childrens of children to children
    for child in children.as_array_mut().unwrap() {
        let child_iri = child["curie"].as_str().unwrap();
        let (grand_children, grand_children_rows) =
            get_immediate_children_tree(child_iri, table, pool).await?;
        rows.extend(grand_children_rows);
        child["children"] = grand_children;
    }

    //add children to the first occurrence of their respective parents in the (sorted) JSON tree
    add_children(&mut sorted, &children);

    if !Path::new(output).exists() {
        File::create(output)?;
    }
    let mut file = OpenOptions::new().write(true).append(true).open(output).unwrap();
    for row in rows {
        if let Err(e) = writeln!(file, "{}", row) {
            eprintln!("Couldn't write to file: {}", e);
        }
    }
    Ok(sorted)
}
