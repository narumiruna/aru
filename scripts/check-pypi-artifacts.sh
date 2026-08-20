#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 <local|prepublish|postpublish> <dist-dir> <version> [pypi-json]" >&2
  exit 2
}

[[ $# -ge 3 && $# -le 4 ]] || usage
mode="$1"
dist="$2"
version="$3"
remote_json="${4:-}"

case "$mode" in
  local)
    [[ $# -eq 3 ]] || usage
    ;;
  prepublish|postpublish)
    [[ $# -eq 4 ]] || usage
    ;;
  *)
    usage
    ;;
esac

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "version must be stable SemVer, got: $version" >&2
  exit 1
fi
if [[ ! -d "$dist" ]]; then
  echo "distribution directory does not exist: $dist" >&2
  exit 1
fi

expected_categories=(
  manylinux-x86_64
  macos-arm64
)
declare -A category_counts=()
declare -A local_digests=()
declare -A local_sizes=()
for category in "${expected_categories[@]}"; do
  category_counts["$category"]=0
done

classify_artifact() {
  local filename="$1"
  local version_pattern="${version//./\\.}"
  if [[ "$filename" =~ ^arust-${version_pattern}-py3-none-macosx_[0-9]+_[0-9]+_arm64\.whl$ ]]; then
    printf '%s\n' macos-arm64
    return
  fi
  case "$filename" in
    "arust-${version}-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl")
      printf '%s\n' manylinux-x86_64
      ;;
    *)
      return 1
      ;;
  esac
}

while IFS= read -r -d '' path; do
  if [[ ! -f "$path" || -L "$path" ]]; then
    echo "unexpected non-regular distribution entry: $path" >&2
    exit 1
  fi
  filename="${path##*/}"
  if [[ "$filename" == *$'\n'* || "$filename" == *$'\t'* ]]; then
    echo "distribution filename contains unsupported whitespace" >&2
    exit 1
  fi
  if ! category="$(classify_artifact "$filename")"; then
    echo "unexpected distribution artifact: $filename" >&2
    exit 1
  fi
  category_counts["$category"]=$((category_counts["$category"] + 1))
  local_digests["$filename"]="$(sha256sum "$path" | awk '{print $1}')"
  local_sizes["$filename"]="$(stat --format '%s' "$path")"
done < <(find "$dist" -mindepth 1 -maxdepth 1 -print0 | sort -z)

for category in "${expected_categories[@]}"; do
  count="${category_counts[$category]}"
  if [[ "$count" -ne 1 ]]; then
    echo "expected exactly one $category artifact, found $count" >&2
    exit 1
  fi
done

mapfile -t local_filenames < <(printf '%s\n' "${!local_digests[@]}" | sort)
for filename in "${local_filenames[@]}"; do
  printf '%s\t%s\t%s\n' \
    "${local_digests[$filename]}" \
    "${local_sizes[$filename]}" \
    "$filename"
done

if [[ "$mode" == local ]]; then
  exit 0
fi

if [[ ! -f "$remote_json" || -L "$remote_json" ]]; then
  echo "PyPI JSON file is not a regular file: $remote_json" >&2
  exit 1
fi
if [[ "$(stat --format '%s' "$remote_json")" -gt 10485760 ]]; then
  echo "PyPI response exceeds the 10 MiB limit" >&2
  exit 1
fi
if ! remote_version="$(jq --exit-status --raw-output '.info.version | strings' "$remote_json" 2>/dev/null)"; then
  echo "PyPI response is malformed or missing info.version" >&2
  exit 1
fi
if [[ "$remote_version" != "$version" ]]; then
  echo "PyPI response version $remote_version does not match $version" >&2
  exit 1
fi
if ! jq --exit-status '.urls | arrays' "$remote_json" >/dev/null 2>&1; then
  echo "PyPI response is malformed or missing urls" >&2
  exit 1
fi

declare -A remote_digests=()
while IFS=$'\t' read -r filename digest; do
  if [[ -z "$filename" || ! "$filename" =~ ^[A-Za-z0-9._+-]+$ ]]; then
    echo "PyPI response contains an invalid filename" >&2
    exit 1
  fi
  if [[ ! "$digest" =~ ^[0-9a-f]{64}$ ]]; then
    echo "PyPI response contains an invalid SHA-256 for $filename" >&2
    exit 1
  fi
  if [[ -v "remote_digests[$filename]" ]]; then
    echo "PyPI response contains duplicate filename: $filename" >&2
    exit 1
  fi
  remote_digests["$filename"]="$digest"
done < <(
  jq --exit-status --raw-output \
    '.urls[] | select((.filename | type) == "string" and (.digests.sha256 | type) == "string") | [.filename, .digests.sha256] | @tsv' \
    "$remote_json"
)

remote_count="$(jq '.urls | length' "$remote_json")"
if [[ "$remote_count" -ne "${#remote_digests[@]}" ]]; then
  echo "PyPI response contains an artifact without a valid filename and SHA-256" >&2
  exit 1
fi

for filename in "${!remote_digests[@]}"; do
  if [[ ! -v "local_digests[$filename]" ]]; then
    echo "PyPI contains unexpected artifact: $filename" >&2
    exit 1
  fi
  if [[ "${remote_digests[$filename]}" != "${local_digests[$filename]}" ]]; then
    echo "PyPI SHA-256 mismatch for $filename" >&2
    exit 1
  fi
done

if [[ "$mode" == postpublish ]]; then
  for filename in "${local_filenames[@]}"; do
    if [[ ! -v "remote_digests[$filename]" ]]; then
      echo "PyPI is missing expected artifact: $filename" >&2
      exit 3
    fi
  done
fi
