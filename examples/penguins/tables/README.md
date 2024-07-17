# Examples: Penguin Tables

This Nanobot example shows multiple tables with foreign key constraints:

- `src/schema/`: schema for the data
  - `table.tsv`: table definitions for all the tables
  - `column.tsv`: column definitions for all the tables
  - `datatype.tsv`: datatype definitions used by the columns
  - `species.tsv`: the species in the study
  - `region.tsv`: the geographical regions in the study
  - `island.tsv`: the islands in the study
  - `stage.tsv`: the developmental stages in the study
  - `sex.tsv`: the biological sexes in the study
- `src/data/`: penguin study data
  - `penguin.tsv`: synthetic data about penguins

Run this example with `nanobot serve --connection :memory:`.
See the [README](../README.md) in the parent directory for more information.
