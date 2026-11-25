#!/usr/bin/env bash
# Tests for the functions in scripts/release.sh.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../release.sh"
RELEASE_SH=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/release.sh

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
fail() { echo "FAIL: $*" >&2; FAILS=$((FAILS + 1)); }

LINUX='linux={"include":[{"os":"ubuntu-24.04","target":"x86_64-unknown-linux-gnu"},{"os":"ubuntu-24.04-arm","target":"aarch64-unknown-linux-gnu"}]}'
WINDOWS_ROW='{"os":"windows-2025","target":"x86_64-pc-windows-msvc"}'
ARM_ROW='{"os":"macos-15","target":"aarch64-apple-darwin"}'
INTEL_ROW='{"os":"macos-15-intel","target":"x86_64-apple-darwin"}'
# A real minisign public key line, used only as well-formed input.
KEY=dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEVEQzZFQzUxQTUxRjFBMUUKUldRZUdoK2xVZXpHN2E2djZISU5UUlpWc0U0WjQ1NVY2a3UvQnpGcFhFQ2lGdm9Jd1l3akhwcFoK

test_release_sh_is_executable() {
  [[ -x $RELEASE_SH ]] || fail "scripts/release.sh is not executable"
}

test_check_ref_accepts_only_the_matching_tag() {
  check_ref tag v1.2.3 1.2.3 false || fail "matching tag refused"
  if check_ref tag v1.2.4 1.2.3 false 2>/dev/null; then fail "mismatched tag accepted"; fi
  if check_ref tag 1.2.3 1.2.3 false 2>/dev/null; then fail "tag without v accepted"; fi
  local msg; msg=$(check_ref tag v9.9.9 1.2.3 false 2>&1) || true
  [[ $msg == "release: tag v9.9.9 does not match the workspace version v1.2.3" ]] || fail "mismatch message: $msg"
}

test_check_ref_allows_a_branch_only_on_a_dry_run() {
  check_ref branch master 1.2.3 true || fail "dry run from a branch refused"
  if check_ref branch master 1.2.3 false 2>/dev/null; then fail "release from a branch accepted"; fi
  if check_ref branch master 1.2.3 "" 2>/dev/null; then fail "empty dry_run treated as a dry run"; fi
  if check_ref pull 1 1.2.3 true 2>/dev/null; then fail "unknown ref type accepted"; fi
}

test_check_keys_refuses_the_placeholder_and_an_empty_file() {
  printf '# comment\nPLACEMAT-UPDATE-KEY-PLACEHOLDER\n' >"$WORK/placeholder.txt"
  local msg
  if msg=$(check_keys "$WORK/placeholder.txt" 2>&1); then fail "placeholder accepted"; fi
  [[ $msg == *"still holds the placeholder key"* ]] || fail "placeholder message: $msg"
  printf '%s\nPLACEMAT-UPDATE-KEY-PLACEHOLDER\n' "$KEY" >"$WORK/both.txt"
  if check_keys "$WORK/both.txt" 2>/dev/null; then fail "placeholder beside a key accepted"; fi
  printf '# nothing\n\n' >"$WORK/empty.txt"
  if msg=$(check_keys "$WORK/empty.txt" 2>&1); then fail "empty file accepted"; fi
  [[ $msg == *"holds no update key"* ]] || fail "empty message: $msg"
  : >"$WORK/zero.txt"
  if check_keys "$WORK/zero.txt" 2>/dev/null; then fail "zero-byte file accepted"; fi
}

test_check_keys_refuses_a_line_that_is_not_a_key() {
  printf '%s\nbm90IGEga2V5Cg==\n' "$KEY" >"$WORK/bad.txt"
  local msg
  if msg=$(check_keys "$WORK/bad.txt" 2>&1); then fail "non-key line accepted"; fi
  [[ $msg == *"line 2 is not a minisign public key"* ]] || fail "bad-line message: $msg"
}

test_check_keys_accepts_a_key_and_the_committed_file() {
  printf '%s' "$KEY" >"$WORK/key.txt"
  check_keys "$WORK/key.txt" || fail "key without a final newline refused"
  check_keys "$(dirname "$RELEASE_SH")/../assets/update/keys.txt" || fail "the committed keys.txt refused"
}

test_select_targets_splits_linux_from_the_desktop_runners() {
  local want_all="$LINUX"$'\n'"desktop={\"include\":[$WINDOWS_ROW,$ARM_ROW,$INTEL_ROW]}"
  [[ $(select_targets "" false) == "$want_all" ]] || fail "empty, release: $(select_targets "" false)"
  [[ $(select_targets linux true) == "$LINUX"$'\n'"desktop=" ]] || fail "linux only: $(select_targets linux true)"
  [[ $(select_targets "macos-intel, windows" true) == "linux="$'\n'"desktop={\"include\":[$WINDOWS_ROW,$INTEL_ROW]}" ]] \
    || fail "subset: $(select_targets "macos-intel, windows" true)"
}

test_select_targets_refuses_unknown_and_partial_releases() {
  if select_targets "linux,beos" true 2>/dev/null; then fail "unknown platform accepted"; fi
  if select_targets " , " true 2>/dev/null; then fail "a list with no tokens accepted"; fi
  if select_targets "linux,windows,macos-arm64" false 2>/dev/null; then fail "a release without macos-intel accepted"; fi
  select_targets "windows,macos-intel,macos-arm64,linux" false >/dev/null || fail "a full release refused"
}

test_stable_name_drops_the_version() {
  [[ $(stable_name placemat_1.2.3_amd64.deb 1.2.3) == placemat_amd64.deb ]] || fail "deb"
  [[ $(stable_name Placemat_1.2.3_aarch64.dmg 1.2.3) == Placemat_aarch64.dmg ]] || fail "dmg"
  [[ -z $(stable_name SHA256SUMS 1.2.3) ]] || fail "SHA256SUMS got a stable name"
}

test_write_manifest_lists_every_app_target() {
  mkdir -p "$WORK/dist" "$WORK/update"
  local f
  for f in placemat_1.2.3_x86_64.AppImage placemat_1.2.3_aarch64.AppImage placemat_1.2.3_x64-setup.exe; do
    printf '%s' "$f" >"$WORK/dist/$f"
  done
  for f in Placemat_1.2.3_aarch64.app.tar.gz Placemat_1.2.3_x86_64.app.tar.gz; do
    printf '%s' "$f" >"$WORK/update/$f"
  done
  write_manifest "$WORK/dist" "$WORK/update" 1.2.3 https://example.invalid/v1.2.3/ || { fail "manifest failed"; return 0; }
  [[ $(jq -r .version "$WORK/update/latest.json") == 1.2.3 ]] || fail "manifest version"
  [[ $(jq -r '.targets | keys | join(",")' "$WORK/update/latest.json") == app-linux-aarch64,app-linux-x86_64,app-macos-aarch64,app-macos-x86_64,app-windows-x86_64 ]] \
    || fail "manifest targets: $(jq -c .targets "$WORK/update/latest.json")"
  [[ $(jq -r '.targets["app-windows-x86_64"].url' "$WORK/update/latest.json") == https://example.invalid/v1.2.3/placemat_1.2.3_x64-setup.exe ]] \
    || fail "manifest url"
  rm "$WORK/update/Placemat_1.2.3_x86_64.app.tar.gz" "$WORK/update/latest.json"
  if write_manifest "$WORK/dist" "$WORK/update" 1.2.3 https://example.invalid 2>/dev/null; then fail "missing file accepted"; fi
  [[ ! -e $WORK/update/latest.json ]] || fail "a failed manifest left latest.json"
}

for t in $(declare -F | awk '{print $3}' | grep '^test_'); do "$t"; done
if ((FAILS > 0)); then echo "$FAILS failure(s)" >&2; exit 1; fi
echo "release-test: ok"
