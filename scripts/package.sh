#!/usr/bin/env bash
# scripts/package.sh — turn a built App into distributable artifacts.
#
#   build/Release/CleanWizard.app            (input, from scripts/build.sh)
#   dist/CleanWizard-<version>-<arch>.zip    (plain zip, keeps the signature)
#   dist/CleanWizard-<version>-<arch>.dmg    (drag-to-Applications disk image)
#   dist/SHA256SUMS                          (checksums for both)
#
# Uses hdiutil only: no third-party disk-image tooling is installed or fetched.
#
# Usage:
#   scripts/package.sh [--version 1.2.3] [--app path/to/CleanWizard.app]
#
# Environment:
#   CW_VERSION (default: git tag without the leading "v", else 0.0.0-dev)

# shellcheck source=common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

VERSION="${CW_VERSION:-}"
APP=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) shift; VERSION="${1:-}" ;;
    --app)     shift; APP="${1:-}" ;;
    -h|--help) sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1 (see --help)" ;;
  esac
  shift
done

if [ -z "${VERSION}" ]; then
  VERSION="$(git -C "${CW_ROOT}" describe --tags --exact-match 2>/dev/null || true)"
fi
VERSION="${VERSION#v}"
[ -n "${VERSION}" ] || VERSION="0.0.0-dev"
info "version: ${VERSION}"

if [ -z "${APP}" ]; then
  APP="$(product_path "${CW_CONFIGURATION}")"
fi
[ -d "${APP}" ] || die "App not found: ${APP} (run scripts/build.sh first)"

BUNDLE_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
  "${APP}/Contents/Info.plist" 2>/dev/null || true)"
info "bundle version: ${BUNDLE_VERSION:-unset (ad-hoc dev build)}"

ARCH_NAME="$(lipo -archs "${APP}/Contents/MacOS/${CW_APP_NAME}")"
ARCH_SLUG="${ARCH_NAME// /-}"
info "architectures: ${ARCH_NAME}"

DIST="${CW_ROOT}/dist"
rm -rf "${DIST}"
mkdir -p "${DIST}"

step "zip"
# ditto preserves resource forks, symlinks and the code signature; zip(1) does not.
ZIP="${DIST}/${CW_APP_NAME}-${VERSION}-${ARCH_SLUG}.zip"
ditto -c -k --sequesterRsrc --keepParent "${APP}" "${ZIP}"
ok "${ZIP} ($(du -h "${ZIP}" | cut -f1 | tr -d ' '))"

step "dmg"
STAGE="${CW_BUILD_DIR}/dmg"
rm -rf "${STAGE}"
mkdir -p "${STAGE}"
ditto "${APP}" "${STAGE}/${CW_APP_NAME}.app"
ln -s /Applications "${STAGE}/Applications"

DMG="${DIST}/${CW_APP_NAME}-${VERSION}-${ARCH_SLUG}.dmg"
if hdiutil create -quiet -srcfolder "${STAGE}" \
     -volname "${CW_APP_NAME} ${VERSION}" -fs HFS+ -format UDZO "${DMG}"; then
  ok "${DMG} ($(du -h "${DMG}" | cut -f1 | tr -d ' '))"
else
  die "dmg creation failed"
fi

step "checksums"
shasum -a 256 "${ZIP}" "${DMG}" > "${DIST}/SHA256SUMS"
ok "${DIST}/SHA256SUMS"

step "artifacts"
ls -la "${DIST}" >&2
