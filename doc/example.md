# Example

**WARNING: These features are planned, not yet implemented.**

This example document shows how Nanobot works
using example data from
[Structural size measurements and isotopic signatures of foraging among adult male and female Adélie penguins (Pygoscelis adeliae) nesting along the Palmer Archipelago near Palmer Station, 2007-2009](https://doi.org/10.6073/pasta/abc50eed9138b75f54eaada0841b9b86)
This data is also available through <https://github.com/allisonhorst/palmerpenguins/>.
I ran across it in this [Quarto example](https://quarto.org/docs/interactive/ojs/examples/penguins.html).

We run through these steps:

1. create a new Nanobot project
2. collect the source data
3. select the subset that we want
4. connect it to some OBO ontologies
5. revise out validation rules
6. resolve any remaining problems
7. start working with the data in Quarto

## 1. Create

The first thing to do is create a new directory for the project,
and initialize it with `git` and `nanobot`.

```sh
$ mkdir penguins
$ cd penguins
$ git init
$ nanobot init
```

Nanobot initializes a new project in these steps:

- create nanobot.toml config file
- create .nanobot.db database file
  - add .nanobot.db to .gitignore
- create src/ directory
- create schema/ directory
  - create meta tables: table, column, datatype, rule

At any time,
you can check that your nanobot project is configured properly:

```sh
$ nanobot config check
```

## 2. Collect

First we need to collect our upstream data.
We will cache this data,
and we will not change it.

We will keep careful track of all our steps,
so that later we can update this upstream data
and run the entire workflow again.
If the upstream data hasn't changed much,
then the whole workflow will run again smoothly.
If the changes are larger,
we may need to update our workflow,
but everything will be clear in the version control history.

The first thing to do is add upstream source data.
In this case, there

```sh
$ nanobot add cache <source> <url>
fetched <url>
SHA1 hash 12445abc
saved to cache/<hash>-<source>.format
```

In general,
the `add` command adds a new row to a table.
The 'cache' table is a special one for Nanobot.
If it does not alread exist,
Nanobot will create this table in `src/workflow/cache.tsv`,
and update the 'table', 'column', and 'datatype' tables
to include it.

After adding this row,
the 'cache' table will look like this:

name         | url | time | etag | hash
---          |---  |---   |---   |---
penguins.zip | <>  |      |      |

We tell Nanobot to download and update all the cached files:

```sh
$ nanobot pull
```

Nanobot will create the `cache/` directory
(if it does not already exist),
and add it to `.gitignore`.
Then Nanobot will download the file and store it in `cache/`.
Keeping track of different versions of files is very important,
so Nanobot takes a hash of cached files and adds that to the filename.
Nanobot will also store the data and any HTTP eTag value
in the 'cache' table.

name | url | time | etag | hash
---|---|---|---|---
penguins.zip | <> | 2022-11-22T12:34:56Z | aefiwehf | 23r23awfe

## 3. Select

The `pengins.zip` file contains several files.
We care about the `table_219.csv` file specifically,
but only a few of its columns.
We also want to convert it to TSV format.
For this we use the `select` command.

```sh
$ nanobot add conversion penguins.zip table_219.csv penguin "select=Individual ID,Species,Island,Body Mass (g)"
saved to data/<table>.tsv
```

This creates the 'conversion' table and adds a row:

source | path | table | query | time | hash
---|---|---|---|---|---
penguins.zip | table_219.csv | penguin | select=Individual ID,Species,Island,Body Mass (g) | |

TODO: Properly handle hashes for inputs and outputs.

Then we tell Nanobot to run the conversions:

```sh
$ nanobot convert
```

This updates the 'conversion' table:

source | path | table | query | time | hash
---|---|---|---|---|---
penguins.zip | table_219.csv | penguin | select=Individual ID,Species,Island,Body Mass (g)&limit=1 | 2022-11-22T12:34:56.789Z | wer234

It also creates a 'penguin' table,
stored in `data/penguin.tsv`:

Individual ID | Species | Island | Body Mass (g)
---|---|---|---
N1A1 | Adelie Penguin (Pygoscelis adeliae) | Torgersen | 3750

The 'table' table will also be updated to add the 'penguin' row

table | path
---|---
penguin | data/penguin.tsv

VALVE will guess the column types and update the 'column' table:

table   | column     | nulltype | datatype | structure | description
--------|---------------|-------|---------|---------|---
penguin | Individual ID |       | word    | primary |
penguin | Species       |       | label   |         |
penguin | Island        |       | word    |         |
penguin | Body Mass (g) |       | integer |         |

## 4. Connect

Some of our data uses terminology that we should standardize.
The next step is to find ontology terms.

Seach OLS

- NCBITaxonomy is a good source for species,
  and we can find "Pygoscelis adeliae"
- GAZ is a good source for geographic locations,
  and we can find Torgersen

```sh
$ nanobot load NCBITaxon
$ nanobot load GAZ
```

- check that these are OBO ontologies
- look for LDTab file
- otherwise fetch OWL and convert to LDTab
- update 'table' table?
- create and populate a 'prefix' table

Select the terminology from the ontologies.

```sh
$ nanobot map measurement Island "Torgersen" exact GAZ "Torgersen Island"
```

- add "GAZ" to source_ontology table
- add "GAZ" "GAZ:00045064" "Torgersen Island" to import table
- add some ancestors to import table?
- add "Torgersen" "GAZ:00045064" to measurement_island_term table?
- edit column table structure to "from(terminology_island)"

## 5. Revise

Update the VALVE schema,
reload.

```sh
$ nanobot set column datatype integer
$ nanobot add datatype
{
  "datatype": ""
}
```

### 6. Resolve

Update selected values,
reload?

```sh
$ nanobot set <table> <row> <column> <value> --user --comment
$ nanobot save
$ git stuff
```

### 7. Continue

- use the data in Quarto, Pandas, etc.



