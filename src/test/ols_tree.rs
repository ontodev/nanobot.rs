use reqwest;
use serde_json::{from_str, json, Map, Value};
use std::collections::{HashMap, HashSet};

/// Given an entity and an ontology acronym,
/// return information about the entity's is-a and part-of relationships as maintained by OLS.
///
/// # Examples
///
///  get_ols_ancestor_tree("obo:ZFA_0000354", "zfa") returns the following tree
///
/// [ {
///   "id" : "193025246_1",
///   "parent" : "193024704_1",
///   "iri" : "http://purl.obolibrary.org/obo/ZFA_0000496",
///   "text" : "compound organ",
///   "state" : {
///     "opened" : true
///   },
///   "children" : false,
///   "a_attr" : {
///     "iri" : "http://purl.obolibrary.org/obo/ZFA_0000496",
///     "ontology_name" : "zfa",
///     "title" : "http://purl.obolibrary.org/obo/ZFA_0000496",
///     "class" : "is_a"
///   },
///   "ontology_name" : "zfa"
/// }, {
///   "id" : "211024856_1",
///   "parent" : "193024704_1",
///   "iri" : "http://purl.obolibrary.org/obo/ZFA_0001094",
///   "text" : "whole organism",
///   "state" : {
///     "opened" : true
///   },
///   "children" : false,
///   "a_attr" : {
///     "iri" : "http://purl.obolibrary.org/obo/ZFA_0001094",
///     "ontology_name" : "zfa",
///     "title" : "http://purl.obolibrary.org/obo/ZFA_0001094",
///     "class" : "is_a"
///   },
///   "ontology_name" : "zfa"
/// } ... (end of excerpt) ]
pub fn get_ols_ancestor_tree(entity_id: &str, ontology: &str) -> Value {
    let mut url = String::from("");
    //let children = "https://www.ebi.ac.uk/ols/api/ontologies/zfa/terms/http%253A%252F%252Fpurl.obolibrary.org%252Fobo%252FZFA_0000354/jstree/children/211483179_3?lang=en";

    if ontology.eq("zfa") {
        url = format!("https://www.ebi.ac.uk/ols/api/ontologies/zfa/terms/http%253A%252F%252Fpurl.obolibrary.org%252Fobo%252FZFA_{}/jstree?viewMode=All&lang=en&siblings=false", entity_id);
    }

    if ontology.eq("uberon") {
        url = format!("https://www.ebi.ac.uk/ols/api/ontologies/uberon/terms/http%253A%252F%252Fpurl.obolibrary.org%252Fobo%252FUBERON_{}/jstree?viewMode=All&lang=en&siblings=false", entity_id);
    }

    //TODO: handle all these unrwaps
    let response = reqwest::blocking::get(&url).unwrap();
    let text = response.text().unwrap();
    let value = from_str::<Value>(&text).unwrap();

    value
}

/// Given an entity and a map from entities to information as maintained by OSL,
/// return a node for a simple JSON term tree (see get_json_tree_view for an example)
pub fn render(entity: &str, class_2_info: &HashMap<String, Value>) -> String {
    let obo = "http://purl.obolibrary.org/obo/";

    match class_2_info.get(entity) {
        Some(info) => {
            let s = info["iri"].as_str().unwrap();
            let s = s.replace(obo, "obo:");
            let hierarchy = info["a_attr"].as_object().unwrap();
            let hierarchy = hierarchy["class"].as_str().unwrap();
            if hierarchy.eq("part_of") {
                //encode part-of relation
                format!("partOf {}", s)
            } else {
                //encode is-a relation
                String::from(s)
            }
        }
        None => String::from(entity),
    }
}

/// Given a root entity, a map for sub-hierarchy information w.r.t. is-a and part-of,
/// and a map from entities to information as maintained by OSL,
/// return a simple JSON term tree (see get_json_tree_view for an example)
pub fn build_json_tree(
    start: &str,
    class_2_subclasses: &HashMap<String, HashSet<String>>,
    class_2_info: &HashMap<String, Value>,
) -> Value {
    let mut json_map = Map::new();

    match class_2_subclasses.get(start) {
        Some(subs) => {
            let mut inner_map = Map::new();
            for s in subs {
                let v = build_json_tree(s, class_2_subclasses, class_2_info);
                inner_map.extend(v.as_object().unwrap().clone());
            }
            json_map.insert(render(start, class_2_info), Value::Object(inner_map));
        }
        None => {
            let v = json!("owl:Nothing");
            json_map.insert(render(start, class_2_info), v);
        }
    };
    Value::Object(json_map)
}

/// Given an entity and an ontology acronym,
/// return a simple term tree for the entity built using OLS (version 3).
///
/// # Examples
///
///
///  get_json_tree_view("obo:ZFA_0000354", "zfa") returns the following tree
///
/// {
///   "obo:ZFA_0100000": {
///     "obo:ZFA_0000037": {
///       "obo:ZFA_0001094": {
///         "partOf obo:ZFA_0000496": {
///           "obo:ZFA_0000354": "owl:Nothing"
///         },
///         "partOf obo:ZFA_0001439": {
///           "obo:ZFA_0000272": {
///             "partOf obo:ZFA_0000354": "owl:Nothing"
///           }
///         }
///       },
///       "obo:ZFA_0000496": {
///         "obo:ZFA_0000354": "owl:Nothing"
///       },
///       "obo:ZFA_0001512": {
///         "obo:ZFA_0001439": {
///           "obo:ZFA_0000272": {
///             "partOf obo:ZFA_0000354": "owl:Nothing"
///           }
///         }
///       }
///     }
///   }
/// }
pub fn get_json_tree_view(entity: &str, ontology: &str) -> Value {
    let ancestor_tree = get_ols_ancestor_tree(entity, ontology);
    ols_2_nanobot_tree(&ancestor_tree)
}

/// Given a term tree in the format maintained by OLS (see get_ols_ancestor_tree for an example),
/// return the tree in simple JSON format (see get_json_tree_view for an example).
pub fn ols_2_nanobot_tree(ols_tree: &Value) -> Value {
    let mut id_to_info = HashMap::new();
    let mut class_2_children: HashMap<String, HashSet<String>> = HashMap::new();
    let array = ols_tree.as_array().unwrap();
    let mut roots: HashSet<String> = HashSet::new();

    for o in array {
        let id = o["id"].as_str().unwrap();
        let id_string = String::from(id);
        id_to_info.insert(id_string.clone(), o.clone());

        let parent = o["parent"].as_str().unwrap();
        if parent.eq("#") {
            roots.insert(id_string.clone());
        }

        match class_2_children.get_mut(parent) {
            Some(x) => {
                x.insert(id_string.clone());
            }
            None => {
                let mut subclasses = HashSet::new();
                subclasses.insert(id_string.clone());
                class_2_children.insert(String::from(parent), subclasses);
            }
        };
    }

    let mut json_map = Map::new();

    for r in roots {
        let tree = build_json_tree(&r, &class_2_children, &id_to_info);
        json_map.extend(tree.as_object().unwrap().clone());
    }

    Value::Object(json_map)
}
