#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:-aarch64-apple-darwin}"
ARCH_LABEL="${2:-arm64}"
OUT_DIR="${3:-dist}"
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
APP_DIR="${OUT_DIR}/ChronoBase.app"
rm -rf "${APP_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS" "${APP_DIR}/Contents/Resources"

cp "${BIN_PATH}" "${APP_DIR}/Contents/MacOS/chronobase"
chmod +x "${APP_DIR}/Contents/MacOS/chronobase"
cp "crates/app/assets/Info.plist" "${APP_DIR}/Contents/Info.plist"
cp "crates/app/assets/icon.png" "${APP_DIR}/Contents/Resources/icon.png"

APP_TAR="${OUT_DIR}/ChronoBase-macos-${ARCH_LABEL}.app.tar.gz"
tar -czf "${APP_TAR}" -C "${OUT_DIR}" "ChronoBase.app"
test -f "${APP_TAR}"
echo "Created ${APP_TAR}"

DMG_STAGE="${OUT_DIR}/dmg-stage"
rm -rf "${DMG_STAGE}"
mkdir -p "${DMG_STAGE}"
cp -R "${APP_DIR}" "${DMG_STAGE}/"
ln -s /Applications "${DMG_STAGE}/Applications"
cp "README.md" "${DMG_STAGE}/README.md"
cp "LICENSE" "${DMG_STAGE}/LICENSE"

DMG_PATH="${OUT_DIR}/ChronoBase-macos-${ARCH_LABEL}.dmg"
rm -f "${DMG_PATH}"
hdiutil create -volname "ChronoBase" -srcfolder "${DMG_STAGE}" -ov -format UDZO "${DMG_PATH}"
test -f "${DMG_PATH}"
echo "Created ${DMG_PATH}"

