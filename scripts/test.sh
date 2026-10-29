#!/usr/bin/env bash
# scripts/test.sh — full local test run.
#
#   1. cargo test for the Rust engine
#   2. xcodebuild test for the App unit tests
#
# Usage:
#   scripts/test.sh            # engine + App unit tests
#   scripts/test.sh engine     # Rust only
#   scripts/test.sh app        # App unit tests only
#
# The same commands run in CI on a Release tag (see .github/workflows/release.yml).

# shellcheck source=common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

WHAT="${1:-all}"
case "${WHAT}" in
  all|engine|app) ;;
  -h|--help) sed -n '2,9p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) die "unknown target: ${WHAT} (expected: all, engine, app)" ;;
esac

FAILED=0
FAILED_SUITES=""

record_failure() {
  FAILED=1
  FAILED_SUITES="${FAILED_SUITES} $1"
}

run_engine_tests() {
  require_rust
  step "cargo test (engine)"
  local log="${CW_LOG_DIR}/cargo-test-engine.log"
  mkdir -p "$(dirname "${log}")"
  if (cd "$(crate_dir)" && cargo test --locked --all-targets > "${log}" 2>&1); then
    ok "cargo test passed"
  else
    warn "cargo test failed (log: ${log})"
    grep -nE 'error|FAILED|panicked' "${log}" | tail -n 30 >&2 || true
    record_failure engine
  fi
}

run_app_tests() {
  require_xcode
  require_project
  eval "$("${CW_ROOT}/scripts/xcode-env.sh")"
  build_engine debug

  step "xcodebuild test (App unit tests)"
  local log="${CW_LOG_DIR}/xcodebuild-test.log"
  local result="${CW_BUILD_DIR}/TestResults-${CW_SCHEME}.xcresult"
  rm -rf "${result}"
  # CODE_SIGN_IDENTITY=- keeps unit tests runnable without a developer account, as
  # specified for the project. UI tests are not part of the unit test bundle.
  if xcodebuild \
      -project "${CW_PROJECT}" \
      -scheme "${CW_TEST_SCHEME}" \
      -configuration Debug \
      -destination "${CW_TEST_DESTINATION}" \
      -derivedDataPath "${CW_BUILD_DIR}/DerivedData-tests" \
      -xcconfig "${CW_ROOT}/Configs/Debug.xcconfig" \
      -resultBundlePath "${result}" \
      CODE_SIGN_IDENTITY=- \
      test > "${log}" 2>&1; then
    ok "xcodebuild test passed (bundle: ${result})"
  else
    warn "xcodebuild test failed (log: ${log})"
    grep -nE 'error:|testing failed|TEST FAILED' "${log}" | tail -n 30 >&2 || true
    record_failure app
  fi
}

if [ "${WHAT}" = "all" ] || [ "${WHAT}" = "engine" ]; then
  run_engine_tests
fi
if [ "${WHAT}" = "all" ] || [ "${WHAT}" = "app" ]; then
  run_app_tests
fi

if [ "${FAILED}" -ne 0 ]; then
  die "test suite failed:${FAILED_SUITES}"
fi
step "All tests passed"
