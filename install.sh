#!/bin/sh
# install.sh — install the toltrim binary on macOS and Linux.
#
# The script detects the operating system and the architecture, maps them to the
# release archive the release workflow builds for that platform family,
# downloads it from the project's GitHub releases, checks that it really is a
# non-empty archive, extracts the binary into a directory the user chose, and
# prints exactly what it changed and where. It never leaves a partial binary
# behind: the binary is staged inside the destination directory and moved into
# place in one step, and the staging directory is removed on every exit.
#
# It is POSIX sh, has no dependency beyond curl (or wget) and tar, and `--help`
# needs no network at all.

set -eu

# owner/repo to download from; a fork overrides it.
REPO="${TOLTRIM_REPO:-shaneshou/toltrim}"
INSTALL_DIR="${TOLTRIM_INSTALL_DIR:-${HOME:-}/.local/bin}"
VERSION="latest"

die() {
    printf 'install.sh: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat <<'EOF'
install.sh: install the toltrim binary on macOS or Linux.

USAGE:
    install.sh [--version <tag>]

OPTIONS:
    --version <tag>   the release tag to install (default: latest)
    -h, --help        show this message and exit

ENVIRONMENT:
    TOLTRIM_INSTALL_DIR   where the binary is installed (default: ~/.local/bin)
    TOLTRIM_REPO          owner/repo to download from (default: shaneshou/toltrim)
    TMPDIR                where the download is staged

Uninstalling is removing the one file this script created.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -h | --help)
            usage
            exit 0
            ;;
        --version)
            [ "$#" -ge 2 ] || die "--version needs a tag, e.g. --version v0.1.0"
            VERSION="$2"
            shift 2
            ;;
        --version=*)
            VERSION="${1#--version=}"
            shift
            ;;
        *)
            die "unknown argument \`$1'; try --help"
            ;;
    esac
done

[ -n "${HOME:-}" ] || [ -n "${TOLTRIM_INSTALL_DIR:-}" ] ||
    die "HOME is not set; set TOLTRIM_INSTALL_DIR to choose where to install"
[ -n "$VERSION" ] || die "--version needs a tag, e.g. --version v0.1.0"

# Map this machine onto one of the release workflow's platform families.
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
    Darwin)
        case "$arch" in
            arm64 | aarch64) target="aarch64-apple-darwin" ;;
            x86_64 | amd64) target="x86_64-apple-darwin" ;;
            *) die "unsupported macOS architecture: $arch" ;;
        esac
        ;;
    Linux)
        case "$arch" in
            x86_64 | amd64) target="x86_64-unknown-linux-gnu" ;;
            *) die "unsupported Linux architecture: $arch (the release workflow builds x86_64 only; build from source with cargo instead)" ;;
        esac
        ;;
    *)
        die "unsupported operating system: $os (this script installs on macOS and Linux; use install.ps1 on Windows)"
        ;;
esac

download() {
    url="$1"
    out="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$out" || die "download failed: $url"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$out" || die "download failed: $url"
    else
        die "neither curl nor wget is installed; install one and try again"
    fi
}

tmp="$(mktemp -d "${TMPDIR:-/tmp}/toltrim-install.XXXXXX")" ||
    die "cannot create a temporary directory under ${TMPDIR:-/tmp}"
trap 'rm -rf "$tmp"' 0
trap 'exit 1' 1 2 15

if [ "$VERSION" = "latest" ]; then
    download "https://api.github.com/repos/$REPO/releases/latest" "$tmp/latest.json"
    tag="$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$tmp/latest.json" | head -n 1)"
    [ -n "$tag" ] || die "could not read the latest release tag from $REPO"
else
    tag="$VERSION"
    case "$tag" in
        v*) ;;
        *) tag="v$tag" ;;
    esac
fi
version="${tag#v}"

archive="$tmp/toltrim-$version-$target.tar.gz"
url="https://github.com/$REPO/releases/download/$tag/toltrim-$version-$target.tar.gz"
download "$url" "$archive"

# A failed download must not be extracted, and an HTML error page is not an
# archive. Both checks happen before anything is installed.
[ -s "$archive" ] || die "the downloaded archive is empty: $url"
tar -tzf "$archive" >/dev/null 2>&1 || die "the download is not a readable tar.gz archive: $url"
tar -xzf "$archive" -C "$tmp" || die "cannot extract the download: $url"

binary="$tmp/toltrim-$version-$target/toltrim"
if [ ! -f "$binary" ]; then
    binary="$(find "$tmp" -type f -name toltrim 2>/dev/null | head -n 1)"
fi
[ -n "$binary" ] && [ -f "$binary" ] || die "the archive does not contain a toltrim binary: $url"

mkdir -p "$INSTALL_DIR" || die "cannot create the install directory $INSTALL_DIR"
staged="$INSTALL_DIR/.toltrim.staged.$$"
cp "$binary" "$staged" || {
    rm -f "$staged"
    die "cannot write to $INSTALL_DIR"
}
chmod 755 "$staged" || {
    rm -f "$staged"
    die "cannot make the staged binary executable"
}
mv -f "$staged" "$INSTALL_DIR/toltrim" || {
    rm -f "$staged"
    die "cannot move the binary into $INSTALL_DIR"
}

printf 'installed toltrim %s for %s\n' "$version" "$target"
printf '  from %s\n' "$url"
printf '  to   %s\n' "$INSTALL_DIR/toltrim"

case ":${PATH:-}:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        printf 'note: %s is not on your PATH; add it with:\n' "$INSTALL_DIR"
        printf '  export PATH="%s:$PATH"\n' "$INSTALL_DIR"
        ;;
esac

printf 'to uninstall, remove that one file:\n'
printf '  rm %s\n' "$INSTALL_DIR/toltrim"
