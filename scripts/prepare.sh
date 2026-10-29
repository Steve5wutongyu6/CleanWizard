#!/usr/bin/env bash
# scripts/prepare.sh — Rust link preparation for a single Xcode architecture.
#
# INTENT (for lead / xcodeproj wiring)
#   The App target's "Link Binary With Libraries" phase links
#       $(CW_RUST_LIB)   where   CW_RUST_LIB = $(SRCROOT)/engine/target/$(CLEANWIZARD_TRIPLE)/$(CONFIGURATION)/libcleanwizard_engine.a
#   but a single .a path cannot serve a multi-arch build. This script is meant to be
#   run as the FIRST Run Script build phase, exactly as lead described:
#
#       "$SRCROOT/scripts/prepare.sh" --arch "$CURRENT_ARCH" --configuration "$CONFIGURATION"
#
#   It maps ARCH -> Rust triple, verifies the staticlib for that slice exists and
#   fails with a copy-pasteable cargo command when it does not. In Xcode, cargo is
#   NOT invoked (the engine is built by scripts/build.sh and by CI before
#   xcodebuild), which keeps user script sandboxing irrelevant for this phase.
#
# Usage:
#   scripts/prepare.sh --arch arm64 --configuration Release
#   scripts/prepare.sh --check          # verify every arch/profile without building

# shellcheck source=common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

ARCH=""
CONFIGURATION="${CW_CONFIGURATION:-Release}"
CHECK_ONLY=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --arch)          shift; ARCH="${1:-}" ;;
    --configuration) shift; CONFIGURATION="${1:-}" ;;
    --check)         CHECK_ONLY=1 ;;
    -h|--help)       sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1 (see --help)" ;;
  esac
  shift
done

case "${ARCH}" in
  arm64|arm64e) TRIPLE=aarch64-apple-darwin ;;
  x86_64)       TRIPLE=x86_64-apple-darwin ;;
  "")           TRIPLE="" ;;  # --check mode
  *) die "unsupported architecture: ${ARCH}" ;;
esac

PROFILE=debug
[ "${CONFIGURATION}" = "Release" ] && PROFILE=release

verify_one() {
  local arch=$1 triple=$2 profile=$3
  local lib
  lib="$(engine_lib_path "${triple}" "${profile}")"
  if [ -f "${lib}" ]; then
    info "${arch}: ${lib}"
    return 0
  fi
  if [ "${CHECK_ONLY}" -eq 1 ]; then
    warn "${arch}: missing ${lib}"
    return 1
  fi
  cat >&2 <<EOF

  The Rust engine staticlib for ${arch} is missing:
      ${lib}

  Build it first, then re-run the Xcode build:

      cargo build --release --target ${triple}

EOF
  return 1
}

if [ "${CHECK_ONLY}" -eq 1 ]; then
  failed=0
  for arch in arm64 x86_64; do
    case "${arch}" in
      arm64) triple=aarch64-apple-darwin ;; x86_64) triple=x86_64-apple-darwin ;;
    esac
    verify_one "${arch}" "${triple}" "${PROFILE}" || failed=1
  done
  [ "${failed}" -eq 0 ] || die "engine libraries are incomplete (run: scripts/build.sh)"
  ok "engine staticlibs present for: arm64 x86_64"
  exit 0
fi

verify_one "${ARCH}" "${TRIPLE}" "${PROFILE}" || die "engine staticlib unavailable for ${ARCH}"
