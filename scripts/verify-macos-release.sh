#!/bin/sh
set -eu

target=${1:?target is required}
identity=${2:?Developer ID identity is required}
bundle_root="target/$target/release/bundle"
macos_root="$bundle_root/macos"
dmg_root="$bundle_root/dmg"

app_count=$(find "$macos_root" -maxdepth 1 -type d -name '*.app' | wc -l | tr -d ' ')
dmg_count=$(find "$dmg_root" -maxdepth 1 -type f -name '*.dmg' | wc -l | tr -d ' ')
test "$app_count" -eq 1
test "$dmg_count" -eq 1

app=$(find "$macos_root" -maxdepth 1 -type d -name '*.app' -print -quit)
dmg=$(find "$dmg_root" -maxdepth 1 -type f -name '*.dmg' -print -quit)
updater=$(find "$macos_root" -maxdepth 1 -type f -name '*.app.tar.gz' -print -quit)
test -n "$updater"
test -s "$updater"
test -s "$updater.sig"

/usr/bin/codesign --verify --deep --strict --verbose=2 "$app"
details=$(/usr/bin/codesign --display --verbose=4 "$app" 2>&1)
printf '%s\n' "$details" | grep -F "Authority=$identity" >/dev/null
/usr/sbin/spctl --assess --type execute --verbose=4 "$app"
/usr/bin/xcrun stapler validate "$app"
/usr/bin/xcrun stapler validate "$dmg"

printf '{"verified":true,"target":"%s","app":"%s","dmg":"%s","updater":"%s"}\n' \
  "$target" "$(basename "$app")" "$(basename "$dmg")" "$(basename "$updater")"
