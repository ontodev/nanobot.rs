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

Building on the `../table/` example,
in this example the 'table' table has several more entries,
and the 'column' table makes use of `from()` structures.
The `from()` structures specify a column from another table
that will be used as a SQL foreign key constraint.
For example, the 'penguin.species' column
has `from(species.name)` as its structure,
which means that all values in 'penguin.species'
must appear as values in the 'species.name' column.

This approach has several advantages.
Tables such as 'species'
are clearly separated from the other tables,
and can be used as constraints on multiple columns.
It's easy to add and remove rows from 'species' as required,
and track changes in a version control system.
In other examples we will see
how these tables can be enriched with additional information.
The Nanobot web interface links tables together using the `from()` structures,
making it easy to jump between linked tables.

Note that for this to work in SQL,
the target column must have a 'unique' or 'primary' structure.
If the target column is not specified to be 'unique' or 'primary',
Nanobot will make add a 'unique' constraint automatically.

In SQL, foreign key relations must target a different table
-- not just another column in the same table.
Nanobot allows `from()` structures to target columns in the same table,
and will validate them as expected,
but in this case it cannot create a SQL foreign key constraint.
