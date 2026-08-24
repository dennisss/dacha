#!/bin/bash

set -euo pipefail

PI_GEN_DIR="third_party/pi-gen"

cd $PI_GEN_DIR

docker build --no-cache -t pi-gen-base:latest ./docker-base
docker save pi-gen-base:latest | gzip > ${PI_GEN_DIR}/deploy/${IMG_DATE}-pi-gen-base.tar.gz