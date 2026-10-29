#!/usr/bin/env bash
# scripts/build-rust.sh — builds the Rust engine staticlib for one architecture.
# Invoked by the Xcode "Build Rust Engine" phase, which runs once per arch.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${CLEANWIZARD_RUST_PROFILE:-release}"
ARCH="${CURRENT_ARCH:-}"
if [ -z "${ARCH}" ] || [ "${ARCH}" = "undefined_arch" ]; then
  ARCH="$(uname -m)"
fi

case "${ARCH}" in
  arm64|arm64e) TARGET="aarch64-apple-darwin" ;;
  x86_64)       TARGET="x86_64-apple-darwin" ;;
  *) echo "build-rust: unsupported architecture '${ARCH}'" >&2; exit 1 ;;
esac

if [ "${PROFILE}" = "debug" ]; then
  CARGO_FLAGS=()
else
  CARGO_FLAGS=(--release)
fi

echo "build-rust: arch=${ARCH} target=${TARGET} profile=${PROFILE}"
command -v cargo >/dev/null 2>&1 || {
  echo "build-rust: cargo not found; install Rust (rustup.rs)" >&2
  exit 1
}

cd "${ROOT}/engine"
cargo build --target "${TARGET}" "${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"}"

LIB="target/${TARGET}/${PROFILE}/libcleanwizard_engine.a"
[ -f "${LIB}" ] || { echo "build-rust: missing ${LIB}" >&2; exit 1; }
mkdir -p "target/${PROFILE}"
cp "${LIB}" "target/${PROFILE}/libcleanwizard_engine.a"
echo "build-rust: staged target/${PROFILE}/libcleanwizard_engine.a"
