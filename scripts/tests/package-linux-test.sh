#!/usr/bin/env bash
# Tests for the helper functions in scripts/package-linux.sh.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../package-linux.sh"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
fail() { echo "FAIL: $*" >&2; FAILS=$((FAILS + 1)); }

test_deb_layout_names_each_missing_file() {
  local data=$WORK/data f
  for f in usr/bin/placemat usr/share/applications/placemat.desktop usr/lib/placemat/Inter-OFL.txt \
    usr/lib/placemat/Lucide-ISC.txt usr/lib/placemat/THIRD-PARTY-NOTICES.html; do
    mkdir -p "$data/$(dirname "$f")" && : >"$data/$f"
  done
  [[ -z $(deb_layout_missing "$data") ]] || fail "complete layout reported: $(deb_layout_missing "$data")"
  rm "$data/usr/lib/placemat/THIRD-PARTY-NOTICES.html"
  [[ $(deb_layout_missing "$data") == usr/lib/placemat/THIRD-PARTY-NOTICES.html ]] || fail "missing notices not named"
}

test_desktop_entry_needs_the_window_class_and_icon() {
  printf '[Desktop Entry]\nIcon=placemat\nStartupWMClass=placemat\n' >"$WORK/ok.desktop"
  desktop_entry_ok "$WORK/ok.desktop" placemat || fail "complete entry refused"
  printf '[Desktop Entry]\nIcon=placemat\n' >"$WORK/bad.desktop"
  if desktop_entry_ok "$WORK/bad.desktop" placemat 2>/dev/null; then fail "entry without StartupWMClass accepted"; fi
}

test_missing_depends_skips_the_c_runtime_and_names_the_rest() {
  [[ -z $(missing_depends "libc.so.6 libvulkan.so.1" "libvulkan1, libegl1") ]] || fail "declared library reported"
  [[ $(missing_depends "libEGL.so.1" "libvulkan1") == "libEGL.so.1 (libegl1)" ]] || fail "undeclared library not named"
  [[ $(missing_depends "libfoo.so.3" "") == "libfoo.so.3 (unknown)" ]] || fail "unknown library not named"
}

test_write_sums_covers_the_packages_only() {
  mkdir -p "$WORK/out"
  printf a >"$WORK/out/placemat_1.0.0_amd64.deb"
  printf b >"$WORK/out/THIRD-PARTY-NOTICES.html"
  write_sums "$WORK/out" || fail "write_sums failed"
  [[ $(cut -d' ' -f3 "$WORK/out/SHA256SUMS") == placemat_1.0.0_amd64.deb ]] || fail "sums: $(cat "$WORK/out/SHA256SUMS")"
  mkdir -p "$WORK/empty"
  if write_sums "$WORK/empty" 2>/dev/null; then fail "sums over nothing accepted"; fi
  [[ ! -e $WORK/empty/SHA256SUMS ]] || fail "empty SHA256SUMS left behind"
}

for t in $(declare -F | awk '{print $3}' | grep '^test_'); do "$t"; done
if ((FAILS > 0)); then echo "$FAILS failure(s)" >&2; exit 1; fi
echo "package-linux-test: ok"
