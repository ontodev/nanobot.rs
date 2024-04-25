# Nanobot Configuration

You can configure Nanobot with the `nanobot.toml` file.
See <https://toml.io> for details on the syntax.

## Default Configuration

```toml
[nanobot]
config_version = 1
port = 3000
results_per_page = 20
```

## Full Configuration

```toml
[nanobot]
config_version = 1
port = 3000
results_per_page = 20

[logging]
level = "DEBUG" # ERROR, WARN, INFO (default), DEBUG

[database]
# Database connection string: SQLite file or Postgres URL.
connection = ".nanobot.db"

[valve]
# Path to the VALVE 'table' table.
path = "src/schema/table.tsv"

[assets]
# Path to a directory of static files to serve under <http://localhost:PORT/assets/>.
path = "assets/"

[templates]
# Path to a directory of [Minijinja](https://github.com/mitsuhiko/minijinja) templates.
path = "src/templates/"

# Entries for the "Actions" menu.
# `actions` is a TOML dictionary
# Each action requires a `label` and `command`.
[actions.status]
label = "Status"
command = "git status"

[actions.fetch]
label = "Fetch"
command = "git fetch"

[actions.pull]
label = "Pull"
command = "git pull"

[actions.branch]
label = "Branch"
# Actions can request inputs using an HTML form.
input = [
  { name = "branch_name", label = "Branch Name", default = "{username}-{number}", validate = "\\w+" }
]
# Actions can use a single command or a list of commands.
# Inputs can be used in commands with `{variable}` syntax.
commands = [
  "git checkout main",
  "git pull",
  "git checkout --branch {branch_name}",
  "git push --set-upstream origin {branch_name}",
]
```
