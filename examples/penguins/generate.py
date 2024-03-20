#!/usr/bin/env python3

import argparse
import csv
import math
import random
import string


# TODO: We should use the column labels instead of the column names.
columns = [
    "study_name",
    "sample_number",
    "species",
    "region",
    "island",
    "stage",
    "individual_id",
    "clutch_completion",
    "date_egg",
    "culmen_length",
    "culmen_depth",
    "flipper_length",
    "body_mass",
    "sex",
    "delta_15_n",
    "delta_13_c",
    "comments",
]

species = ["Adelie Penguin (Pygoscelis adeliae)"]
regions = ["Anvers"]
islands = ["Biscoe", "Dream", "Torgersen"]
stages = ["Adult, 1 Egg Stage"]
clutch_completions = ["Yes", "No"]
clutch_completion_weights = [90, 10]
sexes = ["MALE", "FEMALE"]


def randdate():
    """Return a random date between 2007-01-01 and 2009-12-30."""
    year = random.randint(2007, 2009)
    month = random.randint(1, 12)
    day = random.randint(1, 30)
    return f"{year}-{month:02}-{day:02}"


def add_leading_space(value):
    """Given a random string, add a trailing space character."""
    return ' ' + value


def add_trailing_space(value):
    """Given a random string, add a trailing space character."""
    return value + ' '


def add_space(value):
    """Given a random string, add a random space character
    at a random postion."""
    i = random.randint(0, len(value))
    value = value[0:i] + ' ' + value[i:]
    return value


def add_letter(value):
    """Given a random string, add a random letter character
    at a random postion."""
    i = random.randint(0, len(value))
    value = value[0:i] + random.choice(string.ascii_letters) + value[i:]
    return value


def add_digit(value):
    """Given a random string, add a random digit character
    at a random postion."""
    i = random.randint(0, len(value))
    value = value[0:i] + random.choice(string.digits) + value[i:]
    return value


def add_punctuation(value):
    """Given a random string, add a random punctuation character
    at a random postion."""
    i = random.randint(0, len(value))
    value = value[0:i] + random.choice(string.punctuation) + value[i:]
    return value


def delete_character(value):
    """Given a value string, delete a random character."""
    i = random.randint(0, len(value) - 1)
    value = value[0:i] + value[i+1:]
    return value


def swap_characters(value):
    """Given a value string, swap two random adjacent characters."""
    if len(value) > 1:
        i = random.randint(0, len(value) - 2)
        value = value[0:i] + value[i+1] + value[i] + value[i+2:]
    return value


def swap_case(value):
    """Given a value string, pick a random character,
    and try to swap its case:
    from lower to upper or upper to lower."""
    if len(value) > 0:
        i = random.randint(0, len(value)-1)
        v = value[i]
        if v.islower():
            v = v.upper()
        elif v.isupper():
            v = v.lower()
        value = value[0:i] + v + value[i+1:]
    return value


def delete_value(value):
    """Always return None."""
    return None


error_functions = [
    add_leading_space,
    add_trailing_space,
    add_space,
    add_letter,
    add_digit,
    add_punctuation,
    delete_character,
    swap_characters,
    swap_case,
    delete_value,
]
error_weights = [10, 10, 10, 10, 10, 10, 10, 10, 5, 15]
# error_weights = [0, 0, 0, 0, 0, 0, 0, 0, 100, 0]
assert len(error_functions) == len(error_weights)
assert sum(error_weights) == 100


def scramble(value):
    """Given a value, pick a random error function,
    and return the result of applying that function to the value."""
    value = str(value)
    f = random.choices(error_functions, weights=error_weights)[0]
    return f(value)


def generate_row(index, error_columns):
    """Given a row index and a list of columns with errors,
    return a randomly generated row
    with randomly generated errors in those columns."""
    n = math.floor(index / 2) + 1
    a = index % 2 + 1
    row = {
        "study_name": "FAKE123",
        "sample_number": index,
        "species": random.choice(species),
        "region": random.choice(regions),
        "island": random.choice(islands),
        "stage": random.choice(stages),
        "individual_id": f"N{n}A{a}",
        "clutch_completion": random.choices(
            clutch_completions,
            weights=clutch_completion_weights
        )[0],
        "date_egg": randdate(),
        "culmen_length": random.randint(300, 500) / 10,
        "culmen_depth": random.randint(150, 230) / 10,
        "flipper_length": random.randint(160, 230),
        "body_mass": random.randint(1000, 5000),
        "sex": random.choice(sexes),
        "delta_15_n": f"{random.randint(700000, 1000000) / 100000:05}",
        "delta_13_c": f"{random.randint(-2700000, -2300000) / 100000:05}",
        "comments": None,
    }
    for column in error_columns:
        row[column] = scramble(row[column])
    return row


def generate_table(path, count=1000, rate=10, seed=0):
    """Given a path, a row count, an error rate percentage, and a random seed,
    generate rows of random data with errors at that rate,
    and write the table to that path in TSV format."""
    random.seed(seed)

    if count < 1:
        raise Exception('count must be greater than zero, but was "{count}"')
    if rate < 0 or rate > 100:
        raise Exception('rate must be between 0 and 100, but was "{rate}"')

    error_rate = rate / 100
    error_count = math.floor(count * error_rate)
    error_rows = []
    for i in range(1, error_count):
        error_rows.append(random.randint(1, count))
    error_rows.sort()
    error_set = set(error_rows)

    with open(path, "w") as f:
        w = csv.DictWriter(f, columns, delimiter="\t", lineterminator="\n")
        w.writeheader()
        for i in range(1, count + 1):
            error_columns = []
            if i in error_set:
                error_columns = [random.choice(columns)]
            row = generate_row(i, error_columns)
            w.writerow(row)


def main():
    parser = argparse.ArgumentParser(description="{{ DESCRIPTION }}")
    parser.add_argument("path", type=str, help="The output file path")
    parser.add_argument("count", type=int, default=1000,
                        nargs="?", help="The number of rows [1000]")
    parser.add_argument("rate", type=int, default=10, nargs="?",
                        help="The percentage of rows with errors [10]")
    parser.add_argument("seed", type=int, nargs="?",
                        default=0, help="The random seed [0]")

    args = parser.parse_args()

    generate_table(args.path, args.count, args.rate, args.seed)


if __name__ == "__main__":
    main()
