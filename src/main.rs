use clap::{arg, command, value_parser, Command};
use std::error;
use std::fs;
use std::fs::File;
use std::path::Path;

fn init(database: &str) -> Result<&'static str, &'static str> {
    match fs::create_dir_all("src/schema") {
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

    //create database
    match File::create(database) {
        Err(_x) => return Err("Couldn't create database"),
        Ok(_x) => {}
    }

    if Path::new("nanobot.toml").exists() {
        Err("nanobot.toml file already exists.")
    } else {
        fs::copy("src/resources/default_config.toml", "nanobot.toml").unwrap();
        Ok("Hello world")
    }
}

fn create_table_tsv() -> Result<(), Box<dyn error::Error>> {
    let data = r#"table	path	description	type
table	src/schema/table.tsv	All of the tables in this project.	table
column	src/schema/column.tsv	Columns for all of the tables.	column
datatype	src/schema/datatype.tsv	Datatypes for all of the columns	datatype
"#;
    fs::write("src/schema/table.csv", data).expect("Unable to write file");

    Ok(())
}

fn create_column_tsv() -> Result<(), Box<dyn error::Error>> {
    let data = r#"table	column	nulltype	datatype	structure	description
table	table		label	unique	name of this table
table	path		line		path to the TSV file for this table, relative to the table.tsv file
table	type	empty	table_type		type of this table, used for tables with special meanings
table	description	empty	text		a description of this table
column	table		label	from(table.table)	the table that this column belongs to
column	column		label		the name of this column
column	nulltype	empty	word	from(datatype.datatype)	the datatype for NULL values in this column
column	datatype		word	from(datatype.datatype)	the datatype for this column
column	structure	empty	label		schema information for this column
column	description	empty	text		a description of this column
datatype	datatype		word	primary	the name of this datatype
datatype	parent	empty	word	tree(datatype)	the parent datatype
datatype	condition	empty	line		the method for testing the datatype
datatype	description	empty	text		a description of this datatype
datatype	SQL type	empty	sql_type		the SQL type for representing this data
datatype	HTML type	empty	html_type	 	the HTML type for viewing and editing this data
"#;
    fs::write("src/schema/column.csv", data).expect("Unable to write file");

    Ok(())
}

fn create_datatype_tsv() -> Result<(), Box<dyn error::Error>> {
    let data = r#"datatype	parent	condition	description	SQL type	HTML type
text			any text	TEXT	textarea
empty	text	equals('')	the empty string	NULL	
line	text	exclude(/\\\\\\\n/)	one line of text		text
label	line	match(/[^\s]+.+[^\s]/)	text that does not begin or end with whitespace		
word	label	exclude(/\W/)	a single word: letters, numbers, underscore		
table_type	word	in('table', 'column', 'datatype')	a VALVE table type		search
sql_type	word	in('NULL', 'TEXT', 'INT')	a SQL type		search
html_type	word	in('text', 'textarea', 'search', 'radio', 'number', 'select')	an HTML form type		search
"#;
    fs::write("src/schema/datatype.csv", data).expect("Unable to write file");

    Ok(())
}

fn main() {
    let matches = command!() // requires `cargo` feature
        .propagate_version(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("init").about("Initialises things").arg(
                arg!(
                    -d --database <FILE> "Specifies a custom database name"
                )
                .required(false)
                .value_parser(value_parser!(String)),
            ),
        )
        .get_matches();

    let exit_result = match matches.subcommand() {
        Some(("init", sub_matches)) => match sub_matches.get_one::<String>("database") {
            Some(x) => init(x),
            _ => init(".nanobot.db"),
        },
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
