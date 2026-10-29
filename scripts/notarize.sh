#!/usr/bin/env bash
# scripts/notarize.sh — Developer ID sign, notarize and staple a packaged disk image.
#
# Optional. Local builds and CI produce ad-hoc signed artifacts when no credentials
# exist (see .github/workflows/release.yml); this upgrades a .dmg once credentials
# are configured.
#
# Local setup:
#   xcrun notarytool store-credentials "cleanwizard-notary" \
#     --apple-id you@example.com --team-id TEAMID --password APP_SPECIFIC_PASSWORD
#
# CI environment (any one authentication route):
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD         Apple ID
#   APPSTORE_CONNECT_KEY_PATH, APPSTORE_CONNECT_KEY_ID,
#     APPSTORE_CONNECT_ISSUER_ID                        App Store Connect API key
#
# Notarization also needs a real "Developer ID Application" identity. In CI it is
# imported from a base64 PKCS#12 (MACOS_CERTIFICATE, MACOS_CERTIFICATE_PWD) into a
# temporary keychain before this script runs.
#
# Usage:
#   scripts/notarize.sh --dmg "dist/CleanWizard-1.2.3-arm64.dmg"

# shellcheck source=common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

DMG=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dmg) shift; DMG="${1:-}" ;;
    -h|--help) sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1 (see --help)" ;;
  esac
  shift
done

require_macos
require_cmd xcrun
[ -f "${DMG}" ] || die "dmg not found: ${DMG:-<none>}"

NOTARY_PROFILE="${NOTARY_PROFILE:-cleanwizard-notary}"
auth_args=()

if [ -n "${APPSTORE_CONNECT_KEY_PATH:-}" ]; then
  auth_args=( -authentication-key "${APPSTORE_CONNECT_KEY_PATH}" )
  [ -n "${APPSTORE_CONNECT_KEY_ID:-}" ] && auth_args+=( -authentication-key-id "${APPSTORE_CONNECT_KEY_ID}" )
  [ -n "${APPSTORE_CONNECT_ISSUER_ID:-}" ] && auth_args+=( -issuer "${APPSTORE_CONNECT_ISSUER_ID}" )
  phase=2
elif [ -n "${APPLE_ID:-}" ]; then
  xcrun notarytool store-credentials "${NOTARY_PROFILE}" \
    --apple-id "${APPLE_ID}" --team-id "${APPLE_TEAM_ID}" \
    --password "${APPLE_APP_PASSWORD}" >/dev/null
  auth_args=( -keychain-profile "${NOTARY_PROFILE}" )
  phase=2
else
  warn "no credentials in the environment; trying keychain profile ${NOTARY_PROFILE}"
  auth_args=( -keychain-profile "${NOTARY_PROFILE}" )
  phase=1
fi

if [ "${phase}" -eq 1 ]; then
  xcrun notarytool history "${auth_args[@]}" >/dev/null 2>&1 \
    || die "notarytool has no usable credentials; configure a profile or CI secrets"
fi

step "notarize $(basename "${DMG}")"
xcrun notarytool submit "${DMG}" "${auth_args[@]}" --wait --timeout 60m

step "staple"
xcrun stapler staple "${DMG}"
xcrun stapler validate "${DMG}"
ok "notarized and stapled: ${DMG}"
