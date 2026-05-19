#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust/Cargo is required for the native desktop pet."
  echo "Install Rust, then run ./run.sh again."
  exit 1
fi

binary="rust/native-pet/target/release/native-pet"
needs_build=0

if [ ! -x "$binary" ]; then
  needs_build=1
elif [ rust/native-pet/Cargo.toml -nt "$binary" ]; then
  needs_build=1
elif find rust/native-pet/src -type f -newer "$binary" | grep -q .; then
  needs_build=1
fi

if [ "$needs_build" -eq 0 ]; then
  exec "$binary" "$@"
fi

cargo build --release --manifest-path rust/native-pet/Cargo.toml
exec "$binary" "$@"
