#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
cargo build --locked
cc -std=c11 -Wall -Wextra -Werror -I include tests/ffi_smoke.c \
  -L target/debug -lxiaohong_agent_runtime \
  -Wl,-rpath,"$PWD/target/debug" -o target/debug/m3_ffi_smoke
target/debug/m3_ffi_smoke
