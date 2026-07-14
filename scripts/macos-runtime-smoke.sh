#!/usr/bin/env bash

set -euo pipefail

PACKAGER="${PACKAGER:-packager}"
IMAGE="${SMOKE_IMAGE:-docker.io/library/nginx:alpine}"
TIMEOUT_SECONDS="${SMOKE_TIMEOUT_SECONDS:-600}"
EVIDENCE_PATH="${EVIDENCE_PATH:-}"
KEEP_GENERATED_PACKAGE="${KEEP_GENERATED_PACKAGE:-false}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "The managed-runtime smoke test must run on macOS." >&2
  exit 1
fi
if (( ${#IMAGE} > 300 )) || ! [[ "$IMAGE" =~ ^[A-Za-z0-9][-A-Za-z0-9._/@:]*$ ]]; then
  echo "Image must be a registry reference without spaces or shell metacharacters." >&2
  exit 1
fi
if ! [[ "$TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || (( TIMEOUT_SECONDS < 60 || TIMEOUT_SECONDS > 3600 )); then
  echo "SMOKE_TIMEOUT_SECONDS must be between 60 and 3600." >&2
  exit 1
fi
for command in jq curl; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "$command is required." >&2
    exit 1
  fi
done
if [[ "$PACKAGER" == */* ]]; then
  PACKAGER="$(cd "$(dirname "$PACKAGER")" && pwd)/$(basename "$PACKAGER")"
fi

created_sandbox=false
if [[ -z "${PACKAGER_DATA_DIR:-}" ]]; then
  sandbox="$(mktemp -d "${TMPDIR:-/tmp}/packager-macos-smoke.XXXXXX")"
  export PACKAGER_DATA_DIR="$sandbox/data"
  export PACKAGER_CACHE_DIR="$sandbox/cache"
  created_sandbox=true
else
  sandbox=""
  export PACKAGER_CACHE_DIR="${PACKAGER_CACHE_DIR:-$PACKAGER_DATA_DIR/cache}"
fi

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
id="packager-macos-smoke-$(date -u +%Y%m%d%H%M%S)-$$"
source_package="$(mktemp -d "${TMPDIR:-/tmp}/${id}-source.XXXXXX")"
generated_package=""
installed=false
runtime_was_running=false
runtime_was_installed=false

packager_json() {
  local output
  if ! output="$("$PACKAGER" --json "$@" 2>&1)"; then
    echo "packager $* failed: $output" >&2
    return 1
  fi
  if ! jq -e . >/dev/null 2>&1 <<<"$output"; then
    echo "packager $* returned invalid JSON: $output" >&2
    return 1
  fi
  printf '%s\n' "$output"
}

packager_quiet() {
  "$PACKAGER" "$@" >/dev/null 2>&1 || true
}

packager_cleanup() {
  local output
  if ! output="$("$PACKAGER" "$@" 2>&1)"; then
    echo "Cleanup failed: packager $*: $output" >&2
    return 1
  fi
}

get_smoke_app() {
  packager_json apps | jq -c --arg id "$id" '.[] | select(.id == $id)'
}

wait_ready() {
  local deadline app status
  deadline=$(( $(date +%s) + TIMEOUT_SECONDS ))
  while (( $(date +%s) < deadline )); do
    app="$(get_smoke_app)"
    if [[ -n "$app" ]] && [[ "$(jq -r '.status' <<<"$app")" == "ready" ]]; then
      printf '%s\n' "$app"
      return 0
    fi
    sleep 2
  done
  app="$(get_smoke_app)"
  status="missing"
  if [[ -n "$app" ]]; then
    status="$(jq -r '.status' <<<"$app")"
  fi
  echo "Packaged workload did not become ready before the timeout (last status: $status)." >&2
  return 1
}

cleanup() {
  local exit_code=$?
  if [[ "$installed" == true ]]; then
    if (( exit_code == 0 )); then
      packager_cleanup stop "$id" || exit_code=1
      packager_cleanup uninstall "$id" --delete-data || exit_code=1
    else
      packager_quiet stop "$id"
      packager_quiet uninstall "$id" --delete-data
    fi
  fi
  if [[ "$KEEP_GENERATED_PACKAGE" != true ]] && [[ -n "$generated_package" ]]; then
    rm -rf "$generated_package"
  fi
  rm -rf "$source_package"
  if [[ "$runtime_was_installed" != true ]]; then
    if (( exit_code == 0 )); then
      packager_cleanup runtime uninstall || exit_code=1
    else
      packager_quiet runtime uninstall
    fi
  elif [[ "$runtime_was_running" != true ]]; then
    if (( exit_code == 0 )); then
      packager_cleanup runtime stop || exit_code=1
    else
      packager_quiet runtime stop
    fi
  fi
  if [[ "$created_sandbox" == true ]] && (( exit_code == 0 )); then
    rm -rf "$sandbox"
  elif [[ "$created_sandbox" == true ]]; then
    echo "Preserved failed smoke-test sandbox: $sandbox" >&2
  fi
  exit "$exit_code"
}
trap cleanup EXIT

version="$("$PACKAGER" --version)"
initial_runtime="$(packager_json runtime status)"
runtime_was_running="$(jq -r '.running' <<<"$initial_runtime")"
runtime_was_installed="$(jq -r '.installed' <<<"$initial_runtime")"
system="$(packager_json status)"
apps_root="$(jq -r '.appDataDir' <<<"$system")"
data_root="$(dirname "$apps_root")"
generated_package="$data_root/created-packages/$id"

cat >"$source_package/compose.yml" <<EOF
services:
  web:
    image: $IMAGE
    ports:
      - "80"
    volumes:
      - "\${PACKAGER_DATA_DIR:?PACKAGER_DATA_DIR is required}/html:/usr/share/nginx/html:ro"
    restart: unless-stopped
EOF

built="$(packager_json build compose "$source_package" \
  --id "$id" \
  --name "Packager macOS Smoke" \
  --description "Disposable end-to-end Packager runtime validation workload." \
  --homepage "https://nginx.org" \
  --port 80)"
[[ "$(jq -r '.id' <<<"$built")" == "$id" ]]
[[ "$(jq -r '.status' <<<"$built")" == "stopped" ]]
installed=true

content_marker="Packager macOS persistent data $id"
html_directory="$data_root/apps/$id/data/html"
mkdir -p "$html_directory"
printf '%s\n' "$content_marker" >"$html_directory/index.html"

started="$(packager_json start "$id")"
[[ "$(jq -r '.id' <<<"$started")" == "$id" ]]
[[ "$(jq -r '.status' <<<"$started")" =~ ^(starting|ready)$ ]]
ready="$(wait_ready)"
workload_url="$(jq -r '.url' <<<"$ready")"
response="$(curl --fail --silent --show-error --noproxy '*' --max-time 20 "$workload_url")"
grep -Fq "$content_marker" <<<"$response"

logs="$(packager_json logs "$id" --lines 100 | jq -r '.')"
grep -Fq "GET / HTTP" <<<"$logs"

packager_json auto-updates disable "$id" >/dev/null
disabled_app="$(get_smoke_app)"
[[ "$(jq -r '.automaticUpdates' <<<"$disabled_app")" == "false" ]]
packager_json auto-updates enable "$id" >/dev/null
enabled_app="$(get_smoke_app)"
[[ "$(jq -r '.automaticUpdates' <<<"$enabled_app")" == "true" ]]

updated="$(packager_json update "$id")"
[[ "$(jq -r '.id' <<<"$updated")" == "$id" ]]
ready_after_update="$(wait_ready)"
updated_url="$(jq -r '.url' <<<"$ready_after_update")"
updated_response="$(curl --fail --silent --show-error --noproxy '*' --max-time 20 "$updated_url")"
grep -Fq "$content_marker" <<<"$updated_response"

stopped="$(packager_json stop "$id")"
[[ "$(jq -r '.status' <<<"$stopped")" == "stopped" ]]
stopped_app="$(get_smoke_app)"
[[ "$(jq -r '.status' <<<"$stopped_app")" == "stopped" ]]

uninstalled="$(packager_json uninstall "$id" --delete-data)"
[[ "$(jq -r '.status' <<<"$uninstalled")" == "not-installed" ]]
installed=false
if [[ -n "$(get_smoke_app)" ]]; then
  echo "The disposable workload remained installed after cleanup." >&2
  exit 1
fi

runtime="$(packager_json runtime status)"
evidence="$(jq -n \
  --arg startedAt "$started_at" \
  --arg completedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg hostArchitecture "$(uname -m)" \
  --arg macosVersion "$(sw_vers -productVersion)" \
  --arg packagerVersion "$version" \
  --arg image "$IMAGE" \
  --arg runtimeVersion "$(jq -r '.version' <<<"$runtime")" \
  --arg workloadUrl "$workload_url" \
  '{
    schemaVersion: 1,
    passed: true,
    startedAt: $startedAt,
    completedAt: $completedAt,
    hostArchitecture: $hostArchitecture,
    macosVersion: $macosVersion,
    packagerVersion: $packagerVersion,
    image: $image,
    runtimeVersion: $runtimeVersion,
    workloadUrl: $workloadUrl,
    checks: [
      "package-build-import",
      "managed-runtime-install-start",
      "compose-pull-up",
      "macos-bind-data-persistence",
      "loopback-http-readiness",
      "container-logs",
      "automatic-update-state",
      "image-update-recreate",
      "stop",
      "destructive-uninstall"
    ]
  }')"

if [[ -n "$EVIDENCE_PATH" ]]; then
  mkdir -p "$(dirname "$EVIDENCE_PATH")"
  printf '%s\n' "$evidence" >"$EVIDENCE_PATH"
fi
printf '%s\n' "$evidence"
