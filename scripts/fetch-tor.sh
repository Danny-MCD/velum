#!/usr/bin/env bash
# Downloads the official Tor Expert Bundle for macOS/Linux and drops the
# `tor` binary into src-tauri/binaries/ using Tauri's sidecar naming
# convention, ready for `cargo tauri dev` / `cargo tauri build`.
#
# As with fetch-tor.ps1, this skips GPG signature verification to stay
# dependency-free. Verify the .asc signature before shipping anything you
# intend to distribute:
# https://support.torproject.org/little-t-tor/verify-signature/
set -euo pipefail

VERSION="${1:-15.0.20}"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin)
    platform="macos"
    case "$arch" in
      arm64) triple="aarch64-apple-darwin" ;;
      x86_64) triple="x86_64-apple-darwin" ;;
      *) echo "Unsupported macOS arch: $arch" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    platform="linux"
    case "$arch" in
      x86_64) triple="x86_64-unknown-linux-gnu" ;;
      i686) triple="i686-unknown-linux-gnu" ;;
      *) echo "Unsupported Linux arch: $arch" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $os" >&2; exit 1 ;;
esac

# The dist archive names use x86_64/i686 for linux and x86_64/aarch64 for macOS.
bundle_arch="$arch"
[ "$platform" = "linux" ] && [ "$arch" = "x86_64" ] && bundle_arch="x86_64"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cache_dir="$root/scripts/.cache"
binaries_dir="$root/src-tauri/binaries"
mkdir -p "$cache_dir" "$binaries_dir"

archive="tor-expert-bundle-${platform}-${bundle_arch}-${VERSION}.tar.gz"
url="https://dist.torproject.org/torbrowser/${VERSION}/${archive}"
archive_path="$cache_dir/$archive"
extract_dir="$cache_dir/${platform}-${bundle_arch}-${VERSION}"

if [ ! -f "$archive_path" ]; then
  echo "Downloading $url"
  curl -fL "$url" -o "$archive_path"
else
  echo "Using cached $archive_path"
fi

if [ ! -d "$extract_dir" ]; then
  mkdir -p "$extract_dir"
  echo "Extracting..."
  tar -xzf "$archive_path" -C "$extract_dir"
fi

tor_bin="$(find "$extract_dir" -name "tor" -type f | head -n1)"
if [ -z "$tor_bin" ]; then
  echo "Couldn't find a 'tor' binary inside $extract_dir" >&2
  exit 1
fi

dest="$binaries_dir/tor-$triple"
cp "$tor_bin" "$dest"
chmod +x "$dest"

echo
echo "Done. Placed $dest"
