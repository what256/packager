#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

if ! command -v gh >/dev/null 2>&1; then
  echo "GitHub CLI (gh) is required." >&2
  exit 1
fi

if ! gh repo view >/dev/null 2>&1; then
  echo "Add a GitHub remote before configuring repository secrets." >&2
  exit 1
fi

test -f .tauri/packager.key
gh secret set TAURI_SIGNING_PRIVATE_KEY < .tauri/packager.key
/usr/bin/security find-generic-password \
  -s dev.packager.release \
  -a updater-key-password \
  -w | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD

echo "Updater signing secrets configured for $(gh repo view --json nameWithOwner --jq .nameWithOwner)."
echo "Add the Apple, Windows code-signing, and NPM_TOKEN secrets listed in README.md before publishing."
