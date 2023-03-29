use nanobot::ldtab::{
    get_label_map, get_predicate_map_hiccup, get_prefix_map, get_property_map, get_subject_map,
};
use nanobot::sql::{parse, select_to_sql, select_to_url, Direction, Operator, Select};
use serde_json::json;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::collections::HashSet;

#[tokio::test]
async fn test_get_prefix_map() {
    let connection = "src/resources/test_data/zfa_excerpt.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let mut curies = HashSet::new();
    curies.insert(String::from("obo:ZFA_0000354"));
    curies.insert(String::from("rdfs:label"));
    let prefix_map = get_prefix_map(&curies, &pool).await.unwrap();
    let expected_prefix_map = json!({"@prefixes":{"obo":"http://purl.obolibrary.org/obo/","rdfs":"http://www.w3.org/2000/01/rdf-schema#"}});
    assert_eq!(prefix_map, expected_prefix_map);
}

#[tokio::test]
async fn test_get_label_map() {
    let connection = "src/resources/test_data/zfa_excerpt.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let table = "statement";

    let mut curies = HashSet::new();
    curies.insert(String::from("obo:ZFA_0000354"));
    let label_map = get_label_map(&curies, &table, &pool).await.unwrap();
    let expected_label_map = json!({"@labels":{"obo:ZFA_0000354":"gill"}});
    assert_eq!(label_map, expected_label_map);
}

#[tokio::test]
async fn test_get_property_map() {
    let connection = "src/resources/test_data/zfa_excerpt.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let subject = "obo:ZFA_0000354";
    let table = "statement";

    let property_map = get_property_map(&subject, &table, &pool).await.unwrap();
    let expected_property_map = json!(
    {
      "obo:IAO_0000115": [
        {
          "object": "Compound organ that consists of gill filaments, gill lamellae, gill rakers and pharyngeal arches 3-7. The gills are responsible for primary gas exchange between the blood and the surrounding water.",
          "datatype": "xsd:string",
          "annotation": {
            "<http://www.geneontology.org/formats/oboInOwl#hasDbXref>": [
              {
                "datatype": "xsd:string",
                "meta": "owl:Axiom",
                "object": "http:http://www.briancoad.com/Dictionary/DicPics/gill.htm"
              }
            ]
          }
        }
      ],
      "<http://www.geneontology.org/formats/oboInOwl#hasDbXref>": [
        {
          "object": "TAO:0000354",
          "datatype": "xsd:string"
        }
      ],
      "rdfs:subClassOf": [
        {
          "object": {
            "owl:onProperty": [
              {
                "datatype": "_IRI",
                "object": "obo:BFO_0000050"
              }
            ],
            "owl:someValuesFrom": [
              {
                "datatype": "_IRI",
                "object": "obo:ZFA_0000272"
              }
            ],
            "rdf:type": [
              {
                "datatype": "_IRI",
                "object": "owl:Restriction"
              }
            ]
          },
          "datatype": "_JSON"
        },
        {
          "object": {
            "owl:onProperty": [
              {
                "datatype": "_IRI",
                "object": "obo:RO_0002497"
              }
            ],
            "owl:someValuesFrom": [
              {
                "datatype": "_IRI",
                "object": "obo:ZFS_0000044"
              }
            ],
            "rdf:type": [
              {
                "datatype": "_IRI",
                "object": "owl:Restriction"
              }
            ]
          },
          "datatype": "_JSON"
        },
        {
          "object": {
            "owl:onProperty": [
              {
                "datatype": "_IRI",
                "object": "obo:RO_0002202"
              }
            ],
            "owl:someValuesFrom": [
              {
                "datatype": "_IRI",
                "object": "obo:ZFA_0001107"
              }
            ],
            "rdf:type": [
              {
                "datatype": "_IRI",
                "object": "owl:Restriction"
              }
            ]
          },
          "datatype": "_JSON"
        },
        {
          "object": "obo:ZFA_0000496",
          "datatype": "_IRI"
        },
        {
          "object": {
            "owl:onProperty": [
              {
                "datatype": "_IRI",
                "object": "obo:RO_0002496"
              }
            ],
            "owl:someValuesFrom": [
              {
                "datatype": "_IRI",
                "object": "obo:ZFS_0000000"
              }
            ],
            "rdf:type": [
              {
                "datatype": "_IRI",
                "object": "owl:Restriction"
              }
            ]
          },
          "datatype": "_JSON"
        }
      ],
      "<http://www.geneontology.org/formats/oboInOwl#id>": [
        {
          "object": "ZFA:0000354",
          "datatype": "xsd:string"
        }
      ],
      "rdf:type": [
        {
          "object": "owl:Class",
          "datatype": "_IRI"
        }
      ],
      "rdfs:label": [
        {
          "object": "gill",
          "datatype": "xsd:string"
        }
      ],
      "<http://www.geneontology.org/formats/oboInOwl#hasOBONamespace>": [
        {
          "object": "zebrafish_anatomy",
          "datatype": "xsd:string"
        }
      ],
      "<http://www.geneontology.org/formats/oboInOwl#hasExactSynonym>": [
        {
          "object": "gills",
          "datatype": "xsd:string",
          "annotation": {
            "<http://www.geneontology.org/formats/oboInOwl#hasSynonymType>": [
              {
                "datatype": "_IRI",
                "meta": "owl:Axiom",
                "object": "obo:zfa#PLURAL"
              }
            ]
          }
        }
      ]
    });
    assert_eq!(property_map, expected_property_map);
}

#[tokio::test]
async fn test_get_predicate_map_hiccup() {
    let connection = "src/resources/test_data/zfa_excerpt.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let subject = "obo:ZFA_0000354";
    let table = "statement";

    let starting_order = vec![String::from("rdfs:label"), String::from("obo:IAO_0000115")];
    let ending_order = vec![String::from("rdfs:comment")];

    let hiccup = get_predicate_map_hiccup(&subject, &table, &pool, &starting_order, &ending_order)
        .await
        .unwrap();
    //oboInOwl prefix is not loaded in zfa_excerpt
    let expected = json!([
      "ul",
      {
        "id": "annotations",
        "style": "margin-left: -1rem;"
      },
      [
        "li",
        [
          "a",
          {
            "resource": "rdfs:label"
          },
          "rdfs:label"
        ],
        [
          "ul",
          [
            "li",
            "gill",
            [
              "sup",
              {
                "class": "text-black-50"
              },
              [
                "a",
                {
                  "resource": "xsd:string"
                },
                "xsd:string"
              ]
            ]
          ]
        ]
      ],
      [
        "li",
        [
          "a",
          {
            "resource": "obo:IAO_0000115"
          },
          "obo:IAO_0000115"
        ],
        [
          "ul",
          [
            "li",
            "Compound organ that consists of gill filaments, gill lamellae, gill rakers and pharyngeal arches 3-7. The gills are responsible for primary gas exchange between the blood and the surrounding water.",
            [
              "sup",
              {
                "class": "text-black-50"
              },
              [
                "a",
                {
                  "resource": "xsd:string"
                },
                "xsd:string"
              ]
            ]
          ]
        ]
      ],
      [
        "li",
        [
          "a",
          {
            "resource": "<http://www.geneontology.org/formats/oboInOwl#hasDbXref>"
          },
          "<http://www.geneontology.org/formats/oboInOwl#hasDbXref>"
        ],
        [
          "ul",
          [
            "li",
            "TAO:0000354",
            [
              "sup",
              {
                "class": "text-black-50"
              },
              [
                "a",
                {
                  "resource": "xsd:string"
                },
                "xsd:string"
              ]
            ]
          ]
        ]
      ],
      [
        "li",
        [
          "a",
          {
            "resource": "<http://www.geneontology.org/formats/oboInOwl#hasExactSynonym>"
          },
          "<http://www.geneontology.org/formats/oboInOwl#hasExactSynonym>"
        ],
        [
          "ul",
          [
            "li",
            "gills",
            [
              "sup",
              {
                "class": "text-black-50"
              },
              [
                "a",
                {
                  "resource": "xsd:string"
                },
                "xsd:string"
              ]
            ]
          ]
        ]
      ],
      [
        "li",
        [
          "a",
          {
            "resource": "<http://www.geneontology.org/formats/oboInOwl#hasOBONamespace>"
          },
          "<http://www.geneontology.org/formats/oboInOwl#hasOBONamespace>"
        ],
        [
          "ul",
          [
            "li",
            "zebrafish_anatomy",
            [
              "sup",
              {
                "class": "text-black-50"
              },
              [
                "a",
                {
                  "resource": "xsd:string"
                },
                "xsd:string"
              ]
            ]
          ]
        ]
      ],
      [
        "li",
        [
          "a",
          {
            "resource": "<http://www.geneontology.org/formats/oboInOwl#id>"
          },
          "<http://www.geneontology.org/formats/oboInOwl#id>"
        ],
        [
          "ul",
          [
            "li",
            "ZFA:0000354",
            [
              "sup",
              {
                "class": "text-black-50"
              },
              [
                "a",
                {
                  "resource": "xsd:string"
                },
                "xsd:string"
              ]
            ]
          ]
        ]
      ],
      [
        "li",
        [
          "a",
          {
            "resource": "rdf:type"
          },
          "rdf:type"
        ],
        [
          "ul",
          [
            "li",
            [
              "a",
              {
                "property": "rdf:type",
                "resource": "owl:Class"
              },
              "owl:Class"
            ]
          ]
        ]
      ],
      [
        "li",
        [
          "a",
          {
            "resource": "rdfs:subClassOf"
          },
          "rdfs:subClassOf"
        ],
        [
          "ul",
          [
            "li",
            [
              "span",
              {
                "property": "rdfs:subClassOf",
                "typeof": "owl:Restriction"
              },
              [
                "a",
                {
                  "property": "owl:onProperty",
                  "resource": "obo:BFO_0000050"
                },
                "obo:BFO_0000050"
              ],
              " some ",
              [
                "a",
                {
                  "property": "owl:someValuesFrom",
                  "resource": "obo:ZFA_0000272"
                },
                "respiratory system"
              ]
            ]
          ],
          [
            "li",
            [
              "span",
              {
                "property": "rdfs:subClassOf",
                "typeof": "owl:Restriction"
              },
              [
                "a",
                {
                  "property": "owl:onProperty",
                  "resource": "obo:RO_0002497"
                },
                "obo:RO_0002497"
              ],
              " some ",
              [
                "a",
                {
                  "property": "owl:someValuesFrom",
                  "resource": "obo:ZFS_0000044"
                },
                "adult"
              ]
            ]
          ],
          [
            "li",
            [
              "span",
              {
                "property": "rdfs:subClassOf",
                "typeof": "owl:Restriction"
              },
              [
                "a",
                {
                  "property": "owl:onProperty",
                  "resource": "obo:RO_0002202"
                },
                "obo:RO_0002202"
              ],
              " some ",
              [
                "a",
                {
                  "property": "owl:someValuesFrom",
                  "resource": "obo:ZFA_0001107"
                },
                "internal gill bud"
              ]
            ]
          ],
          [
            "li",
            [
              "a",
              {
                "property": "rdfs:subClassOf",
                "resource": "obo:ZFA_0000496"
              },
              "obo:ZFA_0000496"
            ]
          ],
          [
            "li",
            [
              "span",
              {
                "property": "rdfs:subClassOf",
                "typeof": "owl:Restriction"
              },
              [
                "a",
                {
                  "property": "owl:onProperty",
                  "resource": "obo:RO_0002496"
                },
                "obo:RO_0002496"
              ],
              " some ",
              [
                "a",
                {
                  "property": "owl:someValuesFrom",
                  "resource": "obo:ZFS_0000000"
                },
                "Unknown"
              ]
            ]
          ]
        ]
      ]
    ]
    );
    assert_eq!(hiccup, expected);
}

#[tokio::test]
async fn test_get_subject_map() {
    let connection = "src/resources/test_data/zfa_excerpt.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let subject = "obo:ZFA_0000354";
    let table = "statement";
    let subject_map = get_subject_map(&subject, &table, &pool).await.unwrap();
    let expected = json!({
      "obo:ZFA_0000354": {
        "obo:IAO_0000115": [
          {
            "object": "Compound organ that consists of gill filaments, gill lamellae, gill rakers and pharyngeal arches 3-7. The gills are responsible for primary gas exchange between the blood and the surrounding water.",
            "datatype": "xsd:string",
            "annotation": {
              "<http://www.geneontology.org/formats/oboInOwl#hasDbXref>": [
                {
                  "datatype": "xsd:string",
                  "meta": "owl:Axiom",
                  "object": "http:http://www.briancoad.com/Dictionary/DicPics/gill.htm"
                }
              ]
            }
          }
        ],
        "<http://www.geneontology.org/formats/oboInOwl#hasDbXref>": [
          {
            "object": "TAO:0000354",
            "datatype": "xsd:string"
          }
        ],
        "rdfs:subClassOf": [
          {
            "object": {
              "owl:onProperty": [
                {
                  "datatype": "_IRI",
                  "object": "obo:BFO_0000050"
                }
              ],
              "owl:someValuesFrom": [
                {
                  "datatype": "_IRI",
                  "object": "obo:ZFA_0000272"
                }
              ],
              "rdf:type": [
                {
                  "datatype": "_IRI",
                  "object": "owl:Restriction"
                }
              ]
            },
            "datatype": "_JSON"
          },
          {
            "object": {
              "owl:onProperty": [
                {
                  "datatype": "_IRI",
                  "object": "obo:RO_0002497"
                }
              ],
              "owl:someValuesFrom": [
                {
                  "datatype": "_IRI",
                  "object": "obo:ZFS_0000044"
                }
              ],
              "rdf:type": [
                {
                  "datatype": "_IRI",
                  "object": "owl:Restriction"
                }
              ]
            },
            "datatype": "_JSON"
          },
          {
            "object": {
              "owl:onProperty": [
                {
                  "datatype": "_IRI",
                  "object": "obo:RO_0002202"
                }
              ],
              "owl:someValuesFrom": [
                {
                  "datatype": "_IRI",
                  "object": "obo:ZFA_0001107"
                }
              ],
              "rdf:type": [
                {
                  "datatype": "_IRI",
                  "object": "owl:Restriction"
                }
              ]
            },
            "datatype": "_JSON"
          },
          {
            "object": "obo:ZFA_0000496",
            "datatype": "_IRI"
          },
          {
            "object": {
              "owl:onProperty": [
                {
                  "datatype": "_IRI",
                  "object": "obo:RO_0002496"
                }
              ],
              "owl:someValuesFrom": [
                {
                  "datatype": "_IRI",
                  "object": "obo:ZFS_0000000"
                }
              ],
              "rdf:type": [
                {
                  "datatype": "_IRI",
                  "object": "owl:Restriction"
                }
              ]
            },
            "datatype": "_JSON"
          }
        ],
        "<http://www.geneontology.org/formats/oboInOwl#id>": [
          {
            "object": "ZFA:0000354",
            "datatype": "xsd:string"
          }
        ],
        "rdf:type": [
          {
            "object": "owl:Class",
            "datatype": "_IRI"
          }
        ],
        "rdfs:label": [
          {
            "object": "gill",
            "datatype": "xsd:string"
          }
        ],
        "<http://www.geneontology.org/formats/oboInOwl#hasOBONamespace>": [
          {
            "object": "zebrafish_anatomy",
            "datatype": "xsd:string"
          }
        ],
        "<http://www.geneontology.org/formats/oboInOwl#hasExactSynonym>": [
          {
            "object": "gills",
            "datatype": "xsd:string",
            "annotation": {
              "<http://www.geneontology.org/formats/oboInOwl#hasSynonymType>": [
                {
                  "datatype": "_IRI",
                  "meta": "owl:Axiom",
                  "object": "obo:zfa#PLURAL"
                }
              ]
            }
          }
        ]
      },
      "@labels": {
        "obo:ZFA_0001107": "internal gill bud",
        "obo:ZFA_0000272": "respiratory system",
        "obo:ZFA_0000354": "gill",
        "obo:ZFS_0000000": "Unknown",
        "obo:ZFS_0000044": "adult"
      },
      "@prefixes": {
        "owl": "http://www.w3.org/2002/07/owl#",
        "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
        "obo": "http://purl.obolibrary.org/obo/",
        "rdfs": "http://www.w3.org/2000/01/rdf-schema#"
      }
    }
    );
    assert_eq!(subject_map, expected);
}
