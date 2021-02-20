#!/bin/bash
RUSTFLAGS="--emit asm --emit llvm-ir" cargo build --release -Z build-std=core --target atmega32u4.json
# XARGO_RUST_SRC=/home/dennis/workspace/rust.src RUST_TARGET_PATH=`pwd` RUSTFLAGS="--emit asm" xargo build --release -Z build-std=core --target atmega32u4