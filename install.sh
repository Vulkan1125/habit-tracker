#!/bin/sh
# habit installer: Linux and macOS
#
#   curl -fsSL https://github.com/Vulkan1125/habit-tracker/releases/latest/download/install.sh | sh
#
# On Linux x86_64, Debian/Ubuntu, Fedora/RHEL and Arch get their native package;
# every other distribution and macOS get the portable prebuilt binary in
# $PREFIX/bin (default ~/.local/bin), which needs no root. Platforms without a
# prebuilt build (Linux arm64, for example) are compiled from source, and if
# Rust is missing the installer offers to set it up with rustup. Every download
# is checked against the release's SHA256SUMS before it is touched.

set -eu

REPO="Vulkan1125/habit-tracker"
BIN="habit"
BINS="habit habit-notify habit-report"

VERSION="${HABIT_VERSION:-}"
PREFIX="${PREFIX:-$HOME/.local}"
MODE="auto"
ASSUME_YES=0
DRY_RUN=0
SETUP_PATH="ask"

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

  -v, --version TAG   release to install, for example v0.8.0 (default: latest)
      --package       force the native package (.deb / .rpm / .pkg.tar.zst)
      --static        force the prebuilt binary; never asks for root
      --from-source   compile from source, installing Rust first if it is missing
  -p, --prefix DIR    where the portable binaries go (default: ~/.local)
      --no-path       do not offer to add the install directory to PATH
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
        --from-source)
            MODE="source"
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
        --no-path)
            SETUP_PATH="no"
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
    Linux | Darwin) ;;
    *)
        die "unsupported platform: $os (Linux and macOS are supported)"
        ;;
esac

case "$arch" in
    x86_64 | amd64) arch="x86_64" ;;
    aarch64 | arm64) arch="arm64" ;;
    *)
        die "unsupported architecture: $arch"
        ;;
esac

# Which prebuilt archive exists for this platform, if any.
prebuilt=""
if [ "$os" = "Linux" ] && [ "$arch" = "x86_64" ]; then
    prebuilt="linux"
elif [ "$os" = "Darwin" ]; then
    prebuilt="macos"
fi

tag="$VERSION"
[ -n "$tag" ] || tag=$(latest_tag)
case "$tag" in
    v*) ;;
    *) tag="v$tag" ;;
esac
version="${tag#v}"

# Pick the install strategy: native package, prebuilt archive, or source.
kind=""
if [ "$MODE" = "source" ]; then
    kind="source"
elif [ "$MODE" = "static" ] && [ -z "$prebuilt" ]; then
    die "there is no prebuilt $os $arch build; use --from-source"
else
    if [ "$MODE" != "static" ] && [ -z "$prebuilt" ]; then
        warn "no prebuilt $os $arch build; compiling from source"
        kind="source"
    elif [ "$MODE" != "static" ]; then
        if have apt-get && have dpkg; then
            kind="deb"
        elif have dnf || have yum; then
            kind="rpm"
        elif have pacman; then
            kind="arch"
        else
            kind="static"
        fi
        if [ "$kind" != "static" ] && [ "$(id -u)" -ne 0 ] && ! have sudo; then
            warn "sudo is not available, using the prebuilt binary instead"
            kind="static"
        fi
        if [ "$MODE" = "package" ] && [ "$kind" = "static" ]; then
            die "--package needs apt/dpkg, dnf, yum or pacman"
        fi
    else
        kind="static"
    fi
fi

install_dir="$PREFIX/bin"

if [ "$kind" = "source" ]; then
    if ! have cargo; then
        say ""
        say "Rust is not installed, and building habit needs it."
        if ! confirm "Install Rust now with rustup?"; then
            die "install Rust from https://rustup.rs and run this script again"
        fi
        tmp=$(mktemp -d "${TMPDIR:-/tmp}/habit-install.XXXXXX")
        trap 'rm -rf "$tmp"' EXIT INT TERM
        fetch "https://sh.rustup.rs" "$tmp/rustup.sh"
        if [ "$DRY_RUN" -eq 1 ]; then
            say "dry run: would install Rust with rustup, then build habit $version"
            exit 0
        fi
        say "installing Rust with rustup, this can take a minute"
        sh "$tmp/rustup.sh" -y --profile minimal --quiet
        if [ -f "$HOME/.cargo/env" ]; then
            # shellcheck disable=SC1091
            . "$HOME/.cargo/env"
        fi
        have cargo ||
            die "rustup finished but cargo is not on PATH; open a new terminal and retry"
        say "rust ready: $(cargo --version)"
    fi
    have git || die "git is required to build from source"
    if [ "$DRY_RUN" -eq 1 ]; then
        say "dry run: would build habit $version from source into $install_dir"
        exit 0
    fi
    confirm "Build habit $version from source into $install_dir?" || exit 1
    say "compiling, this can take a few minutes"
    cargo install --quiet --git "https://github.com/$REPO" --tag "$tag" --locked \
        --root "$PREFIX" --force ||
        die "the build failed"
    binary="$install_dir/$BIN"
else
    case "$kind" in
        deb) asset="habit_${version}_amd64.deb" ;;
        rpm) asset="habit-${version}-1.x86_64.rpm" ;;
        arch) asset="habit-${version}-1-x86_64.pkg.tar.zst" ;;
        static)
            if [ "$os" = "Darwin" ]; then
                asset="habit-${version}-macos-universal.tar.gz"
            else
                asset="habit-${version}-x86_64-linux-musl.tar.gz"
            fi
            ;;
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
        *) plan="unpack into $install_dir" ;;
    esac

    if [ "$DRY_RUN" -eq 1 ]; then
        say "dry run: would $plan"
        exit 0
    fi

    case "$kind" in
        deb)
            confirm "Install habit $version with apt-get?" || exit 1
            if have apt-get; then
                run_root apt-get install -y -qq "$tmp/$asset"
            else
                run_root dpkg -i "$tmp/$asset"
            fi
            install_dir="/usr/bin"
            ;;
        rpm)
            confirm "Install habit $version with dnf?" || exit 1
            if have dnf; then
                run_root dnf install -y -q "$tmp/$asset"
            else
                run_root yum install -y -q "$tmp/$asset"
            fi
            install_dir="/usr/bin"
            ;;
        arch)
            confirm "Install habit $version with pacman?" || exit 1
            run_root pacman -U --noconfirm --quiet "$tmp/$asset"
            install_dir="/usr/bin"
            ;;
        *)
            confirm "Install habit $version into $install_dir?" || exit 1
            mkdir -p "$install_dir" || die "cannot create $install_dir"
            tar -xzf "$tmp/$asset" -C "$tmp"
            for name in $BINS; do
                source=$(find "$tmp" -type f -name "$name" -perm -u+x | head -n 1)
                [ -n "$source" ] || die "the archive did not contain a $name binary"
                install -m 755 "$source" "$install_dir/$name"
            done
            ;;
    esac
    binary="$install_dir/$BIN"
fi

# Offer to make the commands available from anywhere. Package installs land in
# /usr/bin, which is already on PATH, so this only usually asks for ~/.local.
offer_path() {
    dir="$1"
    case ":$PATH:" in
        *":$dir:"*) return 0 ;;
    esac
    if [ "$SETUP_PATH" = "no" ]; then
        say ""
        say "add $dir to PATH when you want it:"
        say "  export PATH=\"$dir:\$PATH\""
        return 0
    fi
    say ""
    if ! confirm "Make $BIN available from any terminal (add $dir to PATH)?"; then
        say "add it yourself when you need it:"
        say "  export PATH=\"$dir:\$PATH\""
        return 0
    fi
    shell_name=$(basename "${SHELL:-sh}")
    case "$shell_name" in
        fish)
            rc="$HOME/.config/fish/config.fish"
            line="set -gx PATH $dir \$PATH"
            ;;
        zsh) rc="$HOME/.zshrc"; line="export PATH=\"$dir:\$PATH\"" ;;
        bash)
            if [ -f "$HOME/.bash_profile" ]; then
                rc="$HOME/.bash_profile"
            else
                rc="$HOME/.bashrc"
            fi
            line="export PATH=\"$dir:\$PATH\""
            ;;
        *) rc="$HOME/.profile"; line="export PATH=\"$dir:\$PATH\"" ;;
    esac
    mkdir -p "$(dirname "$rc")" 2>/dev/null || true
    if [ -f "$rc" ] && grep -F "$line" "$rc" >/dev/null 2>&1; then
        say "PATH is already set in $rc"
    elif printf '\n# habit\n%s\n' "$line" >>"$rc"; then
        say "added $dir to PATH in $rc"
        say "open a new terminal, or run: . \"$rc\""
    else
        die "could not write to $rc"
    fi
}

offer_path "$install_dir"

if [ -x "$binary" ]; then
    say ""
    say "installed $("$binary" --version 2>/dev/null || echo "$BIN $version")"
    say "next: $BIN modules list   # add notifications or reports when you want them"
else
    say "installed $BIN $version"
fi
