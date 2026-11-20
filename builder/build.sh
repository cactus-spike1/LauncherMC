#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?Usage: release.sh <version> [create_release]}"
CREATE_RELEASE="${2:-true}"

REPO="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}"
: "${GH_TOKEN:?GH_TOKEN is not set}"

PACKAGE_VERSION="$(
  sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n 1
)"

if [[ "$VERSION" != "$PACKAGE_VERSION" ]]; then
  echo "Release version $VERSION does not match Cargo.toml version $PACKAGE_VERSION" >&2
  exit 1
fi

MAIN_TAG="v${VERSION}"
INSTALLER_TAG="v${VERSION}-installer"
WINDOWS_TARGET="x86_64-pc-windows-gnu"

cargo build --release --bins
cargo build --release --bins --target "$WINDOWS_TARGET"

rm -rf out
mkdir -p out/main out/installer

copy_binary() {
  local name="$1"
  local src_dir="$2"
  local suffix="$3"
  local output_dir="$4"
  local source="${src_dir}/${name}"

  if [[ -f "$source" ]]; then
    cp "$source" "out/${output_dir}/${name}${suffix}"
  elif [[ -f "${source}.exe" ]]; then
    cp "${source}.exe" "out/${output_dir}/${name}${suffix}"
  else
    echo "Binary not found: $source" >&2
    exit 1
  fi
}

copy_binary LauncherMC target/release "-linux" main
copy_binary LauncherMCcore target/release "-linux" main
copy_binary LauncherMC "target/${WINDOWS_TARGET}/release" "-windows.exe" main
copy_binary LauncherMCcore "target/${WINDOWS_TARGET}/release" "-windows.exe" main

copy_binary LauncherMCupdater target/release "-linux" installer
copy_binary LauncherMCupdater "target/${WINDOWS_TARGET}/release" "-windows.exe" installer

for file in out/main/* out/installer/*; do
  sha256sum "$file" > "${file}.sha256"
done

if [[ "$CREATE_RELEASE" == "true" ]]; then
  if ! gh release view "$MAIN_TAG" --repo "$REPO" >/dev/null 2>&1; then
    gh release create "$MAIN_TAG" \
      --title "$MAIN_TAG" \
      --notes "LauncherMC and LauncherMCcore ${VERSION}" \
      --repo "$REPO"
  fi

  if ! gh release view "$INSTALLER_TAG" --repo "$REPO" >/dev/null 2>&1; then
    gh release create "$INSTALLER_TAG" \
      --title "$INSTALLER_TAG" \
      --notes "LauncherMCupdater installer ${VERSION}" \
      --repo "$REPO"
  fi
fi

gh release upload "$MAIN_TAG" out/main/* --clobber --repo "$REPO"
gh release upload "$INSTALLER_TAG" out/installer/* --clobber --repo "$REPO"
