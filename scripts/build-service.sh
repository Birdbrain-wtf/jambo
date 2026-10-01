#!/usr/bin/env bash
# Build a service under services/ into target/<name>.jam with Parity's jam-pvm-build.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
NAME=${1:-seeds-register}
command -v jam-pvm-build >/dev/null || cargo install jam-pvm-build --version 0.1.28
rustup toolchain install nightly-2025-05-10 --profile minimal --component rust-src >/dev/null
mkdir -p "$ROOT/target"
cd "$ROOT/services/$NAME" && jam-pvm-build -m service -o "$ROOT/target/"
ls "$ROOT/target/"*.jam
