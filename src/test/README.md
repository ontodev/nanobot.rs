# Testing Term Trees

We build term trees using LDTab databases. Since we want to provide similar functionality to well-known term trees visualisations used in OLS, we set up a validation pipeline that compares term trees built with LDtab databases (see module `src/tree_view.rs`) to term trees used in OLS.

In order to compare term trees implemented using different data formats, we define a *simple format* that other formats can be easily transformed into.

# Formats for Term Trees

We implement four different formats for term trees:

1. Simple JSON
    1. using CURIEs/IRIs, or
    2. labels (instead of CURIEs/IRIs)
2. Simple Markdown
3. Rich JSON
4. Rich Hiccup

The "simple" formats are used for testing and validation purposes. In particular, we compare term trees built with nanobot with term trees provided by OLS. The "rich" formats are used by default for term trees in nanobot. See below for examples using the term [gill](https://www.ebi.ac.uk/ols/ontologies/zfa/terms?iri=http%3A%2F%2Fpurl.obolibrary.org%2Fobo%2FZFA_0000354).

Note, that the internal data structures for all formats are the same (in particular `get_hierarchy_maps` is imported in the module `tree_validation.rs` from `part_of_term_trees.rs`) and the generated term trees are based on the same underlying algorithm (the validation could also be implemented using the "rich" format. One would only need a transformation for OLS trees into the "rich" format rather than the "simple" one).

## Simple Format (JSON & Markdown)

Formats 1 & 2 follow the same ('simple') abstract structure: a map of entities to their respective `subclasses` and `part-of` relations. An entity is encoded as a string. A `subclass` or `part-of` relationship is encoded via a nested map, i.e., `"zebrafish anatomical entity": { "anatomical structure": {...}  }` means that `anatomical structure` is a `subclass` of `zebrafish anatomical entity`. In the case of a `part-or` relationship, an entity's key includes the prefix `partOf`, e.g., `"whole organism": { "partOf compound organ": {  ... } }`. The entire tree for gill in OLS looks like this:

<details>
  <summary>
    Simple JSON Example
  </summary>

```
{
  "zebrafish anatomical entity": {
    "anatomical structure": {
      "whole organism": {
        "partOf compound organ": {
          "gill": "owl:Nothing"
        },
        "partOf anatomical system": {
          "respiratory system": {
            "partOf gill": "owl:Nothing"
          }
        }
      },
      "compound organ": {
        "gill": "owl:Nothing"
      },
      "anatomical group": {
        "anatomical system": {
          "respiratory system": {
            "partOf gill": "owl:Nothing"
          }
        }
      }
    }
  }
}
```
</details>

Note that "owl:Nothing" is used as a string in a place where a value is expected. To keep the format consistent, this should also be a map. In any case, this format is used to validate the `subclass` and `part-of` relations extracted from an LDTab database with OLS. The test pipeline looks like follows: 

1. Given a term, e.g., gill,
2. get term tree information from OLS in [JSON](https://www.ebi.ac.uk/ols/api/ontologies/zfa/terms/http%253A%252F%252Fpurl.obolibrary.org%252Fobo%252FZFA_0000354/jstree?viewMode=All&lang=en&siblings=false) 
3. transform the JSON from OLS (step 2.) into the simple format
4. construct a term tree using an LDTab database using the simple format
5. compare both term trees (represented in the same simple format encoded in JSON)

The Markdown equivalent of this simple format looks as follows:

<details>
  <summary>
    Markdown Example
  </summary>

- zebrafish anatomical entity
	- anatomical structure
		- anatomical group
			- anatomical system
				- respiratory system
					- partOf gill
						- owl:Nothing
		- compound organ
			- gill
				- owl:Nothing
		- whole organism
			- partOf compound organ
				- gill
					- owl:Nothing
			- partOf anatomical system
				- respiratory system
					- partOf gill
						- owl:Nothing
</details>

## Rich Format (JSON & Hiccup)

The "rich" format encodes an entity as a JSON object with information about its CURIE, label, and children. Children are entities that are related to an entity via `subclass` and `part-of` relationships. Each JSON object also contains information about the relationship to its parent. In addition to information that is encoded in the simple format, the rich format also includes a branch for children and grandchildren in the first occurrence of an entity (note that the tree representation is lexicographically ordered by labels).

<details>
  <summary>
    Rich Structure Example
  </summary>

 ```
[
  {
    "curie": "obo:ZFA_0100000",
    "label": "zebrafish anatomical entity",
    "property": "rdfs:subClassOf",
    "children": [
      {
        "curie": "obo:ZFA_0000037",
        "label": "anatomical structure",
        "property": "rdfs:subClassOf",
        "children": [
          {
            "curie": "obo:ZFA_0001512",
            "label": "anatomical group",
            "property": "rdfs:subClassOf",
            "children": [
              {
                "curie": "obo:ZFA_0001439",
                "label": "anatomical system",
                "property": "rdfs:subClassOf",
                "children": [
                  {
                    "curie": "obo:ZFA_0000272",
                    "label": "respiratory system",
                    "property": "rdfs:subClassOf",
                    "children": [
                      {
                        "curie": "obo:ZFA_0000354",
                        "label": "gill",
                        "property": "obo:BFO_0000050",
                        "children": [
                          {
                            "curie": "obo:ZFA_0000716",
                            "label": "afferent branchial artery",
                            "property": "obo:BFO_0000050",
                            "children": [
                              {
                                "curie": "obo:ZFA_0005012",
                                "label": "afferent filamental artery",
                                "property": "obo:BFO_0000050",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0005013",
                                "label": "concurrent branch afferent branchial artery",
                                "property": "obo:BFO_0000050",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0005014",
                                "label": "recurrent branch afferent branchial artery",
                                "property": "obo:BFO_0000050",
                                "children": []
                              }
                            ]
                          },
                          {
                            "curie": "obo:ZFA_0000319",
                            "label": "branchiostegal membrane",
                            "property": "obo:BFO_0000050",
                            "children": []
                          },
                          {
                            "curie": "obo:ZFA_0000202",
                            "label": "efferent branchial artery",
                            "property": "obo:BFO_0000050",
                            "children": [
                              {
                                "curie": "obo:ZFA_0005018",
                                "label": "efferent filamental artery",
                                "property": "obo:BFO_0000050",
                                "children": []
                              }
                            ]
                          },
                          {
                            "curie": "obo:ZFA_0000667",
                            "label": "gill filament",
                            "property": "obo:BFO_0000050",
                            "children": [
                              {
                                "curie": "obo:ZFA_0000666",
                                "label": "filamental artery",
                                "property": "obo:BFO_0000050",
                                "children": []
                              }
                            ]
                          },
                          {
                            "curie": "obo:ZFA_0005324",
                            "label": "gill ionocyte",
                            "property": "obo:BFO_0000050",
                            "children": []
                          },
                          {
                            "curie": "obo:ZFA_0000211",
                            "label": "gill lamella",
                            "property": "obo:BFO_0000050",
                            "children": [
                              {
                                "curie": "obo:ZFA_0005015",
                                "label": "afferent lamellar arteriole",
                                "property": "obo:BFO_0000050",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0005019",
                                "label": "efferent lamellar arteriole",
                                "property": "obo:BFO_0000050",
                                "children": []
                              }
                            ]
                          },
                          {
                            "curie": "obo:ZFA_0001613",
                            "label": "pharyngeal arch 3-7",
                            "property": "obo:BFO_0000050",
                            "children": [
                              {
                                "curie": "obo:ZFA_0000172",
                                "label": "branchial muscle",
                                "property": "obo:BFO_0000050",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0005390",
                                "label": "gill ray",
                                "property": "obo:BFO_0000050",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0001606",
                                "label": "pharyngeal arch 3",
                                "property": "rdfs:subClassOf",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0000095",
                                "label": "pharyngeal arch 3-7 skeleton",
                                "property": "obo:BFO_0000050",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0001607",
                                "label": "pharyngeal arch 4",
                                "property": "rdfs:subClassOf",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0001608",
                                "label": "pharyngeal arch 5",
                                "property": "rdfs:subClassOf",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0001609",
                                "label": "pharyngeal arch 6",
                                "property": "rdfs:subClassOf",
                                "children": []
                              },
                              {
                                "curie": "obo:ZFA_0001610",
                                "label": "pharyngeal arch 7",
                                "property": "rdfs:subClassOf",
                                "children": []
                              }
                            ]
                          }
                        ]
                      }
                    ]
                  }
                ]
              }
            ]
          },
          {
            "curie": "obo:ZFA_0000496",
            "label": "compound organ",
            "property": "rdfs:subClassOf",
            "children": [
              {
                "curie": "obo:ZFA_0000354",
                "label": "gill",
                "property": "rdfs:subClassOf",
                "children": []
              }
            ]
          },
          {
            "curie": "obo:ZFA_0001094",
            "label": "whole organism",
            "property": "rdfs:subClassOf",
            "children": [
              {
                "curie": "obo:ZFA_0001439",
                "label": "anatomical system",
                "property": "obo:BFO_0000050",
                "children": [
                  {
                    "curie": "obo:ZFA_0000272",
                    "label": "respiratory system",
                    "property": "rdfs:subClassOf",
                    "children": [
                      {
                        "curie": "obo:ZFA_0000354",
                        "label": "gill",
                        "property": "obo:BFO_0000050",
                        "children": []
                      }
                    ]
                  }
                ]
              },
              {
                "curie": "obo:ZFA_0000496",
                "label": "compound organ",
                "property": "obo:BFO_0000050",
                "children": [
                  {
                    "curie": "obo:ZFA_0000354",
                    "label": "gill",
                    "property": "rdfs:subClassOf",
                    "children": []
                  }
                ]
              }
            ]
          }
        ]
      }
    ]
  }
]
```
</details>

This JSON structure can be transformed into a JSON hiccup representation:

<details>
  <summary>
    Rich JSON Hiccup Example
  </summary>

```
["ul",["li","Ontology"],["li",["a",{"resource":"owl:Class"},"owl:Class"],["ul",["li",["a",{"resource":"obo:ZFA_0100000"},"zebrafish anatomical entity"],["ul",["li",["a",{"resource":"obo:ZFA_0000037","about":"obo:ZFA_0100000","rev":"rdfs:subClassOf"},"anatomical structure"],["ul",["li",["a",{"resource":"obo:ZFA_0001512","about":"obo:ZFA_0000037","rev":"rdfs:subClassOf"},"anatomical group"],["ul",["li",["a",{"resource":"obo:ZFA_0001439","about":"obo:ZFA_0001512","rev":"rdfs:subClassOf"},"anatomical system"],["ul",["li",["a",{"resource":"obo:ZFA_0000272","about":"obo:ZFA_0001439","rev":"rdfs:subClassOf"},"respiratory system"],["ul",["li",["a",{"resource":"obo:ZFA_0000354","about":"obo:ZFA_0000272","rev":"obo:BFO_0000050"},"gill"],["ul",{"id":"children"},["li",["a",{"resource":"obo:ZFA_0000716","about":"obo:ZFA_0000354","rev":"obo:BFO_0000050"},"afferent branchial artery"],["ul",{"id":"children"},["li",["a",{"resource":"obo:ZFA_0005012","about":"obo:ZFA_0000716","rev":"obo:BFO_0000050"},"afferent filamental artery"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0005013","about":"obo:ZFA_0000716","rev":"obo:BFO_0000050"},"concurrent branch afferent branchial artery"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0005014","about":"obo:ZFA_0000716","rev":"obo:BFO_0000050"},"recurrent branch afferent branchial artery"],["ul",{"id":"children"}]]]],["li",["a",{"resource":"obo:ZFA_0000319","about":"obo:ZFA_0000354","rev":"obo:BFO_0000050"},"branchiostegal membrane"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0000202","about":"obo:ZFA_0000354","rev":"obo:BFO_0000050"},"efferent branchial artery"],["ul",{"id":"children"},["li",["a",{"resource":"obo:ZFA_0005018","about":"obo:ZFA_0000202","rev":"obo:BFO_0000050"},"efferent filamental artery"],["ul",{"id":"children"}]]]],["li",["a",{"resource":"obo:ZFA_0000667","about":"obo:ZFA_0000354","rev":"obo:BFO_0000050"},"gill filament"],["ul",{"id":"children"},["li",["a",{"resource":"obo:ZFA_0000666","about":"obo:ZFA_0000667","rev":"obo:BFO_0000050"},"filamental artery"],["ul",{"id":"children"}]]]],["li",["a",{"resource":"obo:ZFA_0005324","about":"obo:ZFA_0000354","rev":"obo:BFO_0000050"},"gill ionocyte"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0000211","about":"obo:ZFA_0000354","rev":"obo:BFO_0000050"},"gill lamella"],["ul",{"id":"children"},["li",["a",{"resource":"obo:ZFA_0005015","about":"obo:ZFA_0000211","rev":"obo:BFO_0000050"},"afferent lamellar arteriole"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0005019","about":"obo:ZFA_0000211","rev":"obo:BFO_0000050"},"efferent lamellar arteriole"],["ul",{"id":"children"}]]]],["li",["a",{"resource":"obo:ZFA_0001613","about":"obo:ZFA_0000354","rev":"obo:BFO_0000050"},"pharyngeal arch 3-7"],["ul",{"id":"children"},["li",["a",{"resource":"obo:ZFA_0000172","about":"obo:ZFA_0001613","rev":"obo:BFO_0000050"},"branchial muscle"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0005390","about":"obo:ZFA_0001613","rev":"obo:BFO_0000050"},"gill ray"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0001606","about":"obo:ZFA_0001613","rev":"rdfs:subClassOf"},"pharyngeal arch 3"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0000095","about":"obo:ZFA_0001613","rev":"obo:BFO_0000050"},"pharyngeal arch 3-7 skeleton"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0001607","about":"obo:ZFA_0001613","rev":"rdfs:subClassOf"},"pharyngeal arch 4"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0001608","about":"obo:ZFA_0001613","rev":"rdfs:subClassOf"},"pharyngeal arch 5"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0001609","about":"obo:ZFA_0001613","rev":"rdfs:subClassOf"},"pharyngeal arch 6"],["ul",{"id":"children"}]],["li",["a",{"resource":"obo:ZFA_0001610","about":"obo:ZFA_0001613","rev":"rdfs:subClassOf"},"pharyngeal arch 7"],["ul",{"id":"children"}]]]]]]]]]]]],["li",["a",{"resource":"obo:ZFA_0000496","about":"obo:ZFA_0000037","rev":"rdfs:subClassOf"},"compound organ"],["ul",["li",["a",{"resource":"obo:ZFA_0000354","about":"obo:ZFA_0000496","rev":"rdfs:subClassOf"},"gill"],["ul",{"id":"children"}]]]],["li",["a",{"resource":"obo:ZFA_0001094","about":"obo:ZFA_0000037","rev":"rdfs:subClassOf"},"whole organism"],["ul",["li",["a",{"resource":"obo:ZFA_0001439","about":"obo:ZFA_0001094","rev":"obo:BFO_0000050"},"anatomical system"],["ul",["li",["a",{"resource":"obo:ZFA_0000272","about":"obo:ZFA_0001439","rev":"rdfs:subClassOf"},"respiratory system"],["ul",["li",["a",{"resource":"obo:ZFA_0000354","about":"obo:ZFA_0000272","rev":"obo:BFO_0000050"},"gill"],["ul",{"id":"children"}]]]]]],["li",["a",{"resource":"obo:ZFA_0000496","about":"obo:ZFA_0001094","rev":"obo:BFO_0000050"},"compound organ"],["ul",["li",["a",{"resource":"obo:ZFA_0000354","about":"obo:ZFA_0000496","rev":"rdfs:subClassOf"},"gill"],["ul",{"id":"children"}]]]]]]]]]]]]]
```
</details>

# Code Organisation

The code for testing and validating term trees is split across four modules:

1. **ols_tree.rs**: module for building term trees (encoded in the simple JSON format) using information from OLS (version 3)
2. **tree_validation.rs**: module for building term trees (encoded in the simple JSON format and human-readable Markdown) using an LDTab database
3. **term_tree_test_data_generation.rs**: module for generating (minimal) excerpts of an LDTab database. Such an excerpt contains all the relevant information for building a term tree for a given entity as the original database.
4. **part_of_term_tree.rs**: module for generating term trees using the *part-of* relation.

The test setup is rather involved. So, here is a summary.

## Term Tree Construction

The module `part_of_term_trees.rs` provides an implementation for term trees based on LDTab databases. The term trees are restricted to information about a term's **is-a** and **part-of** information.

## Term Tree Validation

Term trees based on LDTab databases are supposed to be comparable (if not identical) to term trees as displayed by OLS. So, we use the function `get_json_tree_view` in the module `ols_tree.rs` to fetch term trees from OLS which are transformed into the *simple* JSON format (see above). The examples in `src/resources/test_data/ols_term_trees` are created using this function. These example test trees can then be compared to term trees as implemented by `tree_validation.rs`. An example test is [here](https://github.com/ontodev/nanobot.rs/blob/term-tree-json/tests/term_tree.rs#L146). So, to summarise, the module `tree_validation.rs` builds term trees based (using the simple JSON format) on LDTab databses, which get compared to trees as fetched from OLS.
 
## Test Data

Setting up test data for large term trees that error-prone and a lot of work. So, we automate this process for trees that we have validated using OLS (see above).  The module `term_tree_test_data_generation.rs` is essentially a copy of `part_of_tree_view.rs` with the only modification that it collects the results of any SQL queries which in turn are written to a TSV file (see `output` parameter of `get_rich_json_tree_view`). This is how the example test data in `src/resources/test_data/uberon` and `src/resources/test_data/zfa` is generated. These TSV files can be used to set up small LDTab databases that contain all the required information for generating the term tree of a given entity.  Since the expected output term trees are potentially large JSON objects, we maintain these expected values as text files in `src/resources/test_data/ldtab_term_trees`.


