# `get`: Get Rows

You can read rows from tables using `nanobot get`:

```console tesh-session="test"
$ nanobot init
Initialized a Nanobot project
$ nanobot get table
table     path                     description                         type
table     src/schema/table.tsv     All of the tables in this project.  table
column    src/schema/column.tsv    Columns for all of the tables.      column
datatype  src/schema/datatype.tsv  Datatypes for all of the columns    datatype
```

This reads the first 100 rows of the 'table' table
and prints them to STDOUT with "elastic tabstops"
for human-readability.

For machine-readability use `--format json`.
The output is designed to match [PostgREST](https://postgrest.org).
Piping the output through `jq` makes it easier to read:

```console
$ nanobot get table --format json | jq
[
  {
    "table": "table",
    "path": "src/schema/table.tsv",
    "description": "All of the tables in this project.",
    "type": "table"
  },
  {
    "table": "column",
    "path": "src/schema/column.tsv",
    "description": "Columns for all of the tables.",
    "type": "column"
  },
  {
    "table": "datatype",
    "path": "src/schema/datatype.tsv",
    "description": "Datatypes for all of the columns",
    "type": "datatype"
  }
]
```
