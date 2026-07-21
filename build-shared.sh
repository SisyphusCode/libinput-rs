#!/usr/bin/env bash
set -euo pipefail

cargo build --lib --release --locked

cc -shared \
  -Wl,--whole-archive target/release/libinput.a -Wl,--no-whole-archive \
  -Wl,--version-script=libinput.map \
  -Wl,-soname,libinput.so.0 \
  -ldl -lm -lpthread -lrt \
  -o target/release/libinput.so
