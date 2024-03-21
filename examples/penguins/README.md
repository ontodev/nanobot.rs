# Examples: Penguins

This directory contains a series of Nanobot examples,
based on the
[Palmer Penguins](https://allisonhorst.github.io/palmerpenguins/)
data collected and made available by
[Dr. Kristen Gorman](https://www.uaf.edu/cfos/people/faculty/detail/kristen-gorman.php)
and the
[Palmer Station, Antarctica LTER](https://pallter.marine.rutgers.edu/),
a member of the
[Long Term Ecological Research Network](https://lternet.edu/).

## Examples

The simplest "table" example demonstrates most of Nanobot's features.
The following examples show additional functionality
and increasingly powerful workflows.

1. table
2. tables

## Running Examples

You can run each of these examples individually from its directory.
First get a `nanobot` binary,
either by downloading a
[release](https://github.com/ontodev/nanobot.rs/releases),
or by using `cargo build` to build `target/debug/nanobot`.
Second, make sure that the `nanobot` binary is on your
[`PATH`](https://opensource.com/article/17/6/set-path-linux).
Then inside the directory:

1. run `nanobot init` to load and validate the tables,
   creating the `nanobot.toml` configuration file
   and the `.nanobot.db` SQLite database file
2. run `nanobot serve` to start the web server,
   then open <http://localhost:3000> in your web browser
3. press `^C` (Control-C) to stop the web server
4. delete the `.nanobot.db` file

If you're running into errors,
you can configure more verbose logging
by adding this to the `nanobot.toml` file:

```toml
[logging]
level = "DEBUG"
```

## Example Data

The `generate.py` script generates random data
with columns and ranges of values similar to Palmer Penguins,
as many rows as we want,
and a specified rate of randomly generated errors in the data.
(Note that the probability distribution of the random values
is not the same as the real Palmer Penguins data.)
This lets us generate as many rows as we like,
with whatever error rate we choose,
and test Nanobot on small or large tables of realistic data.

Each example includes a `src/data/penguin.tsv` table
with 1000 rows and a 10% error rate.

You can test variations of the `penguin.tsv` table
by using `generate.py` to generate more random rows
with a specified error rate.
Run `python3 generate.py --help` for more information.
For example, to test the "table" example
using a million rows with a 1% error rate,
run `python3 generate.py table/src/data/penguin.tsv 1000000 1`.
