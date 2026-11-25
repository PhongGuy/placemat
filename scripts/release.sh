#!/usr/bin/env bash
# Release helpers for the release workflow: the tag and keys guard, the build matrices, the
# secret-free build and package, the macOS dmg, stable-named copies, SHA256SUMS and the update
# manifest. Must stay bash 3.2 safe, since macOS runners run /bin/bash 3.2.
# Sourcing this file only defines functions; running it dispatches a subcommand.
source "$(dirname "${BASH_SOURCE[0]}")/package-linux.sh"
set -euo pipefail

# The workspace version, read the way package-linux.sh's main reads it.
workspace_version() {
  local v
  v=$(cargo pkgid -p placemat-app) || return 1
  echo "${v##*[#@]}"
}

# Refuse a release from the wrong ref: a tag must be exactly vVERSION, and a
# branch may only start a dry run.
check_ref() {
  local type=$1 name=$2 version=$3 dry=$4
  case $type in
    tag)
      if [[ $name != "v$version" ]]; then
        echo "release: tag $name does not match the workspace version v$version" >&2
        return 1
      fi
      ;;
    branch)
      if [[ $dry != true ]]; then
        echo "release: a branch ($name) can only start a dry run; push tag v$version to release" >&2
        return 1
      fi
      ;;
    *)
      echo "release: unsupported ref type '$type'" >&2
      return 1
      ;;
  esac
}

# Refuse a keys file that a shipped build could not verify an update with: the placeholder,
# no key at all, or a line that is not base64 of a minisign public key. Comments and blank
# lines are skipped, as the app's own parser skips them.
check_keys() {
  local file=$1 line n=0 keys=0
  while IFS= read -r line || [[ -n $line ]]; do
    n=$((n + 1))
    line=$(printf '%s' "$line" | tr -d '[:space:]')
    [[ -z $line || $line == \#* ]] && continue
    if [[ $line == PLACEMAT-UPDATE-KEY-PLACEHOLDER ]]; then
      echo "release: $file still holds the placeholder key; generate one (RELEASING.md, Keys)" >&2
      return 1
    fi
    if ! printf '%s' "$line" | base64 -d 2>/dev/null | head -n1 | grep -q '^untrusted comment: minisign public key: '; then
      echo "release: $file line $n is not a minisign public key" >&2
      return 1
    fi
    keys=$((keys + 1))
  done <"$file"
  if ((keys == 0)); then
    echo "release: $file holds no update key" >&2
    return 1
  fi
}

# Print the Linux and the macOS and Windows build matrices for PLATFORMS, one JSON line
# each as linux=... and desktop=..., rows in a fixed order; an empty side prints "".
# Only a dry run may build a subset; a real release needs all four platforms.
select_targets() {
  local platforms=${1:-linux windows macos-arm64 macos-intel} dry=$2
  local linux=0 windows=0 mac_arm=0 mac_intel=0 p tokens=()
  # Split with read so a token is never glob-expanded; empty tokens are skipped.
  IFS=', ' read -r -a tokens <<<"$platforms"
  for p in ${tokens[@]+"${tokens[@]}"}; do
    case $p in
      '') ;;
      linux) linux=1 ;;
      windows) windows=1 ;;
      macos-arm64) mac_arm=1 ;;
      macos-intel) mac_intel=1 ;;
      *) echo "release: unknown platform '$p'" >&2; return 1 ;;
    esac
  done
  if [[ $dry != true && $linux$windows$mac_arm$mac_intel != 1111 ]]; then
    echo "release: a release must build linux, windows, macos-arm64 and macos-intel (got: $platforms)" >&2
    return 1
  fi
  if [[ $linux$windows$mac_arm$mac_intel == 0000 ]]; then
    echo "release: no platforms selected" >&2
    return 1
  fi

  local rows=() IFS=,
  if ((linux)); then
    echo "linux={\"include\":[{\"os\":\"ubuntu-24.04\",\"target\":\"x86_64-unknown-linux-gnu\"},{\"os\":\"ubuntu-24.04-arm\",\"target\":\"aarch64-unknown-linux-gnu\"}]}"
  else
    echo "linux="
  fi
  if ((windows)); then rows+=('{"os":"windows-2025","target":"x86_64-pc-windows-msvc"}'); fi
  if ((mac_arm)); then rows+=('{"os":"macos-15","target":"aarch64-apple-darwin"}'); fi
  if ((mac_intel)); then rows+=('{"os":"macos-15-intel","target":"x86_64-apple-darwin"}'); fi
  if ((${#rows[@]} == 0)); then
    echo "desktop="
  else
    echo "desktop={\"include\":[${rows[*]}]}"
  fi
}

# Ad-hoc sign PATH (no Apple identity) and check the signature holds.
adhoc_sign() {
  "${CODESIGN:-codesign}" --force --deep --sign - "$1" || return 1
  "${CODESIGN:-codesign}" --verify --deep --strict "$1"
}

# Ad-hoc sign OUT/Placemat.app, tar the signed bundle into UPDATE as the updater's payload, wrap
# it as OUT/Placemat_VERSION_ARCH.dmg, and drop the .app so only the release artifact is left.
macos_dmg() {
  local out=$1 version=$2 arch=$3 update=$4
  local app=$out/Placemat.app
  adhoc_sign "$app" || return 1
  mkdir -p "$update" || return 1
  "${TAR:-tar}" -C "$out" -czf "$update/Placemat_${version}_${arch}.app.tar.gz" Placemat.app || return 1
  "${HDIUTIL:-hdiutil}" create -volname Placemat -srcfolder "$app" -ov -format UDZO \
    "$out/Placemat_${version}_${arch}.dmg" || return 1
  rm -rf "$app"
}

# GNU realpath -m for hosts without it (macOS): resolve the last argument even
# when it does not exist. python3 is on every runner image.
portable_realpath() {
  python3 -c 'import os, sys; print(os.path.realpath(sys.argv[-1]))' "${@: -1}"
}

# The packaging host, macos or windows; Linux and anything else is sent to make package.
package_os() {
  case $(uname -s) in
    Darwin) echo macos ;;
    MINGW* | MSYS* | CYGWIN*) echo windows ;;
    *) echo "release: on $(uname -s), run make package" >&2; return 1 ;;
  esac
}

# The repository root and the absolute output directories for OS: OUT_DIR, default
# target/dist/OS, and UPDATE_DIR, default target/update/OS.
package_dirs() {
  # BSD realpath has no -m; prepare_out_dir needs it.
  realpath -m / >/dev/null 2>&1 || realpath() { portable_realpath "$@"; }
  ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
  OUT=${OUT_DIR:-$ROOT/target/dist/$1}
  UPDATE=${UPDATE_DIR:-$ROOT/target/update/$1}
}

# Build the release binary and the notices on macOS or Windows into OUT_DIR.
build_main() {
  local os
  os=$(package_os) || return 1
  package_dirs "$os"
  prepare_out_dir "$ROOT" "$OUT" || return 1
  OUT=$(realpath -m -- "$OUT")
  cd "$ROOT"
  cargo build --release --locked -p placemat-app
  cargo fetch --locked
  cargo xtask notices --out "$OUT/THIRD-PARTY-NOTICES.html"
  echo "release: $os build in $OUT"
}

# Package what build_main left into the app installer. macOS is ad-hoc signed; Windows ships
# unsigned, since there is no CA-issued certificate.
package_main() {
  local os
  os=$(package_os) || return 1
  package_dirs "$os"
  local out version target
  out=$(realpath -m -- "$OUT")
  UPDATE=$(realpath -m -- "$UPDATE")
  if [[ ! -f $out/THIRD-PARTY-NOTICES.html ]]; then
    echo "release: no build in $out; run release.sh build first" >&2
    return 1
  fi
  unset PLACEMAT_MACOS_SIGNING_IDENTITY PLACEMAT_WINDOWS_SIGN_COMMAND
  cd "$ROOT"
  version=$(workspace_version)
  target=$(rustc -vV | sed -n 's/^host: //p')
  if [[ $os == macos ]]; then
    package_app "$out" app
    TAR=gtar macos_dmg "$out" "$version" "${target%%-*}" "$UPDATE"
  else
    package_app "$out" nsis
  fi
  rm -rf -- "$out/.cargo-packager"
  echo "release: $os artifacts in $out ($([[ $os == macos ]] && echo ad-hoc signed || echo unsigned))"
}

# The stable name of release file NAME at VERSION, for /releases/latest/download links: the
# version dropped. Prints nothing for a file that is not an artifact.
stable_name() {
  local name=$1 version=$2 out
  case $name in
    *.deb | *.AppImage | *.dmg | *.exe) ;;
    *) return 0 ;;
  esac
  case $name in
    *"_${version}_"*) out=${name/"_${version}_"/_} ;;
    *) return 0 ;;
  esac
  echo "$out"
}

# Copy every versioned artifact under DIR to its stable name at DIR's top level, beside the
# versioned file. Fails when two files would share one stable name, or when none is copied.
stable_copies() {
  local dir=$1 version=$2 f stable seen=" " copied=0
  while IFS= read -r f; do
    stable=$(stable_name "$(basename "$f")" "$version")
    [[ -n $stable ]] || continue
    if [[ $seen == *" $stable "* ]]; then
      echo "release: two release files map to the stable name $stable" >&2
      return 1
    fi
    seen="$seen$stable "
    cp -- "$f" "$dir/$stable" || return 1
    copied=$((copied + 1))
  done < <(find "$dir" -type f | LC_ALL=C sort)
  if ((copied == 0)); then
    echo "release: no versioned release files under $dir" >&2
    return 1
  fi
}

# The update targets: id, where the file ships from, its name with @V@ for the version, and its format.
update_targets() {
  cat <<'TARGETS'
app-linux-x86_64 dist placemat_@V@_x86_64.AppImage appimage
app-linux-aarch64 dist placemat_@V@_aarch64.AppImage appimage
app-macos-aarch64 update Placemat_@V@_aarch64.app.tar.gz app
app-macos-x86_64 update Placemat_@V@_x86_64.app.tar.gz app
app-windows-x86_64 dist placemat_@V@_x64-setup.exe nsis
TARGETS
}

# One manifest entry as compact JSON, {ID: {url, format, size, sha256}}; refuses a malformed id or a missing file.
manifest_entry() {
  local id=$1 file=$2 format=$3 url=$4 re='^app-(linux|macos|windows)-(x86_64|aarch64)$' size sha
  if [[ ! $id =~ $re ]]; then echo "release: malformed update target id '$id'" >&2; return 1; fi
  if [[ ! -f $file ]]; then echo "release: update target $id needs $file, which is missing" >&2; return 1; fi
  size=$(wc -c <"$file" | tr -d ' ') || return 1
  sha=$(sha256sum "$file" | cut -d' ' -f1) || return 1
  jq -cn --arg id "$id" --arg url "$url" --arg format "$format" --arg sha "$sha" --argjson size "$size" \
    '{($id): {url: $url, format: $format, size: $size, sha256: $sha}}'
}

# Write UPDATE/latest.json for stable VERSION, artifact URLs under BASE_URL; nothing is written on failure.
write_manifest() {
  local dist=$1 update=$2 version=$3 base=${4%/} re='^[0-9]+\.[0-9]+\.[0-9]+$' id where pattern format name dir entries=
  if [[ ! $version =~ $re ]]; then echo "release: the update manifest needs a stable version, got '$version'" >&2; return 1; fi
  while read -r id where pattern format; do
    name=${pattern//@V@/$version}
    if [[ $where == dist ]]; then dir=$dist; else dir=$update; fi
    entries="$entries$(manifest_entry "$id" "$dir/$name" "$format" "$base/$name")"$'\n' || return 1
  done < <(update_targets)
  printf '%s' "$entries" | jq -s --arg v "$version" --arg d "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    '{version: $v, pub_date: $d, targets: add}' >"$update/latest.json.tmp" || { rm -f -- "$update/latest.json.tmp"; return 1; }
  mv -- "$update/latest.json.tmp" "$update/latest.json"
}

usage() {
  echo "usage: release.sh guard TYPE NAME DRY [PLATFORMS] | keys FILE | build | package | stable DIR VERSION | sums DIR | manifest DIST UPDATE VERSION BASE_URL" >&2
  exit 2
}

main() {
  case ${1:-} in
    guard)
      (($# >= 4)) || usage
      local version root matrices
      root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
      version=$(workspace_version)
      check_ref "$2" "$3" "$version" "$4" || exit 1
      check_keys "$root/assets/update/keys.txt" || exit 1
      matrices=$(select_targets "${5:-}" "$4") || exit 1
      echo "version=$version"
      echo "$matrices"
      ;;
    keys)
      (($# == 2)) || usage
      check_keys "$2"
      ;;
    # Called plainly so set -e covers every step; a function on the left of
    # || or in a condition runs with set -e off.
    build)
      (($# == 1)) || usage
      build_main
      ;;
    package)
      (($# == 1)) || usage
      package_main
      ;;
    stable)
      (($# == 3)) || usage
      stable_copies "$2" "$3"
      ;;
    sums)
      (($# == 2)) || usage
      write_sums "$2"
      ;;
    manifest)
      (($# == 5)) || usage
      write_manifest "$2" "$3" "$4" "$5"
      ;;
    *) usage ;;
  esac
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then main "$@"; fi
