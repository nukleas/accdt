#!/bin/sh
# Downloads Microsoft's Northwind 2.0 Developer Edition template for the integration test.
set -e
cd "$(dirname "$0")/.."
mkdir -p tests/fixtures
curl -fsSL -o tests/fixtures/northwind-2.0-dev.accdt \
  "https://binaries.templates.cdn.office.net/support/templates/en-us/tf22238896_win32.accdt"
ls -la tests/fixtures/northwind-2.0-dev.accdt
