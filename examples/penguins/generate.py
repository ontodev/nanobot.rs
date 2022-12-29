#!/usr/bin/env python3
import csv
import math
import random

random.seed(0)


columns = [
    "studyName",
    "Sample Number",
    "Species",
    "Region",
    "Island",
    "Stage",
    "Individual ID",
    "Clutch Completion",
    "Date Egg",
    "Culmen Length (mm)",
    "Culmen Depth (mm)",
    "Flipper Length (mm)",
    "Body Mass (g)",
    "Sex",
    "Delta 15 N (o/oo)",
    "Delta 13 C (o/oo)",
    "Comments",
]

levels = ["error", "warn", "info"]

species = ["Adelie Penguin (Pygoscelis adeliae)"]
regions = ["Anvers"]
islands = ["Biscoe", "Dream", "Torgersen"]
stages = ["Adult, 1 Egg Stage"]
clutch_completions = ["Yes", "No"]
clutch_completion_weights = [90, 10]
sexes = ["MALE", "FEMALE", ""]
sex_weights = [48, 48, 4]


def randdate():
    year = random.randint(2007, 2009)
    month = random.randint(1, 12)
    day = random.randint(1, 30)
    return f"{year}-{month:02}-{day:02}"


def main():
    count = int(1e3)
    error_rate = 0.1
    error_count = math.floor(count * error_rate)
    error_rows = []
    for i in range(1, error_count):
        error_rows.append(random.randint(1, count))
    error_rows.sort()
    error_set = set(error_rows)

    with open("src/data/penguin.tsv", "w") as f:
        w = csv.DictWriter(f, columns, delimiter="\t", lineterminator="\n")
        w.writeheader()
        for i in range(1, count + 1):
            row = {
                "studyName": "FAKE123",
                "Sample Number": i,
                "Species": random.choice(species),
                "Region": random.choice(regions),
                "Island": random.choice(islands),
                "Stage": random.choice(stages),
                "Individual ID": f"N{math.floor(i / 2) + 1}A{i % 2 + 1}",
                "Clutch Completion": random.choices(
                    clutch_completions,
                    weights=clutch_completion_weights
                )[0],
                "Date Egg": randdate(),
                "Culmen Length (mm)": random.randint(300, 500) / 10,
                "Culmen Depth (mm)": random.randint(150, 230) / 10,
                "Flipper Length (mm)": random.randint(160, 230),
                "Body Mass (g)": random.randint(1000, 5000),
                "Sex": random.choices(sexes, weights=sex_weights)[0],
                "Delta 15 N (o/oo)":
                f"{random.randint(700000, 1000000) / 100000:05}",
                "Delta 13 C (o/oo)":
                f"{random.randint(-2700000, -2300000) / 100000:05}",
                "Comments": None,
            }
            if i in error_set:
                row["Sample Number"] = f"{i} foo"
            w.writerow(row)

    fieldnames = ["table", "row", "column", "level", "rule", "value",
                  "message"]
    with open("src/schema/message.tsv", "a") as f:
        w = csv.DictWriter(f, fieldnames, delimiter="\t", lineterminator="\n")
        for i in error_rows:
            row = {
                "table": "penguin",
                "row": i,
                # "column": random.choice(columns),
                # "level": random.choice(levels),
                "column": "Sample Number",
                "level": "error",
                "rule": "test",
                "value": f"{i} foo",
                "message": "Test message",
            }
            w.writerow(row)


if __name__ == "__main__":
    main()
