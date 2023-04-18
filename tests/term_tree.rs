use nanobot::tree_validation::get_json_tree_view;
use nanobot::tree_view::{get_hiccup_term_tree, get_hiccup_top_hierarchy, get_rich_json_tree_view};
use serde_json::{from_str, json, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::env;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;

async fn set_up_database(tsv: &str, db: &str) -> SqlitePool {
    let test_database = format!("src/resources/.tmp/{}", db);
    let connection_string = format!("sqlite://{}?mode=rwc", test_database);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .unwrap();

    let statement_query = r#"CREATE TABLE statement (
  'transaction' INTEGER NOT NULL,
  'retraction' INTEGER NOT NULL DEFAULT 0,
  'graph' TEXT NOT NULL,
  'subject' TEXT NOT NULL,
  'predicate' TEXT NOT NULL,
  'object' TEXT NOT NULL,
  'datatype' TEXT NOT NULL,
  'annotation' TEXT);"#;
    sqlx::query(&statement_query).execute(&pool).await.unwrap();

    //TODO: I couldn't figure out how to import a tsv file into the sql database
    //also: adding things line by line is bad (but I didn't find anything useful and didn't want to spend more time on this...)
    //let f = File::open("src/resources/test_data/uberon/0002535.tsv").unwrap();
    let f = File::open(tsv).unwrap();
    for line in BufReader::new(f).lines() {
        let line = line.unwrap();
        let line = line.split("\t").collect::<Vec<&str>>();
        let insert_query = format!(
            "INSERT INTO statement VALUES('{}','{}','{}','{}','{}','{}','{}','{}');",
            line[0], line[1], line[2], line[3], line[4], line[5], line[6], line[7]
        );
        sqlx::query(&insert_query).execute(&pool).await.unwrap();
    }
    pool
}

fn tear_down_database(db: &str) {
    let db_destination = format!("src/resources/.tmp/{}", db);
    fs::remove_file(&db_destination).unwrap();
}

#[tokio::test]
async fn test_get_rich_json_tree_view() {
    let database = "0000354_rich_json.db";
    let pool = set_up_database("src/resources/test_data/zfa/0000354.tsv", database).await;

    let table = "statement";
    let subject = "obo:ZFA_0000354";
    //boolean flag is for preferred_roots
    let rich_hierarchy = get_rich_json_tree_view(subject, false, table, &pool)
        .await
        .unwrap();

    let expected_string =
        fs::read_to_string("src/resources/test_data/ldtab_term_trees/ZFA_0000354.json")
            .expect("Should have been able to read the file");
    let expected = from_str::<Value>(&expected_string);

    tear_down_database(database);

    assert_eq!(rich_hierarchy, expected.unwrap());
}

#[tokio::test]
async fn test_get_hiccup_term_tree() {
    let database = "0000354_hiccup.db";
    let pool = set_up_database("src/resources/test_data/zfa/0000354.tsv", database).await;

    let table = "statement";
    let subject = "obo:ZFA_0000354";

    //boolean is for preferred root terms
    let hiccup = get_hiccup_term_tree(&subject, false, table, &pool)
        .await
        .unwrap();

    let expected_string =
        fs::read_to_string("src/resources/test_data/ldtab_term_trees/ZFA_0000354.hiccup")
            .expect("Should have been able to read the file");
    let expected = from_str::<Value>(&expected_string);

    tear_down_database(database);

    assert_eq!(hiccup, expected.unwrap());
}

#[tokio::test]
async fn test_get_hiccup_term_tree_with_preferred_roots() {
    let database = "0000354_preferred_hiccup.db";
    let pool = set_up_database("src/resources/test_data/zfa/0000354.tsv", database).await;

    let table = "statement";
    let subject = "obo:ZFA_0000354";

    //boolean is for preferred root terms
    let hiccup = get_hiccup_term_tree(&subject, true, table, &pool)
        .await
        .unwrap();

    let expected_string = fs::read_to_string(
        "src/resources/test_data/ldtab_term_trees/ZFA_0000354_preferred_roots.hiccup",
    )
    .expect("Should have been able to read the file");
    let expected = from_str::<Value>(&expected_string);

    tear_down_database(database);

    assert_eq!(hiccup, expected.unwrap());
}

#[tokio::test]
async fn test_get_html_top_hierarchy() {
    let database = "0000354_html_top_hierarchy.db";
    let pool = set_up_database("src/resources/test_data/zfa/0000354.tsv", database).await;

    let table = "statement";
    let subject = "obo:ZFA_0000354";

    //boolean is for preferred root terms
    let top_class_hierarchy = get_hiccup_top_hierarchy("Class", table, &pool)
        .await
        .unwrap();

    let expected_string =
        fs::read_to_string("src/resources/test_data/ldtab_term_trees/ZFA_top_classes.hiccup")
            .expect("Should have been able to read the file");
    let expected = from_str::<Value>(&expected_string);

    tear_down_database(database);

    assert_eq!(top_class_hierarchy, expected.unwrap());
}

#[tokio::test]
async fn test_compare_osl_2_ldtab_tree_zfa_0000354() {
    let pool = set_up_database("src/resources/test_data/zfa/0000354.tsv", "0000354.db").await;

    let subject = "obo:ZFA_0000354";
    let ldtab_term_tree: Value = get_json_tree_view(&subject, "statement", &pool).await;

    let osl_tree_path = "src/resources/test_data/ols_term_trees/ZFA_0000354.json";
    let osl_tree_string =
        fs::read_to_string(osl_tree_path).expect("Should have been able to read the file");
    let expected = from_str::<Value>(&osl_tree_string);

    tear_down_database("0000354.db");

    assert_eq!(ldtab_term_tree, expected.unwrap());
}

#[tokio::test]
async fn test_compare_osl_2_ldtab_tree_uberon_0002535() {
    let pool = set_up_database("src/resources/test_data/uberon/0002535.tsv", "0002535.db").await;

    let subject = "obo:UBERON_0002535";
    let ldtab_term_tree: Value = get_json_tree_view(&subject, "statement", &pool).await;

    let osl_tree_path = "src/resources/test_data/ols_term_trees/UBERON_0002535.json";
    let osl_tree_string =
        fs::read_to_string(osl_tree_path).expect("Should have been able to read the file");
    let expected = from_str::<Value>(&osl_tree_string);

    tear_down_database("0002535.db");

    assert_eq!(ldtab_term_tree, expected.unwrap());
}

#[tokio::test]
async fn test_compare_osl_2_ldtab_tree_uberon_0000956() {
    let pool = set_up_database("src/resources/test_data/uberon/0000956.tsv", "0000956.db").await;

    let subject = "obo:UBERON_0000956";
    let ldtab_term_tree: Value = get_json_tree_view(&subject, "statement", &pool).await;

    let osl_tree_path = "src/resources/test_data/ols_term_trees/UBERON_0000956.json";
    let osl_tree_string =
        fs::read_to_string(osl_tree_path).expect("Should have been able to read the file");
    let expected = from_str::<Value>(&osl_tree_string);

    tear_down_database("0000956.db");

    assert_eq!(ldtab_term_tree, expected.unwrap());
}
