#!/bin/bash

set -euox pipefail

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

head -c 100 /dev/urandom > "$DIR/random_100"
head -c 463 /dev/urandom > "$DIR/random_463"
head -c 4096 /dev/urandom > "$DIR/random_4096"