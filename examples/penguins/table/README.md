# Examples: Penguin Table

This basic Nanobot example includes four tables:

- `src/schema/`: schema for the data
  - `table.tsv`: table definitions for all the tables
  - `column.tsv`: column definitions for all the tables
  - `datatype.tsv`: datatype definitions used by the columns
- `src/data/`: penguin study data
  - `penguin.tsv`: synthetic data about penguins

Run this example with `nanobot serve --connection :memory:`.
See the [README](../README.md) in the parent directory for more information.
