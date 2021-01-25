#!/bin/bash
RUSTFLAGS="--emit asm" cargo build --release -Z build-std=core --target atmega32u4.json