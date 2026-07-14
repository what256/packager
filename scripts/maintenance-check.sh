#!/bin/sh

set -u

cd "$(dirname "$0")/.."

full=false
case "${1:-}" in
  "") ;;
  --full) full=true ;;
  *)
    echo "Usage: $0 [--full]" >&2
    exit 2
    ;;
esac

failures=0
warnings=0

heading() {
  printf '\n%s\n' "$1"
}

pass() {
  printf 'PASS: %s\n' "$1"
}

warn() {
  warnings=$((warnings + 1))
  printf 'WARN: %s\n' "$1" >&2
}

fail() {
  failures=$((failures + 1))
  printf 'FAIL: %s\n' "$1" >&2
}

run_required() {
  label=$1
  shift
  if "$@"; then
    pass "$label"
  else
    fail "$label"
  fi
}

heading "Prerequisites"
for command in git node npm cargo jq; do
  if command -v "$command" >/dev/null 2>&1; then
    pass "$command is available"
  else
    fail "$command is required"
  fi
done
if [ "$failures" -ne 0 ]; then
  exit 1
fi

heading "Repository"
branch=$(git branch --show-current 2>/dev/null || true)
if [ "$branch" = main ]; then
  pass "current branch is main"
else
  warn "current branch is '${branch:-detached}', not main"
fi
if [ -z "$(git status --short)" ]; then
  pass "worktree is clean"
else
  warn "worktree has local changes; review git status before pulling or releasing"
  git status --short
fi

heading "JavaScript security and updates"
run_required "npm reports no vulnerability at the configured threshold" npm audit --audit-level=high
outdated=$(npm outdated 2>&1)
outdated_status=$?
case "$outdated_status" in
  0) pass "npm dependencies are within their configured ranges" ;;
  1)
    warn "newer npm dependency versions are available; review them in a tested change"
    printf '%s\n' "$outdated"
    ;;
  *)
    fail "npm outdated could not query the registry"
    printf '%s\n' "$outdated" >&2
    ;;
esac

heading "Rust security"
if cargo audit --version >/dev/null 2>&1; then
  audit_json=$(mktemp "${TMPDIR:-/tmp}/packager-cargo-audit.XXXXXX")
  if cargo audit --json >"$audit_json"; then
    vulnerabilities=$(jq '.vulnerabilities.list | length' "$audit_json")
    advisory_warnings=$(jq '[.warnings[]] | flatten | length' "$audit_json")
    if [ "$vulnerabilities" -eq 0 ]; then
      pass "RustSec reports no known vulnerabilities"
    else
      fail "RustSec reports $vulnerabilities known vulnerability finding(s)"
    fi
    if [ "$advisory_warnings" -gt 0 ]; then
      warn "RustSec reports $advisory_warnings non-vulnerability advisory warning(s); review docs/STATUS.md"
      jq -r '.warnings | to_entries[] | "  \(.key): \(.value | length)"' "$audit_json"
    fi
  else
    fail "RustSec audit could not complete"
  fi
  rm -f "$audit_json"
else
  warn "cargo-audit is not installed; run: cargo install cargo-audit --locked"
fi

heading "Published package and GitHub"
source_version=$(node -p "require('./package.json').version")
registry_version=$(npm view @what256/packager version 2>/dev/null || true)
if [ "$registry_version" = "$source_version" ]; then
  pass "npm @what256/packager matches source version $source_version"
else
  warn "npm version '${registry_version:-unavailable}' differs from source version $source_version"
fi

if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
  for workflow in ci.yml runtime-assets.yml publish-npm.yml package-manager-channels.yml; do
    gh run list --repo what256/packager --workflow "$workflow" --branch main --limit 1
  done
  security=$(gh api repos/what256/packager --jq '.security_and_analysis' 2>/dev/null || true)
  if [ -n "$security" ]; then
    printf '%s\n' "$security"
  else
    warn "GitHub security settings could not be read"
  fi
else
  warn "GitHub CLI is unavailable or not authenticated"
fi

if [ "$full" = true ]; then
  heading "Full local verification"
  run_required "TypeScript check" npm run check
  run_required "frontend build" npm run build
  run_required "Rust formatting" cargo fmt --all -- --check
  run_required "Rust Clippy" cargo clippy --workspace --all-targets --all-features -- -D warnings
  run_required "Rust tests" cargo test --workspace
fi

heading "Summary"
printf '%s failure(s), %s warning(s)\n' "$failures" "$warnings"
if [ "$failures" -ne 0 ]; then
  exit 1
fi
