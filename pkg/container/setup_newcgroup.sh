#!/bin/bash

set -euo pipefail

sudo chown root:$USER built/pkg/container/newcgroup

sudo chmod 750 built/pkg/container/newcgroup

sudo chmod u+s built/pkg/container/newcgroup
