use clap::{command, Command};
use std::error;
use std::fs;
use std::path::Path;

fn init() -> Result<&'static str, &'static str> {

    match fs::create_dir_all("src/schema"){
        Err(_x) => return Err("Couldn't create folder src/schema"),
        Ok(_x) => {} 
    };

    match create_table_tsv() {
        Err(_x) => return Err("Couldn't write table.tsv"),
        Ok(_x) => {}
    };

    match create_column_tsv() {
        Err(_x) => return Err("Couldn't write column.tsv"),
        Ok(_x) => {}
    };

    match create_datatype_tsv() {
        Err(_x) => return Err("Couldn't write datatype.tsv"),
        Ok(_x) => {}
    };

    if Path::new("nanobot.toml").exists() {
        Err("nanobot.toml file already exists.")
    } else {
        fs::copy("src/resources/default_config.toml", "nanobot.toml").unwrap();
        Ok("Hello world")
    }
}

fn create_table_tsv() -> Result<(), Box<dyn error::Error>> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_path("src/schema/table.csv")?;

    wtr.write_record(&["table", "path", "description", "type"])?;
    wtr.write_record(&[
        "table",
        "src/schema/table.tsv",
        "All of the tables in this project.",
        "table",
    ])?;
    wtr.write_record(&[
        "column",
        "src/schema/column.tsv",
        "Columns for all of the tables.",
        "column",
    ])?;
    wtr.write_record(&[
        "datatype",
        "src/schema/datatype.tsv",
        "Datatypes for all of the columns",
        "datatype",
    ])?;

    wtr.flush()?;
    Ok(())
}

fn create_column_tsv() -> Result<(), Box<dyn error::Error>> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_path("src/schema/column.csv")?;

    wtr.write_record(&[
        "table",
        "column",
        "nulltype",
        "datatype",
        "structure",
        "description",
    ])?;
    wtr.write_record(&[
        "table",
        "table",
        "",
        "label",
        "unique",
        "name of this table",
    ])?;
    wtr.write_record(&[
        "table",
        "path",
        "",
        "line",
        "",
        "path to the TSV file for this table, relative to the table.tsv file",
    ])?;
    wtr.write_record(&[
        "table",
        "type",
        "empty",
        "table_type",
        "",
        "type of this table, used for tables with special meanings",
    ])?;
    wtr.write_record(&[
        "table",
        "description",
        "empty",
        "text",
        "",
        "a description of this table",
    ])?;
    wtr.write_record(&[
        "column",
        "table",
        "",
        "label",
        "from(table.table)",
        "the table that this column belongs to",
    ])?;
    wtr.write_record(&[
        "column",
        "column",
        "",
        "label",
        "",
        "the name of this column",
    ])?;
    wtr.write_record(&[
        "column",
        "nulltype",
        "empty",
        "word",
        "from(datatype.datatype)",
        "the datatype for NULL values in this column",
    ])?;
    wtr.write_record(&[
        "column",
        "datatype",
        "",
        "word",
        "from(datatype.datatype)",
        "the datatype for this column",
    ])?;
    wtr.write_record(&[
        "column",
        "structure",
        "empty",
        "label",
        "",
        "schema information for this column",
    ])?;
    wtr.write_record(&[
        "column",
        "description",
        "empty",
        "text",
        "",
        "a description of this column",
    ])?;
    wtr.write_record(&[
        "datatype",
        "datatype",
        "",
        "word",
        "primary",
        "the name of this datatype",
    ])?;
    wtr.write_record(&[
        "datatype",
        "parent",
        "empty",
        "word",
        "tree(datatype)",
        "the parent datatype",
    ])?;
    wtr.write_record(&[
        "datatype",
        "condition",
        "empty",
        "line",
        "",
        "the method for testing the datatype",
    ])?;
    wtr.write_record(&[
        "datatype",
        "description",
        "empty",
        "text",
        "",
        "a description of this datatype",
    ])?;
    wtr.write_record(&[
        "datatype",
        "SQL type",
        "empty",
        "sql_type",
        "",
        "the SQL type for representing this data",
    ])?;
    wtr.write_record(&[
        "datatype",
        "HTML type",
        "empty",
        "html_type",
        " ",
        "the HTML type for viewing and editing this data",
    ])?; //NB: use of white space

    wtr.flush()?;
    Ok(())
}

fn create_datatype_tsv() -> Result<(), Box<dyn error::Error>> {
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_path("src/schema/datatype.csv")?;

    wtr.write_record(&[
        "datatype",
        "parent",
        "condition",
        "description",
        "SQL type",
        "HTML type",
    ])?;
    wtr.write_record(&["text", "", "", "any text", "TEXT", "textarea"])?;
    wtr.write_record(&[
        "empty",
        "text",
        "equals('')",
        "the empty string",
        "NULL",
        "",
    ])?;
    wtr.write_record(&[
        "line",
        "text",
        "exclude(/\\\\\\\\\\\\\\n/)",
        "one line of text",
        "",
        "text",
    ])?;
    wtr.write_record(&[
        "label",
        "line",
        "match(/[^\\s]+.+[^\\s]/)",
        "text that does not begin or end with whitespace",
        "",
        "",
    ])?;
    wtr.write_record(&[
        "word",
        "label",
        "exclude(/\\W/)",
        "a single word: letters, numbers, underscore",
        "",
        "",
    ])?;
    wtr.write_record(&[
        "table_type",
        "word",
        "in('table', 'column', 'datatype')",
        "a VALVE table type",
        "",
        "search",
    ])?;
    wtr.write_record(&[
        "sql_type",
        "word",
        "in('NULL', 'TEXT', 'INT')",
        "a SQL type",
        "",
        "search",
    ])?;
    wtr.write_record(&[
        "html_type",
        "word",
        "in('text', 'textarea', 'search', 'radio', 'number', 'select')",
        "an HTML form type",
        "",
        "search",
    ])?;

    wtr.flush()?;
    Ok(())
}

fn main() {
    let matches = command!() // requires `cargo` feature
        .propagate_version(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("init").about("Initialises things"))
        .get_matches();

    let exit_result = match matches.subcommand() {
        Some(("init", _sub_matches)) => init(),
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    };

    //print exit message
    match exit_result {
        Err(x) => {
            println!("{}", x);
            std::process::exit(1)
        }

        Ok(x) => println!("{}", x),
    }
}
