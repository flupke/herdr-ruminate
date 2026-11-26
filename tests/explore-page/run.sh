#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")"

if [ -z "${E2E_TELEMETRY_DISABLED:-}" ]; then
  echo 'Run this inside the Nix dev shell, which turns e2e telemetry off: scripts/dev-shell make ...' >&2
  exit 2
fi

rm -f .e2e/logs/*.log
node_modules/.bin/e2e run "$@"
status=$?
if grep -h 'csp violation' .e2e/logs/*.log 2>/dev/null; then
  echo 'The Explore page broke its content security policy'
  status=1
fi
if [ "$status" -ne 0 ]; then tail -n 20 .e2e/logs/*.log 2>/dev/null; fi
exit "$status"
