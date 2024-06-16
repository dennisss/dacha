# GCode Parsing/Serialization

Library for parsing GCode programs.

For the most part this follows the [NIST RS274NGC specification](https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=823374) with the addition of string value and ';' comment support as used in many open source firmwares.

Some interesting notes on syntax:

- Per the NIST spec, whitespace and upper/lower case is ignored outside of comments.
- Comments can't be present between work keys and values. e.g. `X (hello) 1` is invalid.
 