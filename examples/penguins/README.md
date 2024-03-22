# Nanobot Examples: Penguins

This directory contains a series of Nanobot examples,
based on the
[Palmer Penguins](https://allisonhorst.github.io/palmerpenguins/)
data collected and made available by
[Dr. Kristen Gorman](https://www.uaf.edu/cfos/people/faculty/detail/kristen-gorman.php)
and the
[Palmer Station, Antarctica LTER](https://pallter.marine.rutgers.edu/),
a member of the
[Long Term Ecological Research Network](https://lternet.edu/).

See the [README](../README.md) in the parent directory for more information.

## Examples

The simplest "[table](table/)" example
demonstrates most of Nanobot's features.
The following examples show additional functionality
and increasingly powerful workflows.

1. [table](table/)
2. tables

## Example Data

The `generate.py` script generates "synthetic" (i.e. random) data
with columns and ranges of values similar to Palmer Penguins,
as many rows as we want,
and a specified rate of randomly generated errors.
(Note that the probability distribution of the random values
is not the same as the real Palmer Penguins data.)
This lets us generate as many rows as we like,
with whatever error rate we choose,
and test Nanobot on small or large tables of realistic data.

Each example directory includes a `src/data/penguin.tsv` table
with 1000 rows and a 10% error rate.

You can test variations of the `penguin.tsv` table
by using `generate.py` to generate more random rows
with a specified error rate.
Run `python3 generate.py --help` for more information.
For example, to test the "[table](table/)" example
using a million rows with a 1% error rate,
run `python3 generate.py table/src/data/penguin.tsv 1000000 1`.
You can restore original table
by running `git checkout table/src/data/penguins.tsv`.
