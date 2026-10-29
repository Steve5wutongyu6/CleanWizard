#!/usr/bin/env bash
# scripts/xcode-env.sh — select the Xcode used by every build and test.
#
# Source of truth: DEVELOPER_DIR (set by CI), then CW_XCODE (explicit override),
# else the toolchain currently selected by xcode-select.
#
# Why this exists: CleanWizard is built with the macOS 27 SDK. A GitHub macOS 26
# runner image ships Xcode 26 by default and may install Xcode 27 alongside it;
# building with the image default would link against the wrong SDK and fail the
# MACOSX_DEPLOYMENT_TARGET=27.0 check. This script pins the toolchain by version
# before anything else runs.
#
# Usage:
#   scripts/xcode-env.sh            # print the selected developer dir and version
#   eval "$(scripts/xcode-env.sh)"  # export DEVELOPER_DIR for the calling shell
#
# Environment:
#   CW_XCODE            preferred Xcode path, or a major version like 27 or 27.0
#   CW_REQUIRED_MAJOR   minimum acceptable Xcode major version (default: 27)
#   CW_SDK              required SDK name (default: macosx27.0)

# shellcheck source=common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

CW_REQUIRED_MAJOR="${CW_REQUIRED_MAJOR:-27}"
CW_SDK="${CW_SDK:-macosx${CW_REQUIRED_MAJOR}.0}"

xcode_major() {
  # xcode_major <path-to-Xcode.app> -> prints major version, fails if unusable
  local app=$1
  local plist="${app}/Contents/version.plist"
  local short
  if [ -f "${plist}" ]; then
    short="$(plutil -extract CFBundleShortVersionString raw -o - "${plist}" 2>/dev/null || true)"
  else
    short=""
  fi
  if [ -z "${short}" ]; then
    short="$(DEVELOPER_DIR="${app}/Contents/Developer" xcodebuild -version 2>/dev/null | awk '/Xcode/ {print $2}')"
  fi
  printf '%s' "${short%%.*}"
}

candidate_xcodes() {
  # Every Xcode-like app on the machine, newest-looking last.
  local candidates=("${1:-}" "/Applications/Xcode.app" "/Applications/Xcode_${CW_REQUIRED_MAJOR}.app")
  local extra
  for extra in /Applications/Xcode*.app /Applications/Xcode-*.app; do
    [ -d "${extra}" ] && candidates+=( "${extra}" )
  done
  # De-duplicate, preserving order.
  printf '%s\n' "${candidates[@]}" | awk 'NF && !seen[$0]++'
}

resolve_developer_dir() {
  local requested="${CW_XCODE:-}"

  # An explicit DEVELOPER_DIR always wins: the caller (CI, or a developer) knows best.
  if [ -n "${DEVELOPER_DIR:-}" ]; then
    [ -d "${DEVELOPER_DIR}" ] || die "DEVELOPER_DIR points at a missing directory: ${DEVELOPER_DIR}"
    printf '%s' "${DEVELOPER_DIR}"
    return 0
  fi

  # CW_XCODE may be a version ("27", "27.0") or an Xcode path.
  if [ -n "${requested}" ] && [ -d "${requested}" ]; then
    if [ -d "${requested}/Contents/Developer" ]; then
      printf '%s' "${requested}/Contents/Developer"
      return 0
    fi
    die "CW_XCODE does not look like an Xcode.app: ${requested}"
  fi

  local dev app major best="" best_major=0
  while IFS= read -r app; do
    [ -d "${app}" ] || continue
    [ -d "${app}/Contents/Developer" ] || continue
    major="$(xcode_major "${app}")" || continue
    case "${major}" in ''|*[!0-9]*) continue ;; esac
    if [ -n "${requested}" ]; then
      # Prefer an exact major match; otherwise fall through to the newest.
      [ "${major}" = "${requested}" ] && { printf '%s' "${app}/Contents/Developer"; return 0; }
    fi
    if [ "${major}" -gt "${best_major}" ]; then
      best_major="${major}"; best="${app}/Contents/Developer"
    fi
  done < <(candidate_xcodes)
  if [ -z "${requested}" ] && [ -n "${best}" ]; then
    printf '%s' "${best}"
    return 0
  fi

  # Fall back to whatever xcode-select has.
  dev="$(xcode-select -p 2>/dev/null || true)"
  [ -n "${dev}" ] && [ -d "${dev}" ] || die "no Xcode installation found under /Applications"
  printf '%s' "${dev}"
}

DEV="$(resolve_developer_dir)"
export DEVELOPER_DIR="${DEV}"

[ -d "${DEVELOPER_DIR}" ] || die "resolved DEVELOPER_DIR is not a directory: ${DEVELOPER_DIR}"

version_line="$(xcodebuild -version 2>/dev/null)" || \
  die "xcodebuild is not usable with DEVELOPER_DIR=${DEVELOPER_DIR}"
major="$(printf '%s' "${version_line}" | awk '/Xcode/ {print $2}' | cut -d. -f1)"
case "${major}" in ''|*[!0-9]*) die "could not parse ${version_line}" ;; esac
if [ "${major}" -lt "${CW_REQUIRED_MAJOR}" ]; then
  # CI images lag behind the newest Xcode. Callers that only need a green build
  # may opt into the older toolchain; the App then deploys to that SDK's floor.
  if [ "${CW_ALLOW_OLDER_XCODE:-0}" = "1" ]; then
    warn "Xcode ${major} is older than ${CW_REQUIRED_MAJOR}; building with the available toolchain"
  else
    die "Xcode ${major} is too old for CleanWizard, need >= ${CW_REQUIRED_MAJOR}; set CW_XCODE or DEVELOPER_DIR"
  fi
fi

described_sdks="$(xcodebuild -showsdks 2>/dev/null || true)"
if ! printf '%s\n' "${described_sdks}" | grep -q "macosx${major}.0"; then
  warn "this Xcode does not advertise a macOS SDK; verify the Command Line Tools selection"
elif ! printf '%s\n' "${described_sdks}" | grep -q "${CW_SDK}"; then
  warn "SDK ${CW_SDK} was not listed; the project will build against the default SDK"
fi

# stdout carries only machine-readable output (for eval), diagnostics go to stderr.
printf 'DEVELOPER_DIR=%s\n' "${DEVELOPER_DIR}"
printf '# %s (%s)\n' "${version_line}" "${DEVELOPER_DIR}" >&2
