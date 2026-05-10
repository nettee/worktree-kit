#!/bin/sh
set -eu

APP_NAME="wtk"
DEFAULT_MODULE="github.com/nettee/worktree-kit/cmd/wtk"
REQUIRED_GO_VERSION="1.25.0"

fail() {
  printf 'wtk installer: %s\n' "$1" >&2
  exit 1
}

info() {
  printf '%s\n' "$1"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

version_number() {
  printf '%s\n' "$1" | grep -Eo '[0-9]+(\.[0-9]+){1,2}' | head -n 1
}

version_ge() {
  current=$1
  required=$2

  current_major=$(printf '%s' "$current" | cut -d. -f1)
  current_minor=$(printf '%s' "$current" | cut -d. -f2)
  current_patch=$(printf '%s' "$current" | cut -d. -f3)
  required_major=$(printf '%s' "$required" | cut -d. -f1)
  required_minor=$(printf '%s' "$required" | cut -d. -f2)
  required_patch=$(printf '%s' "$required" | cut -d. -f3)

  current_patch=${current_patch:-0}
  required_patch=${required_patch:-0}

  [ "$current_major" -gt "$required_major" ] && return 0
  [ "$current_major" -lt "$required_major" ] && return 1
  [ "$current_minor" -gt "$required_minor" ] && return 0
  [ "$current_minor" -lt "$required_minor" ] && return 1
  [ "$current_patch" -ge "$required_patch" ]
}

require_go_version() {
  go_output=$(go version) || fail "failed to run go version"
  go_version=$(version_number "$go_output")
  [ -n "$go_version" ] || fail "could not parse Go version from: $go_output"
  version_ge "$go_version" "$REQUIRED_GO_VERSION" || fail "Go $REQUIRED_GO_VERSION or newer is required; found $go_version"
}

default_install_dir() {
  [ -n "${HOME:-}" ] || fail 'HOME is required when WTK_INSTALL_DIR is unset'
  printf '%s/.local/bin\n' "$HOME"
}

path_contains() {
  dir=$1
  old_ifs=$IFS
  IFS=:
  for entry in $PATH; do
    [ "$entry" = "$dir" ] && {
      IFS=$old_ifs
      return 0
    }
  done
  IFS=$old_ifs
  return 1
}

profile_path() {
  [ -n "${HOME:-}" ] || return 1
  shell_name=$(basename "${SHELL:-sh}")
  case "$shell_name" in
    zsh) printf '%s/.zshrc\n' "$HOME" ;;
    bash) printf '%s/.bashrc\n' "$HOME" ;;
    fish) printf '%s/.config/fish/config.fish\n' "$HOME" ;;
    *) printf '%s/.profile\n' "$HOME" ;;
  esac
}

profile_export_line() {
  dir=$1
  shell_name=$(basename "${SHELL:-sh}")
  case "$shell_name" in
    fish) printf 'fish_add_path %s\n' "$dir" ;;
    *) printf 'export PATH="%s:$PATH"\n' "$dir" ;;
  esac
}

maybe_update_profile() {
  dir=$1
  [ -z "${WTK_SKIP_PATH_UPDATE:-}" ] || return 0
  [ -t 0 ] && [ -t 1 ] || return 0

  profile=$(profile_path) || return 0
  line=$(profile_export_line "$dir")

  printf 'Add %s to PATH in %s? [y/N] ' "$dir" "$profile"
  read answer || return 0
  case "$answer" in
    y|Y|yes|YES)
      profile_dir=$(dirname "$profile")
      mkdir -p "$profile_dir" || fail "failed to create profile directory: $profile_dir"
      if [ -f "$profile" ] && grep -F "$line" "$profile" >/dev/null 2>&1; then
        info "PATH entry already exists in $profile"
      else
        printf '\n# Added by wtk installer\n%s\n' "$line" >>"$profile" || fail "failed to update profile: $profile"
        info "Updated $profile"
      fi
      ;;
  esac
}

main() {
  require_command go
  require_command grep
  require_command head
  require_command cut
  require_command basename
  require_command dirname
  require_command mkdir

  require_go_version

  module=${WTK_MODULE:-$DEFAULT_MODULE}
  version=${WTK_VERSION-latest}
  install_dir=${WTK_INSTALL_DIR:-$(default_install_dir)}

  [ -n "$module" ] || fail 'WTK_MODULE must not be empty'
  [ -n "$install_dir" ] || fail 'install directory must not be empty'

  mkdir -p "$install_dir" || fail "failed to create install directory: $install_dir"

  if [ -n "$version" ]; then
    install_target="$module@$version"
  else
    install_target="$module"
  fi

  info "Installing $APP_NAME from $install_target into $install_dir"
  GOBIN=$install_dir go install "$install_target" || fail "go install failed for $install_target"

  install_path="$install_dir/$APP_NAME"
  [ -f "$install_path" ] || fail "installed binary missing: $install_path"
  [ -x "$install_path" ] || fail "installed binary is not executable: $install_path"
  "$install_path" --help >/dev/null || fail "installed binary failed verification: $install_path --help"

  info "Installed $APP_NAME at $install_path"

  if path_contains "$install_dir"; then
    info "$install_dir is already in PATH"
  else
    maybe_update_profile "$install_dir"
    info "Add $install_dir to PATH:"
    info "  $(profile_export_line "$install_dir")"
  fi

  info ''
  info 'Shell completion examples:'
  info '  wtk completion bash > /usr/local/etc/bash_completion.d/wtk'
  info '  wtk completion zsh > "${fpath[1]}/_wtk"'
  info '  wtk completion fish > ~/.config/fish/completions/wtk.fish'
  info '  wtk completion powershell > wtk.ps1'
  info ''
  info 'Run: wtk --help'
}

main "$@"
