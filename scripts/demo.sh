#!/usr/bin/env bash
# Build the client and the example service, then run the demo.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
[ -f "$ROOT/target/seeds-register-service.jam" ] || bash "$ROOT/scripts/build-service.sh"
cd "$ROOT" && cargo run --release -- --service target/seeds-register-service.jam "$@"
