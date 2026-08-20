#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
checker="$repo_root/scripts/check-pypi-artifacts.sh"
version=1.2.3
temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
valid_dist="$temporary/valid"
mkdir -p "$valid_dist"

artifacts=(
  "arust-${version}-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
  "arust-${version}-py3-none-macosx_11_0_arm64.whl"
)
for artifact in "${artifacts[@]}"; do
  printf 'fixture for %s\n' "$artifact" > "$valid_dist/$artifact"
done

make_json() {
  local output="$1"
  shift
  {
    for filename in "$@"; do
      digest="$(sha256sum "$valid_dist/$filename" | awk '{print $1}')"
      jq --null-input --compact-output \
        --arg filename "$filename" \
        --arg digest "$digest" \
        '{filename: $filename, digests: {sha256: $digest}}'
    done
  } | jq --slurp --arg version "$version" '{info: {version: $version}, urls: .}' > "$output"
}

expect_failure() {
  local expected="$1"
  shift
  local output
  if output="$("$@" 2>&1)"; then
    echo "expected command to fail: $*" >&2
    exit 1
  fi
  if ! grep --fixed-strings "$expected" <<< "$output" >/dev/null; then
    printf 'expected failure containing %q, got:\n%s\n' "$expected" "$output" >&2
    exit 1
  fi
}

manifest="$temporary/manifest"
"$checker" local "$valid_dist" "$version" > "$manifest"
[[ "$(wc -l < "$manifest")" -eq 2 ]]
cut --fields 3 "$manifest" | sort --check

absent="$temporary/absent.json"
make_json "$absent"
"$checker" prepublish "$valid_dist" "$version" "$absent" >/dev/null
expect_failure "PyPI is missing expected artifact" \
  "$checker" postpublish "$valid_dist" "$version" "$absent"

subset="$temporary/subset.json"
make_json "$subset" "${artifacts[0]}"
"$checker" prepublish "$valid_dist" "$version" "$subset" >/dev/null
expect_failure "PyPI is missing expected artifact" \
  "$checker" postpublish "$valid_dist" "$version" "$subset"

complete="$temporary/complete.json"
make_json "$complete" "${artifacts[@]}"
"$checker" prepublish "$valid_dist" "$version" "$complete" >/dev/null
"$checker" postpublish "$valid_dist" "$version" "$complete" >/dev/null

mismatch="$temporary/mismatch.json"
jq '.urls[0].digests.sha256 = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"' \
  "$complete" > "$mismatch"
expect_failure "PyPI SHA-256 mismatch" \
  "$checker" prepublish "$valid_dist" "$version" "$mismatch"

unexpected="$temporary/unexpected.json"
jq '.urls += [{filename: "arust-1.2.3-py3-none-any.whl", digests: {sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}]' \
  "$complete" > "$unexpected"
expect_failure "PyPI contains unexpected artifact" \
  "$checker" prepublish "$valid_dist" "$version" "$unexpected"

duplicate_remote="$temporary/duplicate-remote.json"
jq '.urls += [.urls[0]]' "$complete" > "$duplicate_remote"
expect_failure "PyPI response contains duplicate filename" \
  "$checker" prepublish "$valid_dist" "$version" "$duplicate_remote"

malformed="$temporary/malformed.json"
printf '{not json\n' > "$malformed"
expect_failure "PyPI response is malformed or missing info.version" \
  "$checker" prepublish "$valid_dist" "$version" "$malformed"

oversized="$temporary/oversized.json"
truncate --size 10485761 "$oversized"
expect_failure "PyPI response exceeds the 10 MiB limit" \
  "$checker" prepublish "$valid_dist" "$version" "$oversized"

wrong_version="$temporary/wrong-version.json"
jq '.info.version = "9.9.9"' "$complete" > "$wrong_version"
expect_failure "PyPI response version 9.9.9 does not match 1.2.3" \
  "$checker" prepublish "$valid_dist" "$version" "$wrong_version"

missing_local="$temporary/missing-local"
cp -R "$valid_dist" "$missing_local"
rm "$missing_local/${artifacts[0]}"
expect_failure "expected exactly one manylinux-x86_64 artifact, found 0" \
  "$checker" local "$missing_local" "$version"

duplicate_local="$temporary/duplicate-local"
cp -R "$valid_dist" "$duplicate_local"
printf duplicate > "$duplicate_local/arust-${version}-py3-none-macosx_12_0_arm64.whl"
expect_failure "expected exactly one macos-arm64 artifact, found 2" \
  "$checker" local "$duplicate_local" "$version"

extra_local="$temporary/extra-local"
cp -R "$valid_dist" "$extra_local"
printf extra > "$extra_local/extra.whl"
expect_failure "unexpected distribution artifact: extra.whl" \
  "$checker" local "$extra_local" "$version"

invalid_filename="$temporary/invalid-filename.json"
jq '.urls[0].filename = "artifact[$(false)].whl"' "$complete" > "$invalid_filename"
expect_failure "PyPI response contains an invalid filename" \
  "$checker" prepublish "$valid_dist" "$version" "$invalid_filename"

invalid_digest="$temporary/invalid-digest.json"
jq '.urls[0].digests.sha256 = "not-a-digest"' "$complete" > "$invalid_digest"
expect_failure "PyPI response contains an invalid SHA-256" \
  "$checker" prepublish "$valid_dist" "$version" "$invalid_digest"

missing_digest="$temporary/missing-digest.json"
jq 'del(.urls[0].digests.sha256)' "$complete" > "$missing_digest"
expect_failure "PyPI response contains an artifact without a valid filename and SHA-256" \
  "$checker" prepublish "$valid_dist" "$version" "$missing_digest"

printf 'PyPI artifact checks passed\n'
