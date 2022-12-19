#!/bin/sh -e

NANOBOT="$(pwd)/target/release/nanobot"
DB=".nanobot.db"

rm -rf temp
mkdir temp/
cp -r examples/penguins/* temp/
cd temp
mkdir -p src/data
echo 'Generating random data...'
time python3 generate_data.py
echo 'Initializing Nanobot...'
time "$NANOBOT" init
# tree -a
# sqlite3 "$DB" "SELECT COUNT() FROM penguin"
echo 'Adding views...'
sqlite3 "$DB" < static_views.sql
python3 generate_views.py > generated_views.sql
sqlite3 "$DB" < generated_views.sql
# sqlite3 "$DB" 'SELECT * FROM "penguin_text" LIMIT 1'
echo 'Indexing...'
time sqlite3 "$DB" 'CREATE INDEX message_table_index ON message("table")'
time sqlite3 "$DB" 'CREATE INDEX message_row_index ON message("row")'
echo 'Analyzing'
time sqlite3 "$DB" 'ANALYZE'
echo 'Serving...'
"$NANOBOT" serve
