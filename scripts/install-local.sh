#!/bin/sh
set -eu

APP_NAME="wtk"
DEFAULT_CONFIG_TEMPLATE='[copy]
# Recursively copy ignored files with these exact file names from the main worktree.
recursive = [".env"]

# Copy ignored files at these exact Git-root-relative paths.
exact = [".agents"]
'

fail() {
  printf 'wtk local installer: %s\n' "$1" >&2
  exit 1
}

info() {
  printf '%s\n' "$1"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

default_install_dir() {
  [ -n "${HOME:-}" ] || fail 'HOME is required when WTK_INSTALL_DIR is unset'
  printf '%s/.local/bin\n' "$HOME"
}

install_global_config() {
  if [ -z "${HOME:-}" ]; then
    info 'Skipping WTK config creation because HOME is unset'
    return 0
  fi
  config_dir="$HOME/.wtk"
  config_path="$config_dir/config.toml"

  if ! mkdir -p "$config_dir"; then
    info "Skipping WTK config creation because $config_dir could not be created"
    return 0
  fi
  if [ -e "$config_path" ]; then
    info "WTK config already exists at $config_path"
    return 0
  fi

  if ! printf '%s' "$DEFAULT_CONFIG_TEMPLATE" >"$config_path"; then
    info "Skipping WTK config creation because $config_path could not be created"
    return 0
  fi
  info "Created WTK config at $config_path"
}

script_dir=${0%/*}
[ "$script_dir" != "$0" ] || script_dir=.
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd) || fail 'failed to resolve repository root'

require_command git
require_command cargo
require_command date
require_command mkdir
require_command chmod
require_command cp
install_dir=${WTK_INSTALL_DIR:-$(default_install_dir)}
[ -n "$install_dir" ] || fail 'WTK_INSTALL_DIR must not be empty'

commit=$(git -C "$repo_root" rev-parse --short HEAD) || fail 'failed to read git commit'
[ -n "$commit" ] || fail 'git returned an empty commit hash'

built=$(date -u +"%Y-%m-%dT%H:%M:%SZ") || fail 'failed to generate build time'
[ -n "$built" ] || fail 'date returned an empty build time'

version="dev commit=$commit built=$built"
target_dir="$repo_root/target"

mkdir -p "$install_dir" || fail "failed to create install directory: $install_dir"
(
  cd "$repo_root" &&
  CARGO_TARGET_DIR="$target_dir" WTK_VERSION="$version" cargo build --release --bin "$APP_NAME"
) || fail 'cargo build failed'
cp "$target_dir/release/$APP_NAME" "$install_dir/$APP_NAME" || fail "failed to copy built binary into: $install_dir"
chmod 0755 "$install_dir/$APP_NAME" || fail "failed to chmod installed binary: $install_dir/$APP_NAME"

version_output=$("$install_dir/$APP_NAME" --version) || fail 'installed binary failed --version'
case "$version_output" in
  *"dev commit=$commit"*) ;;
  *) fail "installed binary version is missing commit: $version_output" ;;
esac
case "$version_output" in
  *"built="*) ;;
  *) fail "installed binary version is missing build time: $version_output" ;;
esac

info "Installed $APP_NAME at $install_dir/$APP_NAME"
install_global_config
info "Version: $version_output"
