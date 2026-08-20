#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
installer="$repo_root/scripts/install.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

version="1.2.3"
target="x86_64-unknown-linux-musl"
archive="aru-$version-$target.tar.gz"
mkdir -p "$tmp/payload"
printf '#!/bin/sh\nprintf "aru fixture\\n"\n' > "$tmp/payload/aru"
chmod 755 "$tmp/payload/aru"
tar -czf "$tmp/$archive" -C "$tmp/payload" aru
checksum="$(sha256sum "$tmp/$archive" | awk '{print $1}')"
printf '%s  %s\n' "$checksum" "$archive" > "$tmp/SHA256SUMS"
printf '{"tag_name":"v%s"}\n' "$version" > "$tmp/latest.json"

make_tool_path() {
  local destination="$1"
  local downloader="$2"
  mkdir -p "$destination"

  local tool
  for tool in awk chmod cp gzip mkdir mktemp mv rm sed tar; do
    ln -s "$(command -v "$tool")" "$destination/$tool"
  done
  ln -s "$(command -v sha256sum)" "$destination/sha256sum"
  ln -s /bin/sh "$destination/sh"

  cat > "$destination/uname" <<'EOF'
#!/bin/sh
case "${1:-}" in
  -s) printf '%s\n' "${TEST_OS:-Linux}" ;;
  -m) printf '%s\n' "${TEST_ARCH:-x86_64}" ;;
  *) exit 2 ;;
esac
EOF
  chmod 755 "$destination/uname"

  cat > "$destination/$downloader" <<'EOF'
#!/bin/sh
output=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o|-O)
      output=$2
      shift 2
      ;;
    --proto)
      shift 2
      ;;
    --tlsv1.2|--fail|--location|--silent|--show-error|-q)
      shift
      ;;
    *)
      url=$1
      shift
      ;;
  esac
done
printf '%s\n' "$url" >> "$TEST_DOWNLOAD_LOG"
case "$url" in
  */releases/latest) source_file=$TEST_LATEST_JSON ;;
  */SHA256SUMS) source_file=$TEST_CHECKSUMS ;;
  *.tar.gz) source_file=$TEST_ARCHIVE ;;
  *) printf 'unexpected download URL: %s\n' "$url" >&2; exit 2 ;;
esac
cp "$source_file" "$output"
EOF
  chmod 755 "$destination/$downloader"
}

run_installer() {
  local tool_path="$1"
  local install_dir="$2"
  local checksums="$3"
  shift 3

  : > "$tmp/download.log"
  /usr/bin/env \
    -u ARU_VERSION \
    PATH="$tool_path" \
    ARU_INSTALL_DIR="$install_dir" \
    TEST_ARCHIVE="$tmp/$archive" \
    TEST_CHECKSUMS="$checksums" \
    TEST_DOWNLOAD_LOG="$tmp/download.log" \
    TEST_LATEST_JSON="$tmp/latest.json" \
    "$@" \
    /bin/sh "$installer"
}

curl_path="$tmp/curl-path"
make_tool_path "$curl_path" curl
curl_install="$tmp/curl-install"
run_installer "$curl_path" "$curl_install" "$tmp/SHA256SUMS"
cmp "$tmp/payload/aru" "$curl_install/aru"
grep -Fx "https://api.github.com/repos/narumiruna/aru/releases/latest" "$tmp/download.log" >/dev/null
grep -Fx "https://github.com/narumiruna/aru/releases/download/v$version/$archive" "$tmp/download.log" >/dev/null
grep -Fx "https://github.com/narumiruna/aru/releases/download/v$version/SHA256SUMS" "$tmp/download.log" >/dev/null

echo "curl-backed latest release installation passed"

wget_path="$tmp/wget-path"
make_tool_path "$wget_path" wget
wget_install="$tmp/wget-install"
: > "$tmp/download.log"
cat "$installer" | /usr/bin/env \
  PATH="$wget_path" \
  ARU_INSTALL_DIR="$wget_install" \
  ARU_VERSION="$version" \
  TEST_ARCHIVE="$tmp/$archive" \
  TEST_CHECKSUMS="$tmp/SHA256SUMS" \
  TEST_DOWNLOAD_LOG="$tmp/download.log" \
  TEST_LATEST_JSON="$tmp/latest.json" \
  /bin/sh
cmp "$tmp/payload/aru" "$wget_install/aru"
if grep -F '/releases/latest' "$tmp/download.log" >/dev/null; then
  echo "explicit version unexpectedly queried the latest release" >&2
  exit 1
fi

echo "wget-backed explicit version installation passed"

bad_checksums="$tmp/BAD_SHA256SUMS"
printf '%064d  %s\n' 0 "$archive" > "$bad_checksums"
printf 'existing binary\n' > "$curl_install/aru"
if run_installer "$curl_path" "$curl_install" "$bad_checksums" >"$tmp/bad.out" 2>"$tmp/bad.err"; then
  echo "checksum mismatch unexpectedly succeeded" >&2
  exit 1
fi
grep -Fx 'existing binary' "$curl_install/aru" >/dev/null
grep -F 'checksum' "$tmp/bad.err" >/dev/null

echo "checksum mismatch preserved the existing binary"

if TEST_ARCH=aarch64 run_installer "$curl_path" "$tmp/unsupported-install" "$tmp/SHA256SUMS" >"$tmp/unsupported.out" 2>"$tmp/unsupported.err"; then
  echo "unsupported target unexpectedly succeeded" >&2
  exit 1
fi
grep -F 'unsupported platform' "$tmp/unsupported.err" >/dev/null

echo "unsupported target rejection passed"
