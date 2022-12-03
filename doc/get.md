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

We can get more data about a row --
enough to display it in a rich format
such as an HTML table or form.

```console
$ nanobot get table --format valve.json --limit 1 | jq
{
  "format": "valve.json",
  "table": {
    "table": "table",
    "path": "src/schema/table.tsv",
    "description": "All of the tables in this project.",
    "type": "table",
    "query": "table?limit=1",
    "limit": 1,
    "offset": 0
  },
  "column": {
    "table": {
      "datatype": "label",
      "structure": "unique",
      "description": "name of this table"
    },
    "path": {
      "datatype": "line",
      "description": "path to the TSV file for this table, relative to the table.tsv file"
    },
    "description": {
      "nulltype": "empty",
      "datatype": "text",
      "description": "a description of this table"
    },
    "type": {
      "nulltype": "empty",
      "datatype": "table_type",
      "description": "type of this table, used for tables with special meanings"
    }
  },
  "datatype": {
    "text": {
      "description": "any text",
      "HTML type": "textarea"
    },
    "empty": {
      "parent": "text",
      "condition": "equals('')",
      "description": "the empty string",
      "HTML type": "none"
    },
    "line": {
      "parent": "text",
      "condition": "exclude(/\n/)",
      "description": "one line of text",
      "HTML type": "text"
    },
    "label": {
      "parent": "line",
      "condition": "match(/[^\\s]+.+[^\\s]/)",
      "description": "text that does not begin or end with whitespace"
    },
    "word": {
      "parent": "label",
      "condition": "exclude(/\\W/)",
      "description": "a single word: letters, numbers, underscore"
    },
    "table_type": {
      "parent": "word",
      "condition": "in('table', 'column', 'datatype')",
      "description": "a VALVE table type",
      "HTML type": "select"
    }
  },
  "row": [
    {
      "table": {
        "value": "table",
        "datatype": "label"
      },
      "path": {
        "value": "src/schema/table.tsv",
        "datatype": "line"
      },
      "description": {
        "value": "All of the tables in this project.",
        "datatype": "text"
      },
      "type": {
        "value": "table",
        "datatype": "table_type"
      }
    }
  ]
}
```

We can also ask for additional context
about the Nanobot project
and where this result is located in it.
This is enough information to render a full HTML page.

```console
$ nanobot get table --format nanobot.json --limit 1 | jq
{
  "format": "nanobot.json",
  "page": {
    "url": "/table?limit=1",
    "path": "/table",
    "home": "/",
    "help": "/help",
    "table": {
      "table": {},
      "column": {},
      "datatype": {}
    },
    "link": [
      {
        "href": "https://cdn.jsdelivr.net/npm/bootstrap@5.2.3/dist/css/bootstrap.min.css",
        "rel": "stylesheet",
        "integrity": "sha384-rbsA2VBKQhggwzxH7pPCaAqO46MgnOM80zW1RWuH61DGLwZJEdK2Kadq2F9CUG65",
        "crossorigin": "anonymous"
      }
    ],
    "script": [
      {
        "src": "https://cdn.jsdelivr.net/npm/bootstrap@5.2.3/dist/js/bootstrap.bundle.min.js",
        "integrity": "sha384-kenU1KFdBIe4zVF0s0G1M5b4hcpxyD9F7jL+jjXkk+Q2h455rYXK/7HAuoJl+0I4",
        "crossorigin": "anonymous"
      }
    ]
  },
  ...
}
```

For these more complex formats the basic algorithm is:

1. load the configuration
2. lookup the table in 'table'
3. get the columns for the table from 'column'
4. get the tree of datatypes for the columns and nulltypes from 'datatype'
5. get the rows from the table, with their row numbers
6. get nulltypes from 'cell' by table and row number
7. get messages from 'messages' by table and row number
8. merge these results into JSON
9. convert the JSON to the final format, e.g. HTML
