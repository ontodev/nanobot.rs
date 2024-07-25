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

This example demonstrates:

- the 'table' table specifies
  - TSV file paths
  - special table types such as 'table', 'column', and 'datatype'
- the 'column' table specifies
  - column names and labels
  - a "nulltype" specifying which TSV strings should be NULL values in SQL
  - a datatype for the column
  - a structure for the column: 'primary', 'unique', or 'from()' another column
- the 'datatype' table specifies a hierarchy of datatypes
  - the condition can be:
    - `equals()` for exact matches
    - `in()` for enumerations
    - `match()` for regular expressions matching a full cell
  - the 'description' is used when reporting error messages
  - the 'sql_type' specifies the column's type in SQL
  - the 'html_type' specifies how the column is displayed in HTML

Datatypes form a hierarchy,
from the most general 'text' type
to very specific types for your data.
When validating the value of a cell,
Nanobot first checks to see if the column has a nulltype,
which will be a datatype from the 'datatype' table,
such as 'empty'.
If a nulltype is specified for the column
and the cell matches the nulltype,
then a NULL value will be inserted into the SQL database.
If no nulltype is specified
or the specified nulltype does not match,
then Nanobot checks the value against column's datatype.
If that check fails,
then Nanobot will also check all the ancestors
of that datatype in the hierarchy,
and report validation messages for all that fail.
This helps Nanobot to provide clear and helpful validation messages.

In this example we see how the various columns of the 'penguin' table:
use nulltype 'empty' when values are not required;
use very general datatypes such as 'trimmed_line' and 'word';
use numeric datataypes such as 'natural_number' and 'positive_decimal';
and use very specific custom datatypes such as 'individual_id'.

The 'penguin' table is created by the `./generate.py` script,
which first produces completely valid data,
and then introduces random validation errors of various types.
Browsing the 'penguin' table in the web interface
you can see the invalid cells
and their validation messages,
all defined by the 'table', 'column', and 'datatype' tables.
You can also edit the 'penguin' table
to fix these errors or introduce new errors.
