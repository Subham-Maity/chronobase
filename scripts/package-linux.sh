#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-x86_64-unknown-linux-gnu}"
OUT_DIR="${2:-dist}"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"

BIN_PATH="${TARGET_DIR}/${TARGET}/release/chronobase"
if [[ ! -f "${BIN_PATH}" ]]; then
  BIN_PATH="${TARGET_DIR}/release/chronobase"
fi
if [[ ! -f "${BIN_PATH}" ]]; then
  echo "Release binary not found at ${BIN_PATH}" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"
STAGE="${OUT_DIR}/ChronoBase-linux-x86_64"
rm -rf "${STAGE}"
mkdir -p "${STAGE}"

cp "${BIN_PATH}" "${STAGE}/chronobase"
chmod +x "${STAGE}/chronobase"
cp "crates/app/assets/chronobase.desktop" "${STAGE}/chronobase.desktop"
cp "crates/app/assets/icon.png" "${STAGE}/chronobase.png"
cp "README.md" "${STAGE}/README.md"
cp "LICENSE" "${STAGE}/LICENSE"
cp "crates/app/ui/fonts/OFL.txt" "${STAGE}/FONT-LICENSE-OFL.txt"
printf "Built with Slint (https://slint.dev) — Licensed under Slint Royalty-Free Desktop License v2.0 / GNU GPLv3.\n" > "${STAGE}/SLINT-ATTRIBUTION.txt"

TAR_PATH="${OUT_DIR}/ChronoBase-linux-x86_64.tar.gz"
tar -czf "${TAR_PATH}" -C "${OUT_DIR}" "ChronoBase-linux-x86_64"
test -f "${TAR_PATH}"
echo "Created ${TAR_PATH}"

# Build .deb package via dpkg-deb (always available on Ubuntu/Debian runners without extra cargo tools)
if command -v dpkg-deb >/dev/null 2>&1; then
  VERSION="$(awk -F'"' '/^version =/{print $2; exit}' Cargo.toml || true)"
  VERSION="${VERSION:-0.1.0}"
  DEB_ROOT="${OUT_DIR}/deb-pkg"
  rm -rf "${DEB_ROOT}"
  mkdir -p "${DEB_ROOT}/DEBIAN" \
           "${DEB_ROOT}/usr/bin" \
           "${DEB_ROOT}/usr/share/applications" \
           "${DEB_ROOT}/usr/share/icons/hicolor/256x256/apps" \
           "${DEB_ROOT}/usr/share/doc/chronobase"

  cp "${BIN_PATH}" "${DEB_ROOT}/usr/bin/chronobase"
  chmod 0755 "${DEB_ROOT}/usr/bin/chronobase"
  cp "crates/app/assets/chronobase.desktop" "${DEB_ROOT}/usr/share/applications/chronobase.desktop"
  cp "crates/app/assets/icon.png" "${DEB_ROOT}/usr/share/icons/hicolor/256x256/apps/chronobase.png"
  cp "LICENSE" "${DEB_ROOT}/usr/share/doc/chronobase/LICENSE"

  cat > "${DEB_ROOT}/DEBIAN/control" <<EOF
Package: chronobase
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: amd64
Depends: libfontconfig1, libxkbcommon0, libxkbcommon-x11-0, libgl1
Maintainer: Subham Maity <https://github.com/Subham-Maity>
Description: Local-first native backup manager for PostgreSQL and MongoDB
 Built with Slint (https://slint.dev).
EOF

  DEB_PATH="${OUT_DIR}/ChronoBase-linux-x86_64.deb"
  dpkg-deb --build "${DEB_ROOT}" "${DEB_PATH}"
  test -f "${DEB_PATH}"
  echo "Created ${DEB_PATH}"
fi

