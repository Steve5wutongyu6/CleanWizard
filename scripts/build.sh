#!/usr/bin/env bash
# scripts/build.sh — one-shot local build.
#
#   1. select Xcode 27 (macOS 27 SDK)
#   2. cargo build the Rust engine staticlib for every requested architecture
#   3. xcodebuild archive (or build) the App per architecture, ad-hoc signed
#   4. verify the product
#
# Usage:
#   scripts/build.sh              # Release archive (host architecture)
#   scripts/build.sh --universal  # Release archive, arm64 + x86_64
#   scripts/build.sh --debug      # Debug build (no archive, fastest feedback)
#
# Environment:
#   CW_CONFIGURATION, CODE_SIGN_IDENTITY, DEVELOPMENT_TEAM, CW_BUILD_DIR

# shellcheck source=common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

CONFIGURATION="${CW_CONFIGURATION:-Release}"
ARCHS=""
MODE="archive"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --debug)     CONFIGURATION=Debug;   MODE=build ;;
    --release)   CONFIGURATION=Release; MODE=archive ;;
    --universal) ARCHS="arm64 x86_64" ;;
    --arch)      shift; ARCHS="${1:-}" ;;
    -h|--help)
      sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1 (see --help)" ;;
  esac
  shift
done

if [ -z "${ARCHS}" ]; then
  ARCHS="$(uname -m)"
  info "building the host architecture only (${ARCHS}); pass --universal for arm64 + x86_64"
fi
read -r -a ARCHS_WORD <<< "${ARCHS}"

require_xcode
require_rust
require_project

eval "$("${CW_ROOT}/scripts/xcode-env.sh")"

step "CleanWizard ${CONFIGURATION} build"
info "architectures: ${ARCHS}"

# --- 1. Rust engine -----------------------------------------------------------
RUST_PROFILE=debug
[ "${CONFIGURATION}" = "Release" ] && RUST_PROFILE=release

triples=()
for arch in "${ARCHS_WORD[@]}"; do
  case "${arch}" in
    arm64)  triples+=("aarch64-apple-darwin") ;;
    x86_64) triples+=("x86_64-apple-darwin") ;;
    *) die "unsupported architecture: ${arch}" ;;
  esac
done
build_engine "${RUST_PROFILE}" "${triples[@]}"

# --- 2. xcodebuild ------------------------------------------------------------
# One thin build per architecture: the App links the per-arch staticlib produced
# above, so each slice is archived separately and merged by scripts/package-dmg.sh.
common_args=(
  -project "${CW_PROJECT}"
  -scheme "${CW_SCHEME}"
  -configuration "${CONFIGURATION}"
  CODE_SIGN_IDENTITY="${CODE_SIGN_IDENTITY:--}"
  MACOSX_DEPLOYMENT_TARGET="${CW_DEPLOYMENT_TARGET:-27.0}"
)
if [ -n "${DEVELOPMENT_TEAM:-}" ]; then
  common_args+=( DEVELOPMENT_TEAM="${DEVELOPMENT_TEAM}" )
fi

for arch in "${ARCHS_WORD[@]}"; do
  # One slice per arch. -destination already pins the architecture, so -arch is
  # omitted; only the derived-data path differs per slice.
  if [ "${#ARCHS_WORD[@]}" -eq 1 ]; then
    common_args+=( -destination "platform=macOS,arch=${arch}" ARCHS="${arch}" )
  else
    common_args+=( ARCHS="${ARCHS}" )
  fi
  common_args+=( -derivedDataPath "${CW_BUILD_DIR}/DerivedData-${arch}" )

  # prepare.sh fails early with a copy-pasteable cargo command when the per-arch
  # staticlib is missing. Inside Xcode it runs as the first Run Script phase.
  "${CW_ROOT}/scripts/prepare.sh" --arch "${arch}" --configuration "${CONFIGURATION}"

  if [ "${MODE}" = "archive" ]; then
    xcodebuild_logged "archive-${CONFIGURATION}-${arch}" \
      archive "${common_args[@]}" \
      -archivePath "${CW_BUILD_DIR}/CleanWizard-${arch}.xcarchive"
  else
    xcodebuild_logged "build-${CONFIGURATION}-${arch}" \
      build "${common_args[@]}"
  fi
done

# --- 3. Verify the product ----------------------------------------------------
verify_app() {
  local app=$1
  [ -d "${app}" ] || die "product not found: ${app}"
  if lipo -archs "${app}/Contents/MacOS/${CW_APP_NAME}" >/dev/null 2>&1; then
    info "architectures: $(lipo -archs "${app}/Contents/MacOS/${CW_APP_NAME}")"
  fi
  codesign --verify --deep --strict "${app}" 2>/dev/null \
    || warn "code signature check skipped or failed (ad-hoc builds verify only on the build host)"
  ok "${app}"
}

if [ "${MODE}" = "archive" ] && [ "${#ARCHS_WORD[@]}" -eq 1 ]; then
  # A single-arch archive is a complete product: copy it to the documented location.
  # exportArchive offers no "copy" mode, so a Developer ID re-sign happens later in
  # scripts/publish-github.sh, not here.
  src="${CW_BUILD_DIR}/CleanWizard-${ARCHS_WORD[0]}.xcarchive/Products/Applications/${CW_APP_NAME}.app"
  APP="$(product_path "${CONFIGURATION}")"
  rm -rf "${APP}"
  mkdir -p "$(dirname "${APP}")"
  ditto "${src}" "${APP}"
  verify_app "${APP}"
elif [ "${MODE}" = "archive" ]; then
  for arch in "${ARCHS_WORD[@]}"; do
    verify_app "${CW_BUILD_DIR}/CleanWizard-${arch}.xcarchive/Products/Applications/${CW_APP_NAME}.app"
  done
  warn "per-arch archives kept; run scripts/package-dmg.sh to merge them into a universal App"
else
  verify_app "$(product_path "${CONFIGURATION}")"
fi

step "Build complete"
info "logs: ${CW_LOG_DIR}"
