#!/usr/bin/env bash
# scripts/lint.sh — repository hygiene checks that need no Xcode build.
#
#   * bash -n on every script
#   * offline policy: no Tencent host names anywhere in the sources
#   * offline policy: the App target contains no remote URL usage
#
# Usage: scripts/lint.sh

# shellcheck source=common.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

step "shell syntax"
for script in "${CW_ROOT}/scripts/"*.sh; do
  bash -n "${script}" || die "shell syntax error in ${script}"
done
ok "all scripts pass bash -n"

# Tracked source text only (build products and cargo target dirs are excluded).
SOURCE_TODOS="${CW_BUILD_DIR}/lint-sources.txt"
find "${CW_ROOT}" -type f \
  \( -name '*.swift' -o -name '*.m' -o -name '*.mm' -o -name '*.h' \
     -o -name '*.hpp' -o -name '*.rs' -o -name '*.sh' -o -name '*.plist' \
     -o -name '*.yml' -o -name '*.yaml' -o -name '*.xcconfig' -o -name '*.c' \
     -o -name 'Cargo.toml' \) \
  -not -path '*/build/*' -not -path '*/target/*' -not -path '*/.git/*' \
  > "${SOURCE_TODOS}"
info "$(wc -l < "${SOURCE_TODOS}" | tr -d ' ') source files scanned"

step "offline policy: no Tencent endpoints"
# Tencent service host names are forbidden as request targets, anywhere.
hits="$(xargs grep -nE 'lemon\.guanjia\.qq\.com|(^|[^a-z])qq\.com' \
  < "${SOURCE_TODOS}" 2>/dev/null | grep -vE 'scripts/lint.sh|docs/' || true)"
if [ -n "${hits}" ]; then
  printf '%s\n' "${hits}" >&2
  die "forbidden endpoint found; CleanWizard never contacts Tencent servers"
fi
ok "no Tencent endpoints in sources"

step "offline policy: no remote URLs in the App"
if [ -d "${CW_ROOT}/CleanWizard" ]; then
  hits="$(grep -rnE 'URLSession|NSURLSession|NSURLRequest' "${CW_ROOT}/CleanWizard" 2>/dev/null || true)"
  if [ -n "${hits}" ]; then
    printf '%s\n' "${hits}" >&2
    die "networking API used in the App target (offline-only product)"
  fi
  ok "App target uses no networking APIs"
else
  warn "App source directory not present yet; skipped"
fi

step "workflow trigger check"
workflow="${CW_ROOT}/.github/workflows/release.yml"
if [ -f "${workflow}" ]; then
  yq -e '.on.push.tags' "${workflow}" >/dev/null 2>&1 \
    || die "release workflow must trigger on push tags"
  yq -e '.on.workflow_dispatch' "${workflow}" >/dev/null 2>&1 \
    || die "release workflow must support workflow_dispatch"
  if yq -e '.on.pull_request' "${workflow}" >/dev/null 2>&1; then
    die "release workflow must not trigger on pull_request"
  fi
  if yq -e '.on.push.branches' "${workflow}" >/dev/null 2>&1; then
    die "release workflow must not trigger on branch pushes"
  fi
  ok "release workflow triggers only on v* tags and manual dispatch"
else
  warn "release workflow not present; skipped"
fi

step "lint finished"