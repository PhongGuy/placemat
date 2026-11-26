#!/usr/bin/env bash
# Installs or uninstalls placemat for this OS: `install.sh install|uninstall`.
# PLACEMAT_PREFIX becomes cargo's --root; JOBS caps the build; PLACEMAT_INSTALL_SKIP_CARGO=1 skips
# cargo so tests can check the desktop entry alone.
set -euo pipefail

if [ $# -ne 1 ] || { [ "$1" != install ] && [ "$1" != uninstall ]; }; then
  echo "usage: $0 install|uninstall" >&2
  exit 2
fi
action=$1
root=$(cd "$(dirname "$0")/.." && pwd)
cargo=${CARGO:-cargo}
package=placemat-app
skip_cargo=${PLACEMAT_INSTALL_SKIP_CARGO:-}

# The bin directory cargo installs into, in cargo's own order of precedence.
root_args=()
if [ -n "${PLACEMAT_PREFIX:-}" ]; then
  prefix=$PLACEMAT_PREFIX
  case $prefix in /*) ;; *) prefix=$PWD/$prefix ;; esac
  root_args=(--root "$prefix")
  bin=$prefix/bin
elif [ -n "${CARGO_INSTALL_ROOT:-}" ]; then
  bin=$CARGO_INSTALL_ROOT/bin
else
  bin=${CARGO_HOME:-$HOME/.cargo}/bin
fi
case $bin in /*) ;; *) bin=$PWD/$bin ;; esac

# Exec quoted per the desktop entry spec. The string escape doubles each backslash that
# quoting adds, so a backslash becomes four and " ` $ get two in front; % is doubled.
exec_path=$(printf '%s' "$bin/placemat" | sed -e 's/\\/\\\\\\\\/g' -e 's/["`$]/\\\\&/g' -e 's/%/%%/g')

# XDG requires an absolute data home and says a relative one is ignored.
data=${XDG_DATA_HOME:-}
case $data in /*) ;; *) data=$HOME/.local/share ;; esac
# The same file name, icon name and window class as the .deb and AppImage entry.
desktop=$data/applications/placemat.desktop
hicolor=$data/icons/hicolor
# hicolor has no 1024 directory, so the largest committed export is left out.
sizes=(16 24 32 48 64 128 256 512)

# Cache refreshers are optional and their failure never fails the install.
refresh_caches() {
  if command -v update-desktop-database >/dev/null; then
    update-desktop-database -q "$data/applications" 2>/dev/null || true
  fi
  # Only an existing user cache is refreshed: a new one would hide icons other apps add later.
  if command -v gtk-update-icon-cache >/dev/null && [ -f "$hicolor/icon-theme.cache" ]; then
    gtk-update-icon-cache -q -t "$hicolor" 2>/dev/null || true
  fi
}

os=$(uname -s)
if [ "$os" != Linux ] && [ "$os" != Darwin ]; then
  echo "install.sh: $os is not supported; only Linux and macOS are" >&2
  exit 1
fi

if [ "$action" = install ]; then
  if [ -z "$skip_cargo" ]; then
    CARGO_BUILD_JOBS=${JOBS:-4} "$cargo" install --locked ${root_args[@]+"${root_args[@]}"} \
      --path "$root/crates/$package"
  fi
  if [ "$os" = Darwin ]; then
    echo "Installed placemat in $bin; the .app bundle comes with the release milestone."
    exit 0
  fi
  mkdir -p "$(dirname "$desktop")"
  for n in "${sizes[@]}"; do
    mkdir -p "$hicolor/${n}x$n/apps"
    command cp -f "$root/assets/icon/generated/png/${n}x$n.png" "$hicolor/${n}x$n/apps/placemat.png"
  done
  cat >"$desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Placemat
Comment=A repo-native whiteboard, diagram and notes app
Exec="$exec_path"
Icon=placemat
StartupWMClass=placemat
Terminal=false
Categories=Office;
EOF
  refresh_caches
  echo "Installed placemat in $bin, with a desktop entry at $desktop."
else
  if [ -z "$skip_cargo" ]; then
    # A package that is not installed is not an error here.
    "$cargo" uninstall ${root_args[@]+"${root_args[@]}"} "$package" 2>/dev/null || true
  fi
  if [ "$os" = Linux ]; then
    command rm -f "$desktop"
    for n in "${sizes[@]}"; do
      command rm -f "$hicolor/${n}x$n/apps/placemat.png"
    done
    refresh_caches
  fi
  echo "Uninstalled placemat."
fi
