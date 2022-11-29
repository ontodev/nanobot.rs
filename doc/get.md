# `get`: Get Rows

You can read rows from tables using `nanobot get`:

```console tesh-session="test"
$ nanobot init
Initialized a Nanobot project
$ nanobot get table
row_number  table     path                     description                         type
1           table     src/schema/table.tsv     All of the tables in this project.  table
2           column    src/schema/column.tsv    Columns for all of the tables.      column
3           datatype  src/schema/datatype.tsv  Datatypes for all of the columns    datatype
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
    "row_number": 1,
    "table": "table",
    "path": "src/schema/table.tsv",
    "description": "All of the tables in this project.",
    "type": "table"
  },
  {
    "row_number": 2,
    "table": "column",
    "path": "src/schema/column.tsv",
    "description": "Columns for all of the tables.",
    "type": "column"
  },
  {
    "row_number": 3,
    "table": "datatype",
    "path": "src/schema/datatype.tsv",
    "description": "Datatypes for all of the columns",
    "type": "datatype"
  }
]
```
