# Examples: Penguin Table

This basic Nanobot example includes four tables:

- [`src/schema/`](src/schema/): schema for the data
  - [`table.tsv`](src/schema/table.tsv): table definitions for all the tables
  - [`column.tsv`](src/schema/column.tsv): column definitions for all the tables
  - [`datatype.tsv`](src/schema/datatype.tsv): datatype definitions used by the columns
- [`src/data/`](src/data/): penguin study data
  - [`penguin.tsv`](src/data/penguin.tsv): synthetic data about penguins

Run this example with `nanobot serve --connection :memory:`.
See the [README](../README.md) in the parent directory for more information.
