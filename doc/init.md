# `init`: Initialize a Nanobot Project

You can initialize a new Nanobot project like so:

```shell-session tesh-session="init"
$ mkdir new_project
$ cd new_project
$ nanobot init
Initialized a Nanobot project
```

Nanobot initializes a new project in these steps:

- create `nanobot.toml` config file
- create `.nanobot.db` database file
  - add `.nanobot.db` to `.gitignore`
- create `src/schema/` directory
  - create meta tables: table, column, datatype

You can check that this is the case using the `tree` utility:

```shell-session tesh-session="init"
$ tree
├── nanobot.toml
├── .nanobot.db
└── src
    └── schema
        ├── column.tsv
        ├── datatype.tsv
        └── table.tsv
```