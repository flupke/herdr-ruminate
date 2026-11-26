#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
if [ -z "${E2E_TELEMETRY_DISABLED:-}" ]; then
  exec "$here/../../scripts/dev-shell" "$here/mcp.sh" "$@"
fi
if [ ! -x "$here/node_modules/.bin/e2e" ]; then
  echo 'e2e is not installed: run `scripts/dev-shell make e2e-explore-deps` once.' >&2
  exit 1
fi
exec "$here/node_modules/.bin/e2e" mcp --config "$here/e2e.config.ts" --headless "$@"
