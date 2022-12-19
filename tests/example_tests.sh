#!/bin/sh -e

NANOBOT="$(pwd)/target/release/nanobot"
DB=".nanobot.db"

rm -rf temp
mkdir temp/
cp -r examples/penguins/* temp/
cd temp
mkdir -p src/data
echo 'Generating random data...'
time python3 generate.py
echo 'Initializing Nanobot...'
time "$NANOBOT" init
# tree -a
# sqlite3 "$DB" "SELECT COUNT() FROM penguin"
echo 'Indexing...'
time sqlite3 "$DB" 'CREATE INDEX message_table_index ON message("table")'
time sqlite3 "$DB" 'CREATE INDEX message_row_index ON message("row")'
echo 'Analyzing'
time sqlite3 "$DB" 'ANALYZE'
echo 'Serving...'
"$NANOBOT" serve
