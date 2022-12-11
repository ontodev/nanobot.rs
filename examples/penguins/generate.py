#!/usr/bin/env python3
import csv
import math
import random

random.seed(0)


def main():
    count = 1000000
    error_rate = 0.1
    error_count = math.floor(count * error_rate)
    error_rows = []
    for i in range(1, error_count):
        error_rows.append(random.randint(1, count))
    error_rows.sort()

    fieldnames = ["subject", "mass (g)"]
    with open("src/data/penguin.tsv", "w") as f:
        w = csv.DictWriter(f, fieldnames, delimiter="\t", lineterminator="\n")
        w.writeheader()
        for i in range(1, count + 1):
            row = {
                "subject": f"N{i}A1",
                "mass (g)": random.randint(1000, 5000),
            }
            w.writerow(row)

    fieldnames = ["table", "row", "column", "level", "rule", "message"]
    with open("src/schema/message.tsv", "a") as f:
        w = csv.DictWriter(f, fieldnames, delimiter="\t", lineterminator="\n")
        for i in error_rows:
            row = {
                "table": "penguin",
                "row": i,
                "column": "subject",
                "level": "error",
                "rule": "test",
                "message": "Test message",
            }
            w.writerow(row)


if __name__ == "__main__":
    main()
