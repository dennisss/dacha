#!/bin/bash

set -euox pipefail

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

mkdir -p "$DIR/derived"
gzip -k -c -1 "$DIR/gutenberg/shakespeare.txt" > "$DIR/derived/shakespeare.txt.1.gz"
gzip -k -c -2 "$DIR/gutenberg/shakespeare.txt" > "$DIR/derived/shakespeare.txt.2.gz"
gzip -k -c -4 "$DIR/gutenberg/shakespeare.txt" > "$DIR/derived/shakespeare.txt.4.gz"
gzip -k -c -9 "$DIR/gutenberg/shakespeare.txt" > "$DIR/derived/shakespeare.txt.9.gz"

gzip -k -c -5 "$DIR/random/random_100" > "$DIR/derived/random_100.5.gz"
gzip -k -c -5 "$DIR/random/random_463" > "$DIR/derived/random_463.5.gz"
gzip -k -c -5 "$DIR/random/random_4096" > "$DIR/derived/random_4096.5.gz"