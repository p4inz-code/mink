#!/usr/bin/env python3
"""Create the B `csvdata` fixtures.

Run from anywhere:

    python validation/b_csvdata/fixtures/make_fixtures.py
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))

# Columns: id, customer, city, units, price
# Quoted fields contain commas and doubled quotes (RFC 4180).
SALES = (
    b'id,customer,city,units,price\r\n'
    b'1,"Acme, Inc.",Berlin,3,10.50\r\n'
    b'2,"O\'Neil ""Q"" Ltd",Paris,7,2.25\r\n'
    b'3,Globex,Berlin,1,99.90\r\n'
    b'4,"Acme, Inc.",Paris,5,4.00\r\n'
    b'5,Initech,Berlin,2,10.50\r\n'
    b'6,Umbrella,Paris,1,3.50\r\n'
)

# Quoted field with an embedded LF (must survive record splitting).
NOTES = (
    b'key,note\r\n'
    b'a,"first line\nsecond line"\r\n'
    b'b,plain\r\n'
)

# Fewer fields on the second data row -> ragged-row error (exit 4).
RAGGED = (
    b'id,name\r\n'
    b'1,ok\r\n'
    b'2\r\n'
)

# Header only -> no data rows (exit 3).
HEADER_ONLY = b'id,name\r\n'


def main() -> None:
    files = {
        "sales.csv": SALES,
        "notes.csv": NOTES,
        "ragged.csv": RAGGED,
        "header_only.csv": HEADER_ONLY,
    }
    for name, data in files.items():
        with open(os.path.join(HERE, name), "wb") as handle:
            handle.write(data)
    print(f"fixtures: {len(files)} files under {HERE}")


if __name__ == "__main__":
    main()
