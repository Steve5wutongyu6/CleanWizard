#!/usr/bin/env bash
# scripts/common.sh — shared helpers, sourced by every CleanWizard script.
# Never executed directly.
#
# Design rules:
#   * No network requests to Tencent servers, ever. Nothing in this repository
#     contacts any Tencent service host.
#   * Every script is runnable from any working directory.
#   * Fails fast and loudly: an error means a non-zero exit, never a warning.

# shellcheck shell=bash

set -euo pipefail

cw_resolve_root() {
  local src="${BASH_SOURCE[0]}"
  while [ -L "${src}" ]; do
    local dir
    dir="$(cd -P "$(dirname "${src}")" && pwd)"
    src="$(readlink "${src}")"
    case "${src}" in
      /*) ;;
      *) src="${dir}/${src}" ;;
    esac
  done
  printf '%s' "$(cd -P "$(dirname "${src}")/.." && pwd)"
}

CW_ROOT="$(cw_resolve_root)"
export CW_ROOT
export CW_PROJECT="${CW_PROJECT:-${CW_ROOT}/CleanWizard.xcodeproj}"
export CW_SCHEME="${CW_SCHEME:-CleanWizard}"
export CW_TEST_SCHEME="${CW_TEST_SCHEME:-${CW_SCHEME}}"
export CW_TEST_DESTINATION="${CW_TEST_DESTINATION:-platform=macOS}"
export CW_APP_NAME="${CW_APP_NAME:-CleanWizard}"
export CW_APP_BUNDLE_ID="${CW_APP_BUNDLE_ID:-dev.cleanwizard.app}"
export CW_ENGINE_CRATE="${CW_ENGINE_CRATE:-cleanwizard_engine}"
export CW_BUILD_DIR="${CW_BUILD_DIR:-${CW_ROOT}/build}"
export CW_CONFIGURATION="${CW_CONFIGURATION:-Release}"
export CW_LOCK_DIR="${CW_BUILD_DIR}/.locks"
export CW_LOG_DIR="${CW_BUILD_DIR}/logs"

mkdir -p "${CW_BUILD_DIR}" "${CW_LOCK_DIR}" "${CW_LOG_DIR}"

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  C_BOLD=$'\033[1m'; C_DIM=$'\033[2m'; C_RED=$'\033[31m'
  C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'; C_BLUE=$'\033[34m'; C_OFF=$'\033[0m'
else
  C_BOLD=""; C_DIM=""; C_RED=""; C_GREEN=""; C_YELLOW=""; C_BLUE=""; C_OFF=""
fi

log()   { printf '%s%s%s\n' "${C_BLUE}" "$*" "${C_OFF}" >&2; }
step()  { printf '\n%s==> %s%s\n' "${C_BOLD}${C_BLUE}" "$*" "${C_OFF}" >&2; }
info()  { printf '%s  • %s%s\n' "${C_DIM}" "$*" "${C_OFF}" >&2; }
warn()  { printf '%s  ! %s%s\n' "${C_YELLOW}" "$*" "${C_OFF}" >&2; }
ok()    { printf '%s  ✓ %s%s\n' "${C_GREEN}" "$*" "${C_OFF}" >&2; }
die()   { printf '%s  ✗ %s%s\n' "${C_RED}" "$*" "${C_OFF}" >&2; exit 1; }

on_error() {
  local exit_code=$?
  printf '%s  ✗ %s failed (exit %s)%s\n' "${C_RED}" "${BASH_SOURCE[1]:-script}" "${exit_code}" "${C_OFF}" >&2
}
trap on_error ERR

# ---------------------------------------------------------------------------
# Requirements
# ---------------------------------------------------------------------------
require_cmd() {
  local cmd
  for cmd in "$@"; do
    command -v "${cmd}" >/dev/null 2>&1 || die "required command not found: ${cmd}"
  done
}

require_macos() {
  if [ "$(uname -s)" != "Darwin" ]; then
    die "CleanWizard builds on macOS only (this host is $(uname -s))"
  fi
}

require_xcode() {
  require_macos
  require_cmd xcrun xcodebuild lipo
  if ! xcodebuild -version >/dev/null 2>&1; then
    die "no usable Xcode toolchain; install Xcode and select it (DEVELOPER_DIR or xcode-select)"
  fi
}

require_rust() {
  require_macos
  require_cmd cargo rustc rustup
  local installed have_one=0
  installed="$(rustup target list --installed 2>/dev/null || true)"
  if printf '%s\n' "${installed}" | grep -qx aarch64-apple-darwin; then have_one=1; fi
  if printf '%s\n' "${installed}" | grep -qx x86_64-apple-darwin; then have_one=1; fi
  [ "${have_one}" -eq 1 ] || \
    die "no macOS Rust target installed; run: rustup target add aarch64-apple-darwin x86_64-apple-darwin"
}

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
product_path() {
  # product_path <Configuration> -> absolute path of CleanWizard.app
  printf '%s/%s/%s.app' "${CW_BUILD_DIR}" "$1" "${CW_APP_NAME}"
}

crate_dir() {
  printf '%s/engine' "${CW_ROOT}"
}

engine_include_dir() {
  printf '%s/include' "$(crate_dir)"
}

# ---------------------------------------------------------------------------
# Locking: serialises cargo builds and xcodebuild runs that share one output tree.
# ---------------------------------------------------------------------------
with_lock() {
  local name=$1; shift
  local lock_dir="${CW_LOCK_DIR}/${name}"
  local lock_pid="" waited_pid=""

  while ! mkdir "${lock_dir}" 2>/dev/null; do
    lock_pid="$(cat "${lock_dir}/pid" 2>/dev/null || true)"
    if [ -n "${lock_pid}" ] && ! kill -0 "${lock_pid}" 2>/dev/null; then
      warn "removing stale lock ${name} (owner pid ${lock_pid} is gone)"
      rm -rf "${lock_dir}"
      continue
    fi
    if [ -n "${lock_pid}" ] && [ "${lock_pid}" != "${waited_pid}" ]; then
      waited_pid="${lock_pid}"
      info "waiting for ${name} held by pid ${lock_pid}…"
    fi
    sleep 1
  done

  printf '%s' "$$" > "${lock_dir}/pid"
  # shellcheck disable=SC2064
  trap "rm -rf '${lock_dir}'" EXIT
  "$@"
  local rc=$?
  rm -rf "${lock_dir}"
  trap - EXIT
  return "${rc}"
}

# ---------------------------------------------------------------------------
# Rust engine static library
# ---------------------------------------------------------------------------

# Maps a Rust triple to the Xcode-style architecture name.
triple_to_arch() {
  case "$1" in
    aarch64-apple-darwin) printf 'arm64' ;;
    x86_64-apple-darwin)  printf 'x86_64' ;;
    *) die "unsupported Rust triple: $1" ;;
  esac
}

# Location of the staticlib cargo produces.
# engine_lib_path <triple> <cargo-profile>   (profile: debug or release)
engine_lib_path() {
  local triple=$1 profile=$2
  printf '%s' "${CW_ROOT}/engine/target/${triple}/${profile}/lib${CW_ENGINE_CRATE}.a"
}

# Build the Rust staticlib for one triple. Prints nothing on success.
# build_rust_for <triple> <debug|release>
build_rust_for() {
  local triple=$1 profile=$2
  local cargo_args=(--locked --target "${triple}")
  [ "${profile}" = "release" ] && cargo_args+=(--release)

  local lib
  lib="$(engine_lib_path "${triple}" "${profile}")"

  step "cargo build ${triple} (${profile})"
  (
    cd "$(crate_dir)"
    cargo build "${cargo_args[@]}"
  ) 2>&1 | tail -n 5 >&2 || true
  [ -f "${lib}" ] || die "static library was not produced: ${lib}"
  ok "built ${lib}"
}

# Build the engine for every architecture named in ARCHS (default: both).
# The App links one staticlib per architecture, see scripts/prepare.sh.
# build_engine <debug|release> [triples...]
build_engine() {
  local profile=$1; shift
  local triples=("$@")
  if [ "${#triples[@]}" -eq 0 ]; then
    triples=(aarch64-apple-darwin x86_64-apple-darwin)
  fi
  local triple
  for triple in "${triples[@]}"; do
    with_lock cargo-build-${triple} build_rust_for "${triple}" "${profile}"
  done
}

# ---------------------------------------------------------------------------
# xcodebuild wrappers
# ---------------------------------------------------------------------------
xcodebuild_logged() {
  local name=$1; shift
  local log="${CW_LOG_DIR}/${name}.log"
  step "xcodebuild ${name}"
  if xcodebuild "$@" > "${log}" 2>&1; then
    ok "xcodebuild ${name} succeeded"
    return 0
  fi
  printf '\n%s  ✗ xcodebuild %s failed; full log: %s%s\n' "${C_RED}" "${name}" "${log}" "${C_OFF}" >&2
  grep -nE 'error:|testing failed|TEST FAILED' "${log}" | tail -n 40 >&2 || true
  die "xcodebuild ${name} failed"
}

require_project() {
  [ -d "${CW_PROJECT}" ] || die "Xcode project not found: ${CW_PROJECT} (set CW_PROJECT or PROJECT to override)"
}

# ---------------------------------------------------------------------------
# Universal binary assembly
# ---------------------------------------------------------------------------
# Per-arch builds produce one thin executable each; this stitches them into the
# universal App binary that the shipped product needs.
# lipo_universal_product <Configuration> <arch> <arch> ...
lipo_universal_product() {
  local configuration=$1; shift
  local app
  app="$(product_path "${configuration}")"
  local exe="${app}/Contents/MacOS/${CW_APP_NAME}"
  [ -x "${exe}" ] || die "built executable missing: ${exe}"

  local arch_dir="${CW_BUILD_DIR}/${configuration}/universal-parts"
  rm -rf "${arch_dir}"; mkdir -p "${arch_dir}"

  local arch slice exe_slices=()
  if [ "$#" -eq 0 ]; then
    set -- arm64 x86_64
  fi
  for arch in "$@"; do
    slice="${arch_dir}/${CW_APP_NAME}.${arch}"
    if lipo -thin "${arch}" "${exe}" -output "${slice}" 2>/dev/null; then
      exe_slices+=( "${slice}" )
    else
      warn "executable has no ${arch} slice; skipping it"
    fi
  done
  [ "${#exe_slices[@]}" -gt 0 ] || die "no architecture slices found in ${exe}"

  lipo -create -output "${exe}" "${exe_slices[@]}"
  chmod +x "${exe}"
  lipo -info "${exe}" || true
  info "universal executable: $(lipo -archs "${exe}")"
}
