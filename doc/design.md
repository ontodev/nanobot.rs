# Nanobot Design

**WARNING: These features are planned, not yet implemented.**

Nanobot is a tool for working with various tabular data.
A Nanobot project is laid out like so:

- `nanobot.toml` project configuration
- `src/` source files
  - `schema/` tables about tables
    - `table.tsv` from VALVE
    - `column.tsv` from VALVE
    - `datatype.tsv` from VALVE
    - `rule.tsv` from VALVE, optional
  - `workflow/` describe data flows
    - `cache.tsv`
    - `conversion.tsv`
- `data/` project data tables
- `cache/` upstream data sources,
  not modified,
  not version controlled

Nanobot helps you edit these tables,
then runs commands using these tables.

The `add` command appends a row to a table,
filling the first N columns with the values you provide:

```sh
$ nanobot add penguin N1A1 "" Torgensen MALE 3750
1
```

The `add` command returns the row number of the new row.

Any missing values are filled with default values,
unless the `--default none` option is specified.

If the new row fails VALVE validation,
it will still be added,
but error messages will be written to STDERR.
You can supply a `--fail-on [level]` option.

Unless the `--action none` option is specified,
Nanobot will run actions for that table
immediately after the row is added.
Nanobot has built in actions for:

- 'table': guess column types, load table
- 'column': add required 'datatype' rows
- 'datatype': fetch datatype definition from registry
- 'cache': fetch upstream data
- 'conversion': convert upstream data to TSV

For example, to put a new source file in the cache,
we first add a row to the 'cache' table.
When a new row is added to 'cache'
Nanobot automatically runs the `pull` command
on that new row.

```sh
$ nanobot add cache table_219.csv "https://portal.edirepository.org/nis/dataviewer?packageid=knb-lter-pal.219.3&entityid=002f3893385f710df69eeebe893144ff" 
1
```

If we want to `pull` that entry later, we can run:

```sh
$ nanobot pull 1
```

Or we could add a new table from an existing TSV file:

```sh
$ nanobot add table penguin data/penguin.tsv
```

After this row is added to the 'table' table
Nanobot will read `data/penguin.tsv`,
fill out the 'column' table with best guesses,
which may trigger additions to the 'datatype' table,
and then load the data into 'penguin'.

Or we could add a row to the 'ontology' table:

```sh
$ nanobot add ontology NCBITaxon
```

This will figure out that "NCBITaxon" is an OBO ontology,
fetch an LDTab table for the lastest version into the cache,
configure the schema tables as required,
load the LDTab rows into an 'NCBITaxon' table,
and update the 'prefix' table as required.

## Subcommands

- project
  - `init` initialize a new project
  - `config` configure a project
  - `serve` an HTTP API and web site
- rows
  - `get` existing row or rows
  - `add` a new row or rows
  - `check` (validate) a new row or rows without adding them
  - `set` a row or rows to new values
  - `delete` an existing row or rows
  - `edit` an existing row in your terminal's EDITOR
  - `copy` a row or rows, editing in your terminal's EDITOR, and add them
- workflows
  - `fetch` upstream files to cache
  - `pull` upstream files: fetch and use the new version
  - `convert` upstream files to project data tables
  - `reload` tables from TSV
  - `save` tables to TSV
- other
  - `map` terminology to ontologies

