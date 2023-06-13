use crate::test::part_of_term_tree::{
    get_hierarchy_maps, get_iris_from_subclass_map, get_label_hash_map, identify_roots,
};
use serde_json::{json, Map, Value};
use sqlx::sqlite::SqlitePool;
use std::collections::{HashMap, HashSet};

/// Given an entity and an LDTab database,
/// return a term tree (encoded in JSON) for the entity that displays information about
/// the 'is-a' as well as 'part-of' relationships
///
/// # Examples
///
/// Calling the function with the entity obo:ZFA_0000354 and an LDTab database with
/// information about the zebrafish ontology yields the following term tree:
///
/// {
///   "obo:ZFA_0100000": {
///     "obo:ZFA_0000037": {
///       "obo:ZFA_0001512": {
///         "obo:ZFA_0001439": {
///           "obo:ZFA_0000272": {
///             "partOf obo:ZFA_0000354": "owl:Nothing"
///           }
///         }
///       },
///       "obo:ZFA_0000496": {                 <- is-a ancestor of obo:ZFA_0000354
///         "obo:ZFA_0000354": "owl:Nothing"   <- target entity obo:ZFA_0000354 (with child "owl:Nothing")
///       },
///       "obo:ZFA_0001094": {
///         "partOf obo:ZFA_0001439": {        <- part-of ancestor of obo:ZFA_0000354
///           "obo:ZFA_0000272": {
///             "partOf obo:ZFA_0000354": "owl:Nothing"
///           }
///         },
///         "partOf obo:ZFA_0000496": {
///           "obo:ZFA_0000354": "owl:Nothing"
///         }
///       }
///     }
///   }
/// }
pub async fn get_json_tree_view(entity: &str, table: &str, pool: &SqlitePool) -> Value {
    //extract information about an entities 'is-a' and 'part-of' relationships
    let (class_2_subclasses, class_2_parts) =
        get_hierarchy_maps(entity, table, &pool).await.unwrap();
    let roots = identify_roots(&class_2_subclasses, &class_2_parts);
    build_tree(&roots, &class_2_subclasses, &class_2_parts)
}

/// Given a set of entities, and maps for their subclass and parthood relations,
/// return an encoding of the term tree via JSON objects.
///
/// # Examples
///
/// Consider the maps
///
/// is_a_map = {
///  "obo:ZFA_0000496": {"obo:ZFA_0000354"},
///  "obo:ZFA_0000037": {"obo:ZFA_0001512", "obo:ZFA_0001094", "obo:ZFA_0000496"},
///  "obo:ZFA_0001439": {"obo:ZFA_0000272"},
///  "obo:ZFA_0100000": {"obo:ZFA_0000037"},
///  "obo:ZFA_0001512": {"obo:ZFA_0001439"}
/// }
///
/// part_of_map = {
///  "obo:ZFA_0001094": {"obo:ZFA_0000496", "obo:ZFA_0001439"},
///  "obo:ZFA_0000272": {"obo:ZFA_0000354"}
/// }
///
/// then build_tree({obo:ZFA_0000354}, is_a_map, part_of_map) returns
///
/// {
///   "obo:ZFA_0100000": {
///     "obo:ZFA_0000037": {
///       "obo:ZFA_0001512": {
///         "obo:ZFA_0001439": {
///           "obo:ZFA_0000272": {
///             "partOf obo:ZFA_0000354": "owl:Nothing"
///           }
///         }
///       },
///       "obo:ZFA_0000496": {                 <- is-a ancestor of obo:ZFA_0000354
///         "obo:ZFA_0000354": "owl:Nothing"   <- target entity obo:ZFA_0000354 (with child "owl:Nothing")
///       },
///       "obo:ZFA_0001094": {
///         "partOf obo:ZFA_0001439": {        <- part-of ancestor of obo:ZFA_0000354
///           "obo:ZFA_0000272": {
///             "partOf obo:ZFA_0000354": "owl:Nothing"
///           }
///         },
///         "partOf obo:ZFA_0000496": {
///           "obo:ZFA_0000354": "owl:Nothing"
///         }
///       }
///     }
///   }
/// }
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
        //NB: we add owl:Nothing as a child
        // (even if there is no axiom with owl:Nothing as a subclass)
        if !class_2_subclasses.contains_key(i) & !class_2_parts.contains_key(i) {
            json_map.insert(String::from(i), Value::String(String::from("owl:Nothing")));
        }
    }
    Value::Object(json_map)
}

/// Given an entity, and maps for subclass and parthood relations between classes,
/// return an is-a branch in the entity's term tree
///
/// # Examples
///
/// Consider the maps
///
/// is_a_map = {
///  "obo:ZFA_0000496": {"obo:ZFA_0000354"},
///  "obo:ZFA_0000037": {"obo:ZFA_0001512", "obo:ZFA_0001094", "obo:ZFA_0000496"},
///  "obo:ZFA_0001439": {"obo:ZFA_0000272"},
///  "obo:ZFA_0100000": {"obo:ZFA_0000037"},
///  "obo:ZFA_0001512": {"obo:ZFA_0001439"}
/// }
///
/// part_of_map = {
///  "obo:ZFA_0001094": {"obo:ZFA_0000496", "obo:ZFA_0001439"},
///  "obo:ZFA_0000272": {"obo:ZFA_0000354"}
/// }
///
/// then build_is_a_branch(obo:ZFA_0000496, is_a_map, part_of_map) returns the branch
///
///  "obo:ZFA_0000496": {                 
///    "obo:ZFA_0000354": "owl:Nothing"   
///  },
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

/// Given an entity, and maps for subclass and parthood relations between classes,
/// return an part-of branch in the entity's term tree
///
/// # Examples
///
/// Consider the maps
///
/// is_a_map = {
///  "obo:ZFA_0000496": {"obo:ZFA_0000354"},
///  "obo:ZFA_0000037": {"obo:ZFA_0001512", "obo:ZFA_0001094", "obo:ZFA_0000496"},
///  "obo:ZFA_0001439": {"obo:ZFA_0000272"},
///  "obo:ZFA_0100000": {"obo:ZFA_0000037"},
///  "obo:ZFA_0001512": {"obo:ZFA_0001439"}
/// }
///
/// part_of_map = {
///  "obo:ZFA_0001094": {"obo:ZFA_0000496", "obo:ZFA_0001439"},
///  "obo:ZFA_0000272": {"obo:ZFA_0000354"}
/// }
///
/// then build_is_a_branch(obo:ZFA_0000496, is_a_map, part_of_map) returns the branch
///
///  "partOf obo:ZFA_0000496": {
///    "obo:ZFA_0000354": "owl:Nothing"
///  }
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

/// Given an entity's term tree, and maps entities' labels
/// return the tree with IRIs/CURIEs replaced with their labels.  
///
/// # Examples
///
/// Consider the label map
///
/// label_map = {
///  "obo:ZFA_0001094": "whole organism",
///  "obo:ZFA_0001512": "anatomical group",
///  "obo:ZFA_0100000": "zebrafish anatomical entity",
///  "obo:ZFA_0001439": "anatomical system",
///  "obo:ZFA_0000272": "respiratory system",
///  "obo:ZFA_0000354": "gill",
///  "obo:ZFA_0000496": "compound organ",
///  "obo:ZFA_0000037": "anatomical structure"}
///
/// and the term tree for obo:ZFA_0000354:
///
/// term_tree = {
///   "obo:ZFA_0100000": {
///     "obo:ZFA_0000037": {
///       "obo:ZFA_0001512": {
///         "obo:ZFA_0001439": {
///           "obo:ZFA_0000272": {
///             "partOf obo:ZFA_0000354": "owl:Nothing"
///           }
///         }
///       },
///       "obo:ZFA_0000496": {                 <- is-a ancestor of obo:ZFA_0000354
///         "obo:ZFA_0000354": "owl:Nothing"   <- target entity obo:ZFA_0000354 (with child "owl:Nothing")
///       },
///       "obo:ZFA_0001094": {
///         "partOf obo:ZFA_0001439": {        <- part-of ancestor of obo:ZFA_0000354
///           "obo:ZFA_0000272": {
///             "partOf obo:ZFA_0000354": "owl:Nothing"
///           }
///         },
///         "partOf obo:ZFA_0000496": {
///           "obo:ZFA_0000354": "owl:Nothing"
///         }
///       }
///     }
///   }
/// }
///
/// then build_labelled_tree(term_tree, label_map) returns
///
/// {
///  "zebrafish anatomical entity": {
///    "anatomical structure": {
///      "whole organism": {
///        "partOf compound organ": {
///          "gill": "owl:Nothing"
///        },
///        "partOf anatomical system": {
///          "respiratory system": {
///            "partOf gill": "owl:Nothing"
///          }
///        }
///      },
///      "anatomical group": {
///        "anatomical system": {
///          "respiratory system": {
///            "partOf gill": "owl:Nothing"
///          }
///        }
///      },
///      "compound organ": {
///        "gill": "owl:Nothing"
///      }
///    }
///  }
///}
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
            json!("ERROR"); //TODO: implement error handling
        }
    }
    Value::Object(json_map)
}

/// Given an entity and an LDTab database,
/// return a term tree (encoded in JSON) for the entity that displays information about
/// the 'is-a' as well as 'part-of' relationships where IRIs/CURIEs are replaced with their labels.
///
/// # Examples
///
/// Calling the function with the entity obo:ZFA_0000354 and an LDTab database with
/// information about the zebrafish ontology yields the following term tree:
///
/// {
///  "zebrafish anatomical entity": {
///    "anatomical structure": {
///      "whole organism": {
///        "partOf compound organ": {
///          "gill": "owl:Nothing"
///        },
///        "partOf anatomical system": {
///          "respiratory system": {
///            "partOf gill": "owl:Nothing"
///          }
///        }
///      },
///      "anatomical group": {
///        "anatomical system": {
///          "respiratory system": {
///            "partOf gill": "owl:Nothing"
///          }
///        }
///      },
///      "compound organ": {
///        "gill": "owl:Nothing"
///      }
///    }
///  }
///}
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
    let label_hash_map = get_label_hash_map(&iris, table, pool).await.unwrap();

    //build term tree
    let tree = build_tree(&roots, &class_2_subclasses, &class_2_parts);

    //replace CURIEs with labels
    build_labelled_tree(&tree, &label_hash_map)
}

//#################################################################
//####################### Human readable Text format (Markdown) ###
//#################################################################

///Given a simple term tree, return a human-readable representation
///in Markdown.
/// Given an entity's term tree encoded as JSON,
/// return a term tree encoded in human-readable Markdown
///
/// # Examples
///
/// Given the following labelled term tree:
///
/// term_tree = {
///  "zebrafish anatomical entity": {
///    "anatomical structure": {
///      "whole organism": {
///        "partOf compound organ": {
///          "gill": "owl:Nothing"
///        },
///        "partOf anatomical system": {
///          "respiratory system": {
///            "partOf gill": "owl:Nothing"
///          }
///        }
///      },
///      "anatomical group": {
///        "anatomical system": {
///          "respiratory system": {
///            "partOf gill": "owl:Nothing"
///          }
///        }
///      },
///      "compound organ": {
///        "gill": "owl:Nothing"
///      }
///    }
///  }
///}
///
/// the function json_tree_2_text(term_tree,0) returns
///
///- zebrafish anatomical entity
///	- anatomical structure
///		- compound organ
///			- gill
///				- owl:Nothing
///		- whole organism
///			- partOf anatomical system
///				- respiratory system
///					- partOf gill
///						- owl:Nothing
///			- partOf compound organ
///				- gill
///					- owl:Nothing
///		- anatomical group
///			- anatomical system
///				- respiratory system
///					- partOf gill
///						- owl:Nothing
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

/// Given an entity and an LDTab database,
/// return a term tree (encoded in human-readable Markdown)
/// for the entity that displays information about
/// the 'is-a' as well as 'part-of' relationships
///
/// # Examples
///
/// Calling the function with the entity obo:ZFA_0000354 and an LDTab database with
/// information about the zebrafish ontology yields the following term tree:
///
///- zebrafish anatomical entity
///	- anatomical structure
///		- compound organ
///			- gill
///				- owl:Nothing
///		- whole organism
///			- partOf anatomical system
///				- respiratory system
///					- partOf gill
///						- owl:Nothing
///			- partOf compound organ
///				- gill
///					- owl:Nothing
///		- anatomical group
///			- anatomical system
///				- respiratory system
///					- partOf gill
///						- owl:Nothing
///
pub async fn get_text_view(entity: &str, table: &str, pool: &SqlitePool) -> String {
    //get term tree (encoded in JSON)
    let labelled_json_tree = get_labelled_json_tree_view(entity, table, pool).await;
    //transform JSON to Markdown
    json_tree_2_text(&labelled_json_tree, 0)
}
