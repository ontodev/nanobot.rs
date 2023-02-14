use nanobot::tree_view::get_json_tree_view;
use serde_json::{from_str, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::fs;

fn insert_subclass_of_query(subclass: &str, superclass: &str) -> String {
    format!(
        "INSERT INTO statement VALUES('1','0','graph','{subclass}','rdfs:subClassOf','{superclass}','datatype','annotation');",
        subclass = subclass,
        superclass = superclass
    )
}

#[tokio::test]
async fn test_select_new() {
    let connection = "tests/test.db";
    let connection_string = format!("sqlite://{}?mode=rwc", connection);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let query = "CREATE TABLE statement (
  'transaction' INTEGER NOT NULL,
  'retraction' INTEGER NOT NULL DEFAULT 0,
  'graph' TEXT NOT NULL,
  'subject' TEXT NOT NULL,
  'predicate' TEXT NOT NULL,
  'object' TEXT NOT NULL,
  'datatype' TEXT NOT NULL,
  'annotation' TEXT
);";
    sqlx::query(&query).execute(&pool).await.unwrap();

    let subsumptions = vec![
        //("obo:ZFA_0100000", "owl:Thing"),
        ("obo:ZFA_0000272", "obo:ZFA_0001439"),
        (
            "obo:ZFA_0000272",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002497"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000044"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0000272",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000000"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        ("obo:ZFA_0001439", "obo:ZFA_0001512"),
        (
            "obo:ZFA_0001439",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0001094"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0001439",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000000"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0001439",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002497"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000044"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        ("obo:ZFA_0001512", "obo:ZFA_0000037"),
        (
            "obo:ZFA_0001512",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000000"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0001512",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002497"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000044"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        ("obo:ZFA_0000037", "obo:ZFA_0100000"),
        (
            "obo:ZFA_0000037",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000001"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0000037",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002497"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000044"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0000354",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002202"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0001107"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0000354",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002497"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000044"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0000354",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000000"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0000354",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0000272"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        ("obo:ZFA_0000354", "obo:ZFA_0000496"),
        (
            "obo:ZFA_0001094",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002497"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000044"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0001094",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000001"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        ("obo:ZFA_0001094", "obo:ZFA_0000037"),
        ("obo:ZFA_0000496", "obo:ZFA_0000037"),
        (
            "obo:ZFA_0000496",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:BFO_0000050"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFA_0001094"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0000496",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002496"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000000"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
        (
            "obo:ZFA_0000496",
            r#"{"owl:onProperty":[{"datatype":"_IRI","object":"obo:RO_0002497"}],"owl:someValuesFrom":[{"datatype":"_IRI","object":"obo:ZFS_0000044"}],"rdf:type":[{"datatype":"_IRI","object":"owl:Restriction"}]}"#,
        ),
    ];

    for (sub, sup) in subsumptions {
        let query = insert_subclass_of_query(sub, sup);
        sqlx::query(&query).execute(&pool).await.unwrap();
    }

    let results = get_json_tree_view("obo:ZFA_0000354", "statement", &pool).await;

    let expected_string = r#"{"obo:ZFA_0100000":{"obo:ZFA_0000037":{"obo:ZFA_0000496":{"obo:ZFA_0000354":"owl:Nothing"},"obo:ZFA_0001094":{"partOf obo:ZFA_0000496":{"obo:ZFA_0000354":"owl:Nothing"},"partOf obo:ZFA_0001439":{"obo:ZFA_0000272":{"partOf obo:ZFA_0000354":"owl:Nothing"}}},"obo:ZFA_0001512":{"obo:ZFA_0001439":{"obo:ZFA_0000272":{"partOf obo:ZFA_0000354":"owl:Nothing"}}}}}}"#;
    let expected_value = from_str::<Value>(expected_string);

    fs::remove_file("tests/test.db").expect("File deleted failed");

    assert_eq!(results, expected_value.unwrap());
}
