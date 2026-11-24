#!/usr/bin/env bash
set -euo pipefail
arch="${1:-}"
case "$arch" in
  arm64-v8a) target=aarch64-unknown-linux-ohos ;;
  x86_64) target=x86_64-unknown-linux-ohos ;;
  *) echo 'usage: build-ohos.sh <arm64-v8a|x86_64>' >&2; exit 2 ;;
esac
: "${OHOS_NDK_HOME:?set OHOS_NDK_HOME to an installed OpenHarmony SDK/NDK}"
command -v rustup >/dev/null || { echo 'rustup not found' >&2; exit 1; }
command -v cmake >/dev/null || { echo 'cmake not found' >&2; exit 1; }
rustup target list --installed | grep -qx "$target" || { echo "Install Rust target first: rustup target add $target" >&2; exit 1; }
root="$(cd "$(dirname "$0")/.." && pwd)"
build="${root}/build/ohos-${arch}"
cmake -S "$root" -B "$build" \
  -DCMAKE_TOOLCHAIN_FILE="$OHOS_NDK_HOME/native/build/cmake/ohos.toolchain.cmake" \
  -DOHOS_ARCH="$arch" -DRUST_TARGET="$target" -DCMAKE_BUILD_TYPE=Release
cmake --build "$build" --parallel
