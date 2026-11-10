#!/bin/sh
# habit installer — Linux (x86_64)
#
#   curl -fsSL https://github.com/Vulkan1125/habit-tracker/releases/latest/download/install.sh | sh
#
# Debian/Ubuntu, Fedora/RHEL and Arch get their native package; every other
# distribution gets the static musl binary in $PREFIX/bin (default
# ~/.local/bin), which needs no root. Every download is checked against the
# release's SHA256SUMS before it is touched.

set -eu

REPO="Vulkan1125/habit-tracker"
BIN="habit"

VERSION="${HABIT_VERSION:-}"
PREFIX="${PREFIX:-$HOME/.local}"
MODE="auto"
ASSUME_YES=0
DRY_RUN=0

say() { printf '%s\n' "$*"; }
warn() { printf '%s: %s\n' "$BIN" "$*" >&2; }
die() {
    warn "$*"
    exit 1
}
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
    cat <<EOF
Install $BIN from GitHub releases.

Usage: install.sh [options]

  -v, --version TAG   release to install, for example v0.5.0 (default: latest)
      --package       force the native package (.deb / .rpm / .pkg.tar.zst)
      --static        force the portable static binary; never asks for root
  -p, --prefix DIR    where --static puts the binary (default: ~/.local)
  -y, --yes           do not ask for confirmation
  -n, --dry-run       download and verify, then stop before installing
  -h, --help          show this text

Environment:
  HABIT_VERSION       same as --version
  PREFIX              same as --prefix
  HABIT_OS            override the detected OS (testing)
  HABIT_ARCH          override the detected architecture (testing)
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -h | --help)
            usage
            exit 0
            ;;
        -v | --version)
            [ "$#" -ge 2 ] || die "--version needs a tag"
            VERSION="$2"
            shift 2
            ;;
        --version=*)
            VERSION="${1#*=}"
            shift
            ;;
        --package)
            MODE="package"
            shift
            ;;
        --static)
            MODE="static"
            shift
            ;;
        -p | --prefix)
            [ "$#" -ge 2 ] || die "--prefix needs a directory"
            PREFIX="$2"
            shift 2
            ;;
        --prefix=*)
            PREFIX="${1#*=}"
            shift
            ;;
        -y | --yes)
            ASSUME_YES=1
            shift
            ;;
        -n | --dry-run)
            DRY_RUN=1
            shift
            ;;
        *)
            die "unknown option: $1 (try --help)"
            ;;
    esac
done

fetch() { # url dest
    if have curl; then
        curl -fsSL --retry 3 -o "$2" "$1"
    elif have wget; then
        wget -q -O "$2" "$1"
    else
        die "curl or wget is required"
    fi
}

fetch_text() { # url -> stdout
    if have curl; then
        curl -fsSL --retry 3 "$1"
    elif have wget; then
        wget -q -O - "$1"
    else
        die "curl or wget is required"
    fi
}

latest_tag() {
    if have curl; then
        effective=$(curl -fsSIL -o /dev/null -w '%{url_effective}' \
            "https://github.com/$REPO/releases/latest") ||
            die "cannot reach github.com"
    else
        effective=$(fetch_text "https://api.github.com/repos/$REPO/releases/latest" |
            sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')
    fi
    tag=${effective##*/}
    [ -n "$tag" ] || die "could not work out the latest release"
    printf '%s\n' "$tag"
}

confirm() {
    [ "$ASSUME_YES" -eq 1 ] && return 0
    if [ -r /dev/tty ]; then
        printf '%s [Y/n] ' "$1" >/dev/tty
        read -r answer </dev/tty || answer=""
        case "$answer" in
            "" | y | Y | yes | YES) return 0 ;;
            *) return 1 ;;
        esac
    fi
    # No terminal to ask on (a pipeline); carry on.
    return 0
}

run_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    else
        sudo "$@"
    fi
}

os="${HABIT_OS:-$(uname -s)}"
arch="${HABIT_ARCH:-$(uname -m)}"

case "$os" in
    Linux) ;;
    Darwin)
        die "macOS is installed with Homebrew instead:
  brew install Vulkan1125/tap/habit"
        ;;
    *)
        die "unsupported platform: $os (Linux x86_64 is supported)"
        ;;
esac

case "$arch" in
    x86_64 | amd64) arch="x86_64" ;;
    aarch64 | arm64)
        die "there is no prebuilt Linux arm64 build yet; build from source:
  cargo install --git https://github.com/$REPO"
        ;;
    *)
        die "unsupported architecture: $arch"
        ;;
esac

tag="$VERSION"
[ -n "$tag" ] || tag=$(latest_tag)
case "$tag" in
    v*) ;;
    *) tag="v$tag" ;;
esac
version="${tag#v}"

kind="static"
if [ "$MODE" != "static" ]; then
    if have apt-get && have dpkg; then
        kind="deb"
    elif have dnf; then
        kind="rpm"
    elif have yum; then
        kind="rpm"
    elif have pacman; then
        kind="arch"
    fi
    if [ "$kind" != "static" ] && [ "$(id -u)" -ne 0 ] && ! have sudo; then
        warn "sudo is not available, using the static binary instead"
        kind="static"
    fi
    if [ "$MODE" = "package" ] && [ "$kind" = "static" ]; then
        die "--package needs apt/dpkg, dnf, yum or pacman"
    fi
fi

case "$kind" in
    deb) asset="habit_${version}_amd64.deb" ;;
    rpm) asset="habit-${version}-1.x86_64.rpm" ;;
    arch) asset="habit-${version}-1-x86_64.pkg.tar.zst" ;;
    *) asset="habit-${version}-x86_64-linux-musl.tar.gz" ;;
esac

base="https://github.com/$REPO/releases/download/$tag"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/habit-install.XXXXXX")
trap 'rm -rf "$tmp"' EXIT INT TERM

say "habit $version · $asset"
fetch "$base/$asset" "$tmp/$asset"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS"

expected=$(awk -v name="$asset" '$2 == name { print $1 }' "$tmp/SHA256SUMS")
[ -n "$expected" ] || die "$asset is missing from $tag's SHA256SUMS"
if have sha256sum; then
    actual=$(sha256sum "$tmp/$asset" | awk '{ print $1 }')
else
    actual=$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')
fi
[ "$expected" = "$actual" ] || die "checksum mismatch for $asset"
say "checksum ok"

case "$kind" in
    deb) plan="install with apt-get (or dpkg)" ;;
    rpm) plan="install with dnf (or yum)" ;;
    arch) plan="install with pacman" ;;
    *) plan="unpack into $PREFIX/bin" ;;
esac

if [ "$DRY_RUN" -eq 1 ]; then
    say "dry run: would $plan"
    exit 0
fi

case "$kind" in
    deb)
        confirm "Install habit $version with apt-get?" || exit 1
        if have apt-get; then
            run_root apt-get install -y "$tmp/$asset"
        else
            run_root dpkg -i "$tmp/$asset"
        fi
        ;;
    rpm)
        confirm "Install habit $version with dnf?" || exit 1
        if have dnf; then
            run_root dnf install -y "$tmp/$asset"
        else
            run_root yum install -y "$tmp/$asset"
        fi
        ;;
    arch)
        confirm "Install habit $version with pacman?" || exit 1
        run_root pacman -U --noconfirm "$tmp/$asset"
        ;;
    *)
        confirm "Install habit $version into $PREFIX/bin?" || exit 1
        mkdir -p "$PREFIX/bin" || die "cannot create $PREFIX/bin"
        tar -xzf "$tmp/$asset" -C "$tmp"
        source=$(find "$tmp" -type f -name "$BIN" -perm -u+x | head -n 1)
        [ -n "$source" ] || die "the archive did not contain a $BIN binary"
        install -m 755 "$source" "$PREFIX/bin/$BIN"
        case ":$PATH:" in
            *":$PREFIX/bin:"*) ;;
            *)
                say ""
                say "add $PREFIX/bin to PATH:"
                say "  export PATH=\"$PREFIX/bin:\$PATH\""
                ;;
        esac
        ;;
esac

if have "$BIN"; then
    say "installed $("$BIN" --version 2>/dev/null || echo "$BIN $version")"
else
    say "installed $BIN $version"
fi
