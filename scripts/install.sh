#!/bin/sh
# Install aru from a checksum-verified GitHub Release archive.

set -eu

repository="narumiruna/aru"
api_url="https://api.github.com/repos/$repository/releases/latest"
release_url="https://github.com/$repository/releases/download"
temp_dir=
staged_binary=

fail() {
    printf 'aru installer: error: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [ -n "$staged_binary" ]; then
        rm -f "$staged_binary"
    fi
    if [ -n "$temp_dir" ]; then
        rm -rf "$temp_dir"
    fi
}

trap cleanup 0
trap 'exit 1' 1 2 3 15

has_command() {
    command -v "$1" >/dev/null 2>&1
}

download() {
    source_url=$1
    destination=$2

    if has_command curl; then
        curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
            "$source_url" -o "$destination"
    elif has_command wget; then
        wget -q "$source_url" -O "$destination"
    else
        fail "curl or wget is required"
    fi
}

validate_version() {
    candidate=$1
    major=${candidate%%.*}
    remainder=${candidate#*.}
    [ "$remainder" != "$candidate" ] || return 1
    minor=${remainder%%.*}
    patch=${remainder#*.}
    [ "$patch" != "$remainder" ] || return 1

    case "$patch" in
        *.*) return 1 ;;
    esac
    case "$major:$minor:$patch" in
        *[!0-9:]*|:*|*::*|*:) return 1 ;;
    esac
    return 0
}

resolve_version() {
    requested=${ARU_VERSION:-}
    if [ -n "$requested" ]; then
        case "$requested" in
            v*) requested=${requested#v} ;;
        esac
        validate_version "$requested" || fail "ARU_VERSION must match X.Y.Z"
        printf '%s\n' "$requested"
        return
    fi

    metadata="$temp_dir/latest.json"
    download "$api_url" "$metadata"
    requested=$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\([^"]*\)".*/\1/p' "$metadata")
    validate_version "$requested" || fail "could not determine the latest stable release"
    printf '%s\n' "$requested"
}

platform=$(uname -s)
architecture=$(uname -m)
case "$platform:$architecture" in
    Linux:x86_64|Linux:amd64)
        target="x86_64-unknown-linux-musl"
        ;;
    Darwin:x86_64|Darwin:amd64)
        target="x86_64-apple-darwin"
        ;;
    Darwin:arm64|Darwin:aarch64)
        target="aarch64-apple-darwin"
        ;;
    *)
        fail "unsupported platform: $platform $architecture"
        ;;
esac

if [ -n "${ARU_INSTALL_DIR:-}" ]; then
    install_dir=$ARU_INSTALL_DIR
else
    [ -n "${HOME:-}" ] || fail "HOME is not set; set ARU_INSTALL_DIR explicitly"
    install_dir="$HOME/.local/bin"
fi

umask 077
temp_dir=$(mktemp -d "${TMPDIR:-/tmp}/aru-installer.XXXXXX") || fail "could not create a temporary directory"
version=$(resolve_version)
archive="aru-$version-$target.tar.gz"
archive_path="$temp_dir/$archive"
checksums_path="$temp_dir/SHA256SUMS"
asset_url="$release_url/v$version"

download "$asset_url/$archive" "$archive_path"
download "$asset_url/SHA256SUMS" "$checksums_path"

expected_checksum=$(awk -v name="$archive" '$2 == name { print $1; exit }' "$checksums_path")
case "$expected_checksum" in
    ''|*[!0-9a-fA-F]*) fail "no valid checksum found for $archive" ;;
esac
[ "${#expected_checksum}" -eq 64 ] || fail "no valid checksum found for $archive"

if has_command sha256sum; then
    actual_checksum=$(sha256sum "$archive_path" | awk '{print $1}')
elif has_command shasum; then
    actual_checksum=$(shasum -a 256 "$archive_path" | awk '{print $1}')
else
    fail "sha256sum or shasum is required to verify the download"
fi
[ "$actual_checksum" = "$expected_checksum" ] || fail "checksum verification failed for $archive"

archive_entries=$(tar -tzf "$archive_path") || fail "could not read $archive"
[ "$archive_entries" = "aru" ] || fail "release archive has unexpected contents"
mkdir "$temp_dir/extract"
tar -xzf "$archive_path" -C "$temp_dir/extract" || fail "could not extract $archive"
[ -f "$temp_dir/extract/aru" ] && [ ! -L "$temp_dir/extract/aru" ] || fail "release archive does not contain a regular aru binary"

mkdir -p "$install_dir" || fail "could not create $install_dir"
staged_binary=$(mktemp "$install_dir/.aru.XXXXXX") || fail "could not stage aru in $install_dir"
cp "$temp_dir/extract/aru" "$staged_binary"
chmod 755 "$staged_binary"
mv -f "$staged_binary" "$install_dir/aru"
staged_binary=

printf 'Installed aru %s to %s/aru\n' "$version" "$install_dir"
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *) printf 'Add %s to your PATH to run aru.\n' "$install_dir" ;;
esac
