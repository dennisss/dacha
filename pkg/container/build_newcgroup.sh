#!/bin/bash

set -euo pipefail

cargo build --bin newcgroup

rm -f bin/newcgroup

cp target/debug/newcgroup bin/newcgroup

sudo chown root:$USER bin/newcgroup

sudo chmod 750 bin/newcgroup

sudo chmod u+s bin/newcgroup