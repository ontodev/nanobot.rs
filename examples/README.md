# Nanobot Examples

Get a `nanobot` binary
and then run any of these examples from its directory.

### Binary

1. get a `nanobot` binary, either by
  - downloading a [release](https://github.com/ontodev/nanobot.rs/releases)
  - using `cargo build` to build `target/debug/nanobot`
2. make sure that the `nanobot` binary is on your
   [`PATH`](https://opensource.com/article/17/6/set-path-linux)

Then inside the directory for the specific example,
you have two options for running Nanobot:
temporary and persistent.

### Temporary

1. run `nanobot serve --connection :memory:`.
2. open <http://0.0.0.0:3000> in your web browser
3. press `^C` (Control-C) to stop the web server

This will create an "in-memory" SQLite database,
load and validate all the tables,
then start the Nanobot server on your local machine,
so you can work with it in your web browser.
When you stop the Nanobot server (using Control-C),
the in-memory SQLite database will be deleted,
along with all your unsaved changes.
When you run `nanobot serve --connection :memory:` again,
Nanobot will start over with a new in-memory SQLite database.

If you want to keep a SQLite database file
to reuse, view, or modify,
then use the "persistent" approach to running Nanobot.

### Persistent

1. run `nanobot init` to load and validate the tables,
   creating the `nanobot.toml` configuration file
   (if it does not exist)
   and the `.nanobot.db` SQLite database file
2. run `nanobot serve` to start the web server,
3. open <http://0.0.0.0:3000> in your web browser
4. press `^C` (Control-C) to stop the web server
5. delete the `.nanobot.db` file when you are done with it

The persistent approach will create a SQLite database file
that you can work with
while the Nanobot server is running,
or after it has stopped.
If you stop the Nanobot server
and then start it again with `nanobot serve`,
Nanobot will reuse this SQLite database file --
it will not create a new database or reload the TSV files.
To start fresh,
delete the `.nanobot.db` file
and run `nanobot init` again to recreate it.

You can view and modify the `.nanobot.db` SQLite database file
using the `sqlite3` command-line tool,
other command-line tools like [Visidata](https://www.visidata.org),
or GUI applications like [DB Browser for SQLite](https://sqlitebrowser.org).

## Troubleshooting

If you're running into errors,
see if the debugging messages help.
You will want a `nanobot.toml` configuration file.
If it does not exist in the directory,
running `nanobot init` will create it.
You can configure more verbose logging
by adding this to the `nanobot.toml` file:

```toml
[logging]
level = "DEBUG"
```
