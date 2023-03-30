use nanobot::tree_view::{get_rich_json_tree_view, build_html_hiccup, get_html_top_hierarchy};
use serde_json::{from_str, json, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::fs;

#[tokio::test]
async fn test_get_rich_json_tree_view() {
    let connection = "src/resources/test_data/zfa_excerpt_for_term_trees.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let table = "statement";
    let subject = "obo:ZFA_0000354";

    //boolean flag is for preferred_roots
    let rich_hierarchy = get_rich_json_tree_view(subject, false, table, &pool)
        .await
        .unwrap();

    let expected_string = r#"
[{
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
}]"#;

    let expected = from_str::<Value>(expected_string);

    assert_eq!(rich_hierarchy, expected.unwrap());
}

#[tokio::test]
async fn test_build_html_hiccup() {
    let connection = "src/resources/test_data/zfa_excerpt_for_term_trees.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let table = "statement";
    let subject = "obo:ZFA_0000354";

    //boolean is for preferred root terms
    let hiccup = build_html_hiccup(&subject, false, table, &pool)
        .await
        .unwrap();

    let expected_string = r#"
[
  "ul",
  [
    "li",
    "Ontology"
  ],
  [
    "li",
    [
      "a",
      {
        "resource": "owl:Class"
      },
      "owl:Class"
    ],
    [
      "ul",
      [
        "li",
        [
          "a",
          {
            "resource": "obo:ZFA_0100000"
          },
          "zebrafish anatomical entity"
        ],
        [
          "ul",
          [
            "li",
            [
              "a",
              {
                "resource": "obo:ZFA_0000037",
                "about": "obo:ZFA_0100000",
                "rev": "rdfs:subClassOf"
              },
              "anatomical structure"
            ],
            [
              "ul",
              [
                "li",
                [
                  "a",
                  {
                    "resource": "obo:ZFA_0001512",
                    "about": "obo:ZFA_0000037",
                    "rev": "rdfs:subClassOf"
                  },
                  "anatomical group"
                ],
                [
                  "ul",
                  [
                    "li",
                    [
                      "a",
                      {
                        "resource": "obo:ZFA_0001439",
                        "about": "obo:ZFA_0001512",
                        "rev": "rdfs:subClassOf"
                      },
                      "anatomical system"
                    ],
                    [
                      "ul",
                      [
                        "li",
                        [
                          "a",
                          {
                            "resource": "obo:ZFA_0000272",
                            "about": "obo:ZFA_0001439",
                            "rev": "rdfs:subClassOf"
                          },
                          "respiratory system"
                        ],
                        [
                          "ul",
                          [
                            "li",
                            [
                              "a",
                              {
                                "resource": "obo:ZFA_0000354",
                                "about": "obo:ZFA_0000272",
                                "rev": "obo:BFO_0000050"
                              },
                              "gill"
                            ],
                            [
                              "ul",
                              {
                                "id": "children"
                              },
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000716",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "afferent branchial artery"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005012",
                                        "about": "obo:ZFA_0000716",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "afferent filamental artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005013",
                                        "about": "obo:ZFA_0000716",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "concurrent branch afferent branchial artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005014",
                                        "about": "obo:ZFA_0000716",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "recurrent branch afferent branchial artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000319",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "branchiostegal membrane"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  }
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000202",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "efferent branchial artery"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005018",
                                        "about": "obo:ZFA_0000202",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "efferent filamental artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000667",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "gill filament"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0000666",
                                        "about": "obo:ZFA_0000667",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "filamental artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0005324",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "gill ionocyte"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  }
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000211",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "gill lamella"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005015",
                                        "about": "obo:ZFA_0000211",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "afferent lamellar arteriole"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005019",
                                        "about": "obo:ZFA_0000211",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "efferent lamellar arteriole"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0001613",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "pharyngeal arch 3-7"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0000172",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "branchial muscle"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005390",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "gill ray"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001606",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 3"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0000095",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "pharyngeal arch 3-7 skeleton"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001607",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 4"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001608",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 5"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001609",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 6"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001610",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 7"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ]
                            ]
                          ]
                        ]
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
                    "resource": "obo:ZFA_0000496",
                    "about": "obo:ZFA_0000037",
                    "rev": "rdfs:subClassOf"
                  },
                  "compound organ"
                ],
                [
                  "ul",
                  [
                    "li",
                    [
                      "a",
                      {
                        "resource": "obo:ZFA_0000354",
                        "about": "obo:ZFA_0000496",
                        "rev": "rdfs:subClassOf"
                      },
                      "gill"
                    ],
                    [
                      "ul",
                      {
                        "id": "children"
                      }
                    ]
                  ]
                ]
              ],
              [
                "li",
                [
                  "a",
                  {
                    "resource": "obo:ZFA_0001094",
                    "about": "obo:ZFA_0000037",
                    "rev": "rdfs:subClassOf"
                  },
                  "whole organism"
                ],
                [
                  "ul",
                  [
                    "li",
                    [
                      "a",
                      {
                        "resource": "obo:ZFA_0001439",
                        "about": "obo:ZFA_0001094",
                        "rev": "obo:BFO_0000050"
                      },
                      "anatomical system"
                    ],
                    [
                      "ul",
                      [
                        "li",
                        [
                          "a",
                          {
                            "resource": "obo:ZFA_0000272",
                            "about": "obo:ZFA_0001439",
                            "rev": "rdfs:subClassOf"
                          },
                          "respiratory system"
                        ],
                        [
                          "ul",
                          [
                            "li",
                            [
                              "a",
                              {
                                "resource": "obo:ZFA_0000354",
                                "about": "obo:ZFA_0000272",
                                "rev": "obo:BFO_0000050"
                              },
                              "gill"
                            ],
                            [
                              "ul",
                              {
                                "id": "children"
                              }
                            ]
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
                        "resource": "obo:ZFA_0000496",
                        "about": "obo:ZFA_0001094",
                        "rev": "obo:BFO_0000050"
                      },
                      "compound organ"
                    ],
                    [
                      "ul",
                      [
                        "li",
                        [
                          "a",
                          {
                            "resource": "obo:ZFA_0000354",
                            "about": "obo:ZFA_0000496",
                            "rev": "rdfs:subClassOf"
                          },
                          "gill"
                        ],
                        [
                          "ul",
                          {
                            "id": "children"
                          }
                        ]
                      ]
                    ]
                  ]
                ]
              ]
            ]
          ]
        ]
      ]
    ]
  ]
]"#;

    let expected = from_str::<Value>(expected_string);

    assert_eq!(hiccup, expected.unwrap());
}

#[tokio::test]
async fn test_build_html_hiccup_preferred() {
    let connection = "src/resources/test_data/zfa_excerpt_for_term_trees.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let table = "statement";
    let subject = "obo:ZFA_0000354";

    //boolean is for preferred root terms
    let hiccup = build_html_hiccup(&subject, true, table, &pool)
        .await
        .unwrap();

    let expected_string = r#"
[
  "ul",
  [
    "li",
    "Ontology"
  ],
  [
    "li",
    [
      "a",
      {
        "resource": "owl:Class"
      },
      "owl:Class"
    ],
    [
      "ul",
      [
        "li",
        [
          "a",
          {
            "resource": "obo:ZFA_0100000"
          },
          "zebrafish anatomical entity"
        ],
        [
          "ul",
          [
            "li",
            [
              "a",
              {
                "resource": "obo:ZFA_0000037",
                "about": "obo:ZFA_0100000",
                "rev": "rdfs:subClassOf"
              },
              "anatomical structure"
            ],
            [
              "ul",
              [
                "li",
                [
                  "a",
                  {
                    "resource": "obo:ZFA_0001512",
                    "about": "obo:ZFA_0000037",
                    "rev": "rdfs:subClassOf"
                  },
                  "anatomical group"
                ],
                [
                  "ul",
                  [
                    "li",
                    [
                      "a",
                      {
                        "resource": "obo:ZFA_0001439",
                        "about": "obo:ZFA_0001512",
                        "rev": "rdfs:subClassOf"
                      },
                      "anatomical system"
                    ],
                    [
                      "ul",
                      [
                        "li",
                        [
                          "a",
                          {
                            "resource": "obo:ZFA_0000272",
                            "about": "obo:ZFA_0001439",
                            "rev": "rdfs:subClassOf"
                          },
                          "respiratory system"
                        ],
                        [
                          "ul",
                          [
                            "li",
                            [
                              "a",
                              {
                                "resource": "obo:ZFA_0000354",
                                "about": "obo:ZFA_0000272",
                                "rev": "obo:BFO_0000050"
                              },
                              "gill"
                            ],
                            [
                              "ul",
                              {
                                "id": "children"
                              },
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000716",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "afferent branchial artery"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005012",
                                        "about": "obo:ZFA_0000716",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "afferent filamental artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005013",
                                        "about": "obo:ZFA_0000716",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "concurrent branch afferent branchial artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005014",
                                        "about": "obo:ZFA_0000716",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "recurrent branch afferent branchial artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000319",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "branchiostegal membrane"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  }
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000202",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "efferent branchial artery"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005018",
                                        "about": "obo:ZFA_0000202",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "efferent filamental artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000667",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "gill filament"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0000666",
                                        "about": "obo:ZFA_0000667",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "filamental artery"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0005324",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "gill ionocyte"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  }
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0000211",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "gill lamella"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005015",
                                        "about": "obo:ZFA_0000211",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "afferent lamellar arteriole"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005019",
                                        "about": "obo:ZFA_0000211",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "efferent lamellar arteriole"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ],
                              [
                                "li",
                                [
                                  "a",
                                  {
                                    "resource": "obo:ZFA_0001613",
                                    "about": "obo:ZFA_0000354",
                                    "rev": "obo:BFO_0000050"
                                  },
                                  "pharyngeal arch 3-7"
                                ],
                                [
                                  "ul",
                                  {
                                    "id": "children"
                                  },
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0000172",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "branchial muscle"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0005390",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "gill ray"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001606",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 3"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0000095",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "obo:BFO_0000050"
                                      },
                                      "pharyngeal arch 3-7 skeleton"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001607",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 4"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001608",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 5"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001609",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 6"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ],
                                  [
                                    "li",
                                    [
                                      "a",
                                      {
                                        "resource": "obo:ZFA_0001610",
                                        "about": "obo:ZFA_0001613",
                                        "rev": "rdfs:subClassOf"
                                      },
                                      "pharyngeal arch 7"
                                    ],
                                    [
                                      "ul",
                                      {
                                        "id": "children"
                                      }
                                    ]
                                  ]
                                ]
                              ]
                            ]
                          ]
                        ]
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
                    "resource": "obo:ZFA_0000496",
                    "about": "obo:ZFA_0000037",
                    "rev": "rdfs:subClassOf"
                  },
                  "compound organ"
                ],
                [
                  "ul",
                  [
                    "li",
                    [
                      "a",
                      {
                        "resource": "obo:ZFA_0000354",
                        "about": "obo:ZFA_0000496",
                        "rev": "rdfs:subClassOf"
                      },
                      "gill"
                    ],
                    [
                      "ul",
                      {
                        "id": "children"
                      }
                    ]
                  ]
                ]
              ],
              [
                "li",
                [
                  "a",
                  {
                    "resource": "obo:ZFA_0001094",
                    "about": "obo:ZFA_0000037",
                    "rev": "rdfs:subClassOf"
                  },
                  "whole organism"
                ],
                [
                  "ul",
                  [
                    "li",
                    [
                      "a",
                      {
                        "resource": "obo:ZFA_0001439",
                        "about": "obo:ZFA_0001094",
                        "rev": "obo:BFO_0000050"
                      },
                      "anatomical system"
                    ],
                    [
                      "ul",
                      [
                        "li",
                        [
                          "a",
                          {
                            "resource": "obo:ZFA_0000272",
                            "about": "obo:ZFA_0001439",
                            "rev": "rdfs:subClassOf"
                          },
                          "respiratory system"
                        ],
                        [
                          "ul",
                          [
                            "li",
                            [
                              "a",
                              {
                                "resource": "obo:ZFA_0000354",
                                "about": "obo:ZFA_0000272",
                                "rev": "obo:BFO_0000050"
                              },
                              "gill"
                            ],
                            [
                              "ul",
                              {
                                "id": "children"
                              }
                            ]
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
                        "resource": "obo:ZFA_0000496",
                        "about": "obo:ZFA_0001094",
                        "rev": "obo:BFO_0000050"
                      },
                      "compound organ"
                    ],
                    [
                      "ul",
                      [
                        "li",
                        [
                          "a",
                          {
                            "resource": "obo:ZFA_0000354",
                            "about": "obo:ZFA_0000496",
                            "rev": "rdfs:subClassOf"
                          },
                          "gill"
                        ],
                        [
                          "ul",
                          {
                            "id": "children"
                          }
                        ]
                      ]
                    ]
                  ]
                ]
              ]
            ]
          ]
        ]
      ]
    ]
  ]
]"#;

    let expected = from_str::<Value>(expected_string);

    assert_eq!(hiccup, expected.unwrap());
}

#[tokio::test]
async fn test_get_html_top_hierarchy() {
    let connection = "src/resources/test_data/zfa_excerpt_for_term_trees.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let table = "statement";
    let subject = "obo:ZFA_0000354";

    //boolean is for preferred root terms 
    let top_class_hierarchy = get_html_top_hierarchy("Class", table, &pool).await.unwrap();

    let expected_string = r#"
[
  "ul",
  [
    "li",
    "Ontology"
  ],
  [
    "li",
    "Class",
    [
      "ul",
      {
        "id": "children"
      },
      [
        "li",
        [
          "a",
          {
            "resource": "obo:ZFA_0100000",
            "rev": "rdfs:subClassOf"
          },
          "zebrafish anatomical entity"
        ]
      ]
    ]
  ]
]"#;

    let expected = from_str::<Value>(expected_string);

    assert_eq!(top_class_hierarchy, expected.unwrap());

}
