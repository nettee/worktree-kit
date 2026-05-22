#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
installer="$repo_root/scripts/install-local.sh"

fail() {
  printf 'local installer test: %s\n' "$1" >&2
  exit 1
}

assert_contains() {
  haystack=$1
  needle=$2
  printf '%s' "$haystack" | grep -F "$needle" >/dev/null 2>&1 || fail "expected output to contain: $needle"
}

tmpdir=$(mktemp -d)
cleanup() {
  [ -d "$tmpdir" ] || return 0
  chmod -R u+w "$tmpdir"
  rm -rf "$tmpdir"
}
trap cleanup EXIT INT TERM

sh -n "$installer"

install_dir="$tmpdir/bin"
config_install_dir="$tmpdir/config-bin"
home_dir="$tmpdir/home"
config_cargo_home="$tmpdir/config-cargo-home"
custom_target_dir="$tmpdir/shared-target"
config_target_dir="$tmpdir/config-target"
mkdir -p "$home_dir"

[ -n "${RUSTUP_HOME:-}" ] || RUSTUP_HOME="$HOME/.rustup"
[ -n "${CARGO_HOME:-}" ] || CARGO_HOME="$HOME/.cargo"
output=$(cd "$repo_root" && HOME="$home_dir" RUSTUP_HOME="$RUSTUP_HOME" CARGO_HOME="$CARGO_HOME" CARGO_TARGET_DIR="$custom_target_dir" WTK_INSTALL_DIR="$install_dir" sh "$installer")
[ -x "$install_dir/wtk" ] || fail "wtk binary was not installed as executable"
[ ! -e "$custom_target_dir/release/wtk" ] || fail "installer unexpectedly used caller-provided CARGO_TARGET_DIR"
version_output=$("$install_dir/wtk" --version)
assert_contains "$version_output" "dev commit="
assert_contains "$version_output" "built="
assert_contains "$output" "Installed wtk at $install_dir/wtk"
assert_contains "$output" "Version:"

mkdir -p "$config_cargo_home"
cat >"$config_cargo_home/config.toml" <<EOF
[build]
target-dir = "$config_target_dir"
EOF

config_output=$(cd "$repo_root" && HOME="$home_dir" RUSTUP_HOME="$RUSTUP_HOME" CARGO_HOME="$config_cargo_home" WTK_INSTALL_DIR="$config_install_dir" sh "$installer")
[ -x "$config_install_dir/wtk" ] || fail "wtk binary was not installed as executable with cargo config target-dir"
[ ! -e "$config_target_dir/release/wtk" ] || fail "installer unexpectedly used cargo config target-dir"
assert_contains "$config_output" "Installed wtk at $config_install_dir/wtk"
assert_contains "$config_output" "Version:"

missing_path="$tmpdir/missing-path"
mkdir -p "$missing_path"
for cmd in git date mkdir chmod cp; do
  cmd_path=$(command -v "$cmd") || fail "required test command not found: $cmd"
  ln -s "$cmd_path" "$missing_path/$cmd"
done

set +e
missing_output=$(PATH="$missing_path" HOME="$home_dir" RUSTUP_HOME="$RUSTUP_HOME" CARGO_HOME="$CARGO_HOME" WTK_INSTALL_DIR="$tmpdir/missing-cargo-bin" /bin/sh "$installer" 2>&1)
missing_status=$?
set -e
[ "$missing_status" -ne 0 ] || fail "local installer succeeded with cargo missing"
assert_contains "$missing_output" "missing required command: cargo"

printf 'local installer test: ok\n'
