import csv
from collections import defaultdict


def main():
    tables = defaultdict(list)
    nulltypes = {}
    with open("src/schema/column.tsv") as f:
        rows = csv.DictReader(f, delimiter="\t")
        for row in rows:
            tables[row["table"]].append(row)
            if row["nulltype"].strip() != "":
                nulltypes[row["nulltype"]] = ""

    with open("src/schema/datatype.tsv") as f:
        rows = csv.DictReader(f, delimiter="\t")
        for row in rows:
            if row["datatype"] not in nulltypes:
                continue
            condition = row["condition"]
            if not condition.startswith("equals"):
                message = "Nulltypes can only use 'equal', "
                message += f"but found '{condition}'"
                raise Exception(message)
            condition = condition[8:-2]
            nulltypes[row["datatype"]] = condition

    # union table
    for table in tables.keys():
        ddl = f"""DROP VIEW IF EXISTS "{table}_union";
CREATE VIEW "{table}_union" AS
SELECT * FROM "{table}"
UNION ALL
SELECT * FROM "{table}_conflict";
"""
        print(ddl)

    # text table
    for table, rows in tables.items():
        selects = ['"row_number"']
        joins = []
        for row in rows:
            column = row["column"]
            line = f"""COALESCE(
    CAST("{table}_union"."{column}" AS TEXT),
    "{column} cell"."value"
  ) AS "{column}" """
            selects.append(line.strip())
            line = f"""LEFT JOIN message_cell AS "{column} cell"
       ON "{table}_union".row_number = "{column} cell"."row"
      AND "{column} cell"."table" = '{table}'
      AND "{column} cell"."column" = '{column}' """
            joins.append(line.strip())
        select = ",\n  ".join(selects)
        join = "\n".join(joins)
        ddl = f"""DROP VIEW IF EXISTS "{table}_text";
CREATE VIEW "{table}_text" AS
SELECT
  {select}
FROM "{table}_union"
{join};
"""
        print(ddl)

    # values table
    for table, rows in tables.items():
        selects = ["""'_row_number', "row_number" """.strip()]
        joins = []
        for row in rows:
            column = row["column"]
            line = f"""'{column}', "{column}" """
            selects.append(line.strip())
        select = ",\n    ".join(selects)
        ddl = f"""DROP VIEW IF EXISTS "{table}_values";
CREATE VIEW "{table}_values" AS
SELECT *,
  json_object(
    {select}
  ) AS json_result
FROM "{table}_union";
"""
        print(ddl)

    # cells table
    for table, rows in tables.items():
        selects = ["""'row_number',
    json_object(
      'value', "row_number",
      'datatype', 'integer'
    )"""]
        joins = []
        for row in rows:
            column = row["column"]
            nulltype = row["nulltype"]
            datatype = row["datatype"]

            nullwhen = f"""WHEN "{column}" IS NULL THEN json_object(
          'value', null,
          'valid', json('false'),
          'text', "{column} cell".value
        )"""
            if nulltype.strip() != "":
                nulltext = nulltypes[nulltype]
                nullwhen = f"""WHEN "{column}" IS NULL THEN json_object(
          'value', null,
          'nulltype', '{nulltype}',
          'text', '{nulltext}'
        )"""

            line = f"""'{column}',
    json_patch(
      CASE
        {nullwhen}
        ELSE json_object(
          'value', "{column}",
          'datatype', '{datatype}'
        )
      END,
      json_object(
        'message_level',
        (SELECT level FROM levels WHERE severity = "{column} cell".severity),
        'messages',
        json("{column} cell".messages)
      )
    )"""
            selects.append(line.strip())
            line = f"""LEFT JOIN message_cell AS "{column} cell"
       ON "{table}_union".row_number = "{column} cell"."row"
      AND "{column} cell"."table" = '{table}'
      AND "{column} cell"."column" = '{column}' """
            joins.append(line.strip())
        select = ",\n    ".join(selects)
        join = "\n".join(joins)
        ddl = f"""DROP VIEW IF EXISTS "{table}_cells";
CREATE VIEW "{table}_cells" AS
SELECT "{table}_union".*,
  json_object(
    {select}
  ) AS json_result
FROM "{table}_union"
{join};
"""
        print(ddl)


if __name__ == "__main__":
    main()
