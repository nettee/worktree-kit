#!/bin/sh
set -eu

APP_NAME="wtk"
DEFAULT_REPO="nettee/worktree-kit"

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

script_dir() {
  case "$0" in
    */*) ;;
    *) return 1 ;;
  esac
  dir=${0%/*}
  [ "$dir" != "$0" ] || dir=.
  printf '%s\n' "$dir"
}

local_default_config_template_path() {
  printf '%s/default-config.toml\n' "$(script_dir)"
}

default_config_template_url() {
  repo=$1
  if [ "${WTK_CONFIG_TEMPLATE_URL+x}" = x ]; then
    [ -n "$WTK_CONFIG_TEMPLATE_URL" ] || fail 'WTK_CONFIG_TEMPLATE_URL must not be empty'
    printf '%s\n' "$WTK_CONFIG_TEMPLATE_URL"
    return 0
  fi
  printf 'https://raw.githubusercontent.com/%s/main/scripts/default-config.toml\n' "$repo"
}

write_default_config_template() {
  repo=$1
  dest=$2

  if local_template=$(local_default_config_template_path) && [ -r "$local_template" ]; then
    cp "$local_template" "$dest"
    return $?
  fi

  template_url=$(default_config_template_url "$repo")
  curl -fsSL "$template_url" -o "$dest"
}

default_install_dir() {
  [ -n "${HOME:-}" ] || fail 'HOME is required when WTK_INSTALL_DIR is unset'
  printf '%s/.local/bin\n' "$HOME"
}

install_global_config() {
  repo=$1
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

  if ! write_default_config_template "$repo" "$config_path"; then
    info "Skipping WTK config creation because $config_path could not be created"
    return 0
  fi
  info "Created WTK config at $config_path"
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

detect_os() {
  os=${WTK_OS:-$(uname -s)}
  case "$os" in
    Darwin|darwin) printf 'darwin\n' ;;
    Linux|linux) printf 'linux\n' ;;
    *) fail "unsupported platform OS: $os" ;;
  esac
}

detect_arch() {
  arch=${WTK_ARCH:-$(uname -m)}
  case "$arch" in
    x86_64|amd64) printf 'amd64\n' ;;
    arm64|aarch64) printf 'arm64\n' ;;
    *) fail "unsupported platform architecture: $arch" ;;
  esac
}

checksum_tool() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf 'sha256sum\n'
    return 0
  fi
  if command -v shasum >/dev/null 2>&1; then
    printf 'shasum\n'
    return 0
  fi
  fail 'missing required command: sha256sum or shasum'
}

sha256_file() {
  tool=$1
  file=$2
  case "$tool" in
    sha256sum) sha256sum "$file" | cut -d' ' -f1 ;;
    shasum) shasum -a 256 "$file" | cut -d' ' -f1 ;;
    *) fail "unsupported checksum tool: $tool" ;;
  esac
}

download_file() {
  url=$1
  dest=$2
  curl -fsSL "$url" -o "$dest" || fail "failed to download: $url"
}

resolve_version() {
  repo=$1
  if [ "${WTK_VERSION+x}" = x ]; then
    [ -n "$WTK_VERSION" ] || fail 'WTK_VERSION must not be empty'
    printf '%s\n' "$WTK_VERSION"
    return 0
  fi
  api_url="https://api.github.com/repos/$repo/releases/latest"
  latest_json=$(curl -fsSL "$api_url") || fail "failed to resolve latest release: $api_url"
  tag=$(printf '%s\n' "$latest_json" | grep '"tag_name"' | head -n 1 | cut -d '"' -f4)
  [ -n "$tag" ] || fail "failed to parse latest release tag from: $api_url"
  case "$tag" in
    v*) printf '%s\n' "${tag#v}" ;;
    *) fail "latest release tag must start with v: $tag" ;;
  esac
}

asset_url() {
  repo=$1
  version=$2
  asset=$3
  if [ "${WTK_DOWNLOAD_BASE_URL+x}" = x ]; then
    [ -n "$WTK_DOWNLOAD_BASE_URL" ] || fail 'WTK_DOWNLOAD_BASE_URL must not be empty'
    printf '%s/%s\n' "${WTK_DOWNLOAD_BASE_URL%/}" "$asset"
  else
    printf 'https://github.com/%s/releases/download/v%s/%s\n' "$repo" "$version" "$asset"
  fi
}

verify_checksum() {
  tool=$1
  checksums=$2
  asset=$3
  file=$4
  expected=$(grep "  $asset\$" "$checksums" | head -n 1 | cut -d' ' -f1)
  [ -n "$expected" ] || fail "checksum missing for asset: $asset"
  actual=$(sha256_file "$tool" "$file")
  [ "$actual" = "$expected" ] || fail "checksum mismatch for asset: $asset"
}

main() {
  require_command curl
  require_command tar
  require_command mktemp
  require_command uname
  require_command grep
  require_command head
  require_command cut
  require_command basename
  require_command dirname
  require_command mkdir
  require_command chmod
  require_command cp
  checksum=$(checksum_tool)

  if [ "${WTK_REPO+x}" = x ]; then
    repo=$WTK_REPO
  else
    repo=$DEFAULT_REPO
  fi
  if [ "${WTK_INSTALL_DIR+x}" = x ]; then
    install_dir=$WTK_INSTALL_DIR
  else
    install_dir=$(default_install_dir)
  fi
  os=$(detect_os)
  arch=$(detect_arch)
  version=$(resolve_version "$repo")
  asset="${APP_NAME}_${version}_${os}_${arch}.tar.gz"

  [ -n "$repo" ] || fail 'WTK_REPO must not be empty'
  [ -n "$install_dir" ] || fail 'install directory must not be empty'
  [ -n "$version" ] || fail 'release version must not be empty'

  work_dir=$(mktemp -d) || fail 'failed to create temporary directory'
  trap 'rm -rf "$work_dir"' EXIT INT TERM

  archive="$work_dir/$asset"
  checksums="$work_dir/checksums.txt"
  extract_dir="$work_dir/extract"
  mkdir -p "$extract_dir" || fail "failed to create extraction directory: $extract_dir"

  info "Installing $APP_NAME $version for $os/$arch into $install_dir"

  download_file "$(asset_url "$repo" "$version" "$asset")" "$archive"
  download_file "$(asset_url "$repo" "$version" checksums.txt)" "$checksums"
  verify_checksum "$checksum" "$checksums" "$asset" "$archive"

  tar -xzf "$archive" -C "$extract_dir" || fail "failed to extract asset: $asset"
  [ -f "$extract_dir/$APP_NAME" ] || fail "asset missing binary: $APP_NAME"
  chmod 0755 "$extract_dir/$APP_NAME" || fail "failed to mark extracted binary executable: $extract_dir/$APP_NAME"
  version_output=$("$extract_dir/$APP_NAME" --version) || fail "extracted binary failed verification: $extract_dir/$APP_NAME --version"
  printf '%s\n' "$version_output" | grep -F "$version" >/dev/null 2>&1 || fail "extracted binary version mismatch: expected $version, got $version_output"

  mkdir -p "$install_dir" || fail "failed to create install directory: $install_dir"
  cp "$extract_dir/$APP_NAME" "$install_dir/$APP_NAME" || fail "failed to install binary into: $install_dir"
  chmod 0755 "$install_dir/$APP_NAME" || fail "failed to mark binary executable: $install_dir/$APP_NAME"

  install_path="$install_dir/$APP_NAME"
  [ -f "$install_path" ] || fail "installed binary missing: $install_path"
  [ -x "$install_path" ] || fail "installed binary is not executable: $install_path"

  info "Installed $APP_NAME at $install_path"
  install_global_config "$repo"

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
  info 'Run: wtk --version'
}

main "$@"
