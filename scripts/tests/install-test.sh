#!/usr/bin/env bash
# Tests for scripts/install.sh, run against a temporary data home with cargo skipped or faked.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
INSTALL_SH=$ROOT/scripts/install.sh

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
fail() { echo "FAIL: $*" >&2; FAILS=$((FAILS + 1)); }

SIZES=(16 24 32 48 64 128 256 512)

# Run install.sh ACTION in a fresh case directory DIR with cargo skipped, the data home in
# DIR/data and PREFIX (relative to DIR) as the install root; output goes to DIR/out.
run_in() {
  local dir=$1 action=$2 prefix=$3
  mkdir -p "$dir"
  (cd "$dir" && env -u PREFIX PLACEMAT_INSTALL_SKIP_CARGO=1 XDG_DATA_HOME="$dir/data" \
    PLACEMAT_PREFIX="$prefix" bash "$INSTALL_SH" "$action" >"$dir/out" 2>&1 </dev/null)
}

entry() { cat "$1/data/applications/placemat.desktop"; }
has_line() { grep -qxF -- "$2" <(entry "$1"); }

# The validator is optional on the host; when present the entry must pass it silently.
validate() {
  local file=$1/data/applications/placemat.desktop got
  command -v desktop-file-validate >/dev/null || return 0
  got=$(desktop-file-validate "$file" 2>&1) || { fail "desktop-file-validate: $got"; return; }
  [[ -z $got ]] || fail "desktop-file-validate warned: $got"
}

test_install_writes_the_entry() {
  local d=$WORK/entry line
  run_in "$d" install prefix || { fail "install failed: $(cat "$d/out")"; return; }
  for line in "[Desktop Entry]" "Type=Application" "Name=Placemat" \
    "Comment=A repo-native whiteboard, diagram and notes app" "Exec=\"$d/prefix/bin/placemat\"" \
    "Icon=placemat" "StartupWMClass=placemat" "Terminal=false" "Categories=Office;"; do
    has_line "$d" "$line" || fail "entry lacks $line: $(entry "$d")"
  done
  validate "$d"
}

test_install_copies_each_hicolor_icon() {
  local d=$WORK/icons n
  run_in "$d" install prefix || { fail "install failed: $(cat "$d/out")"; return; }
  for n in "${SIZES[@]}"; do
    cmp -s "$ROOT/assets/icon/generated/png/${n}x$n.png" "$d/data/icons/hicolor/${n}x$n/apps/placemat.png" ||
      fail "icon ${n}x$n missing or different"
  done
  [[ ! -e $d/data/icons/hicolor/1024x1024 ]] || fail "hicolor has no 1024 directory"
}

test_exec_quotes_spaces_quotes_and_percent() {
  local d=$WORK/quoting
  run_in "$d" install 'my "odd" $dir 100%' || { fail "install failed: $(cat "$d/out")"; return; }
  # Inside the quoted Exec value " and $ take a backslash, which the string escape doubles.
  has_line "$d" "Exec=\"$d/my \\\\\"odd\\\\\" \\\\\$dir 100%%/bin/placemat\"" ||
    fail "Exec not quoted: $(grep '^Exec=' <(entry "$d"))"
  validate "$d"
}

test_uninstall_removes_the_entry_and_icons() {
  local d=$WORK/uninstall n
  run_in "$d" install prefix || { fail "install failed: $(cat "$d/out")"; return; }
  run_in "$d" uninstall prefix || { fail "uninstall failed: $(cat "$d/out")"; return; }
  [[ ! -e $d/data/applications/placemat.desktop ]] || fail "entry kept"
  for n in "${SIZES[@]}"; do
    [[ ! -e $d/data/icons/hicolor/${n}x$n/apps/placemat.png ]] || fail "icon ${n}x$n kept"
  done
  grep -q 'Uninstalled placemat' "$d/out" || fail "no uninstall message: $(cat "$d/out")"
}

test_uninstall_without_an_install_succeeds() {
  run_in "$WORK/fresh" uninstall prefix || fail "uninstall of nothing failed: $(cat "$WORK/fresh/out")"
}

test_bad_arguments_are_a_usage_error() {
  local args code
  for args in "" "frobnicate" "install extra"; do
    code=0
    # shellcheck disable=SC2086 # each case is split into its words on purpose
    PLACEMAT_INSTALL_SKIP_CARGO=1 XDG_DATA_HOME=$WORK/bad bash "$INSTALL_SH" $args >/dev/null 2>"$WORK/bad.err" </dev/null || code=$?
    [[ $code == 2 ]] || fail "args '$args' exited $code, want 2"
    grep -q usage "$WORK/bad.err" || fail "args '$args' printed no usage"
  done
  [[ ! -e $WORK/bad ]] || fail "a usage error touched the data home"
}

# A fake cargo that logs its arguments and CARGO_BUILD_JOBS to FAKE_LOG.
make_fake_cargo() {
  cat >"$WORK/fake-cargo" <<'EOF'
#!/usr/bin/env bash
echo "jobs=${CARGO_BUILD_JOBS:-} $*" >>"$FAKE_LOG"
EOF
  chmod +x "$WORK/fake-cargo"
}

test_install_runs_cargo_install_into_the_prefix() {
  make_fake_cargo
  local d=$WORK/cargo log
  mkdir -p "$d"
  log=$d/cargo.log
  (cd "$d" && CARGO=$WORK/fake-cargo FAKE_LOG=$log JOBS=3 PLACEMAT_PREFIX=p XDG_DATA_HOME=$d/data \
    bash "$INSTALL_SH" install >/dev/null 2>&1 </dev/null) || fail "install with fake cargo failed"
  [[ $(cat "$log") == "jobs=3 install --locked --root $d/p --path $ROOT/crates/placemat-app" ]] ||
    fail "cargo install call: $(cat "$log")"
  : >"$log"
  (cd "$d" && CARGO=$WORK/fake-cargo FAKE_LOG=$log PLACEMAT_PREFIX=p XDG_DATA_HOME=$d/data \
    bash "$INSTALL_SH" uninstall >/dev/null 2>&1 </dev/null) || fail "uninstall with fake cargo failed"
  [[ $(cat "$log") == "jobs= uninstall --root $d/p placemat-app" ]] || fail "cargo uninstall call: $(cat "$log")"
}

test_an_exported_prefix_does_not_move_the_install() {
  local d=$WORK/stray
  mkdir -p "$d"
  (cd "$d" && env -u PLACEMAT_PREFIX PREFIX="$d/stray" PLACEMAT_INSTALL_SKIP_CARGO=1 \
    XDG_DATA_HOME="$d/data" CARGO_INSTALL_ROOT="$d/root" bash "$INSTALL_SH" install >/dev/null 2>&1 </dev/null) ||
    fail "install failed"
  has_line "$d" "Exec=\"$d/root/bin/placemat\"" || fail "Exec ignores CARGO_INSTALL_ROOT: $(entry "$d")"
}

# Put a fake uname printing OS first on PATH, so the OS branches run on this host.
fake_os() {
  mkdir -p "$WORK/os-$1"
  printf '#!/bin/sh\necho %s\n' "$1" >"$WORK/os-$1/uname"
  chmod +x "$WORK/os-$1/uname"
  echo "$WORK/os-$1"
}

test_macos_installs_the_binary_only() {
  local d=$WORK/mac bin
  bin=$(fake_os Darwin)
  mkdir -p "$d"
  (cd "$d" && PATH=$bin:$PATH PLACEMAT_INSTALL_SKIP_CARGO=1 PLACEMAT_PREFIX=p XDG_DATA_HOME=$d/data \
    bash "$INSTALL_SH" install >"$d/out" 2>&1 </dev/null) || fail "macOS install failed: $(cat "$d/out")"
  [[ ! -e $d/data ]] || fail "macOS install wrote to the data home"
  grep -qF "Installed placemat in $d/p/bin; the .app bundle comes with the release milestone." "$d/out" ||
    fail "macOS message: $(cat "$d/out")"
}

test_other_oses_are_refused() {
  local d=$WORK/other bin code=0
  bin=$(fake_os FreeBSD)
  mkdir -p "$d"
  (cd "$d" && PATH=$bin:$PATH PLACEMAT_INSTALL_SKIP_CARGO=1 XDG_DATA_HOME=$d/data \
    bash "$INSTALL_SH" install >"$d/out" 2>&1 </dev/null) || code=$?
  [[ $code == 1 ]] || fail "FreeBSD install exited $code, want 1"
  grep -q FreeBSD "$d/out" || fail "refusal does not name the OS: $(cat "$d/out")"
  [[ ! -e $d/data ]] || fail "refused install wrote to the data home"
}

test_make_passes_cargo_jobs_and_prefix() {
  local got
  got=$(make -s -n -C "$ROOT" install CARGO=c JOBS=7 PREFIX=/x 2>&1) || true
  [[ $got == *'CARGO="c" JOBS="7" PLACEMAT_PREFIX="/x" scripts/install.sh install'* ]] || fail "make install: $got"
  got=$(make -s -n -C "$ROOT" uninstall CARGO=c PREFIX=/x 2>&1) || true
  [[ $got == *'CARGO="c" PLACEMAT_PREFIX="/x" scripts/install.sh uninstall'* ]] || fail "make uninstall: $got"
}

for t in $(declare -F | awk '{print $3}' | grep '^test_'); do "$t"; done
if ((FAILS > 0)); then echo "$FAILS failure(s)" >&2; exit 1; fi
echo "install-test: ok"
