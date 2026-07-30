#!/usr/bin/env bash
set -euo pipefail

bump="${1:-patch}"
manifest="${2:-Cargo.toml}"

case "$bump" in
  major|minor|patch) ;;
  *)
    echo "usage: $0 [major|minor|patch] [Cargo.toml]" >&2
    exit 2
    ;;
esac

package_name="$({
  awk '
    $0 == "[package]" { in_package = 1; next }
    in_package && /^\[/ { exit }
    in_package && /^name = "/ {
      value = $0
      sub(/^name = "/, "", value)
      sub(/"$/, "", value)
      print value
      exit
    }
  ' "$manifest"
})"
current="$({
  awk '
    $0 == "[package]" { in_package = 1; next }
    in_package && /^\[/ { exit }
    in_package && /^version = "/ {
      value = $0
      sub(/^version = "/, "", value)
      sub(/"$/, "", value)
      print value
      exit
    }
  ' "$manifest"
})"

if [[ -z "$package_name" ]]; then
  echo "Cargo.toml is missing a package name" >&2
  exit 1
fi
if [[ ! "$current" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
  echo "package version must be stable SemVer, got: $current" >&2
  exit 1
fi

major="${BASH_REMATCH[1]}"
minor="${BASH_REMATCH[2]}"
patch="${BASH_REMATCH[3]}"
case "$bump" in
  major)
    major=$((major + 1)); minor=0; patch=0
    ;;
  minor)
    minor=$((minor + 1)); patch=0
    ;;
  patch)
    patch=$((patch + 1))
    ;;
esac
version="$major.$minor.$patch"

lockfile="$(dirname "$manifest")/Cargo.lock"
manifest_temporary="$(mktemp "${manifest}.tmp.XXXXXX")"
lockfile_temporary="$(mktemp "${lockfile}.tmp.XXXXXX")"
trap 'rm -f "$manifest_temporary" "$lockfile_temporary"' EXIT
awk -v version="$version" '
  $0 == "[package]" { in_package = 1 }
  in_package && !updated && /^version = "/ {
    print "version = \"" version "\""
    updated = 1
    next
  }
  { print }
  END { if (!updated) exit 1 }
' "$manifest" > "$manifest_temporary"
awk -v package_name="$package_name" -v current="$current" -v version="$version" '
  $0 == "[[package]]" { in_package = 1; target = 0 }
  in_package && $0 == "name = \"" package_name "\"" { target = 1 }
  target && $0 == "version = \"" current "\"" {
    print "version = \"" version "\""
    updated += 1
    target = 0
    next
  }
  { print }
  END { if (updated != 1) exit 1 }
' "$lockfile" > "$lockfile_temporary"
cat "$manifest_temporary" > "$manifest"
cat "$lockfile_temporary" > "$lockfile"
rm -f "$manifest_temporary" "$lockfile_temporary"
trap - EXIT

cargo metadata --manifest-path "$manifest" --locked --no-deps --format-version 1 > /dev/null
printf '%s\n' "$version"
