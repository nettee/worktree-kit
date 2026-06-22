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
wrapper_bin="$tmpdir/wrapper-bin"
mkdir -p "$home_dir"

[ -n "${RUSTUP_HOME:-}" ] || RUSTUP_HOME="$HOME/.rustup"
[ -n "${CARGO_HOME:-}" ] || CARGO_HOME="$HOME/.cargo"
output=$(cd "$repo_root" && HOME="$home_dir" RUSTUP_HOME="$RUSTUP_HOME" CARGO_HOME="$CARGO_HOME" CARGO_TARGET_DIR="$custom_target_dir" WTK_INSTALL_DIR="$install_dir" sh "$installer")
[ -x "$install_dir/wtk" ] || fail "wtk binary was not installed as executable"
[ -f "$home_dir/.wtk/config.toml" ] || fail "global WTK config was not created"
[ ! -e "$custom_target_dir/release/wtk" ] || fail "installer unexpectedly used caller-provided CARGO_TARGET_DIR"
version_output=$("$install_dir/wtk" --version)
assert_contains "$version_output" "dev commit="
assert_contains "$version_output" "built="
assert_contains "$output" "Installed wtk at $install_dir/wtk"
assert_contains "$output" "Created WTK config at $home_dir/.wtk/config.toml"
assert_contains "$output" "Version:"
[ ! -s "$home_dir/.wtk/config.toml" ] || fail "local installer should create an empty global WTK config"

printf 'custom config\n' >"$home_dir/.wtk/config.toml"

mkdir -p "$config_cargo_home"
cat >"$config_cargo_home/config.toml" <<EOF
[build]
target-dir = "$config_target_dir"
EOF

config_output=$(cd "$repo_root" && HOME="$home_dir" RUSTUP_HOME="$RUSTUP_HOME" CARGO_HOME="$config_cargo_home" WTK_INSTALL_DIR="$config_install_dir" sh "$installer")
[ -x "$config_install_dir/wtk" ] || fail "wtk binary was not installed as executable with cargo config target-dir"
[ ! -e "$config_target_dir/release/wtk" ] || fail "installer unexpectedly used cargo config target-dir"
assert_contains "$config_output" "Installed wtk at $config_install_dir/wtk"
assert_contains "$config_output" "WTK config already exists at $home_dir/.wtk/config.toml"
assert_contains "$config_output" "Version:"
[ "$(cat "$home_dir/.wtk/config.toml")" = "custom config" ] || fail "local installer overwrote existing global WTK config"

mkdir -p "$wrapper_bin"
real_cargo=$(command -v cargo) || fail 'required test command not found: cargo'
cat >"$wrapper_bin/cargo" <<'EOF'
#!/bin/sh
case "${1:-}" in
  +*)
    printf 'wrapper cargo rejected rustup syntax: %s\n' "$1" >&2
    exit 1
    ;;
esac
exec "$REAL_CARGO" "$@"
EOF
chmod 0755 "$wrapper_bin/cargo"

wrapper_install_dir="$tmpdir/wrapper-bin-install"
wrapper_output=$(cd "$repo_root" && PATH="$wrapper_bin:$PATH" REAL_CARGO="$real_cargo" HOME="$home_dir" RUSTUP_HOME="$RUSTUP_HOME" CARGO_HOME="$CARGO_HOME" WTK_INSTALL_DIR="$wrapper_install_dir" sh "$installer")
[ -x "$wrapper_install_dir/wtk" ] || fail "wtk binary was not installed with wrapper cargo"
assert_contains "$wrapper_output" "Installed wtk at $wrapper_install_dir/wtk"

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

no_home_install_dir="$tmpdir/no-home-bin"
set +e
no_home_output=$(env -u HOME PATH="$PATH" RUSTUP_HOME="$RUSTUP_HOME" CARGO_HOME="$CARGO_HOME" WTK_INSTALL_DIR="$no_home_install_dir" /bin/sh "$installer" 2>&1)
no_home_status=$?
set -e
[ "$no_home_status" -eq 0 ] || fail "local installer failed with WTK_INSTALL_DIR set and HOME unset"
[ -x "$no_home_install_dir/wtk" ] || fail "wtk binary was not installed by local installer when HOME was unset"
assert_contains "$no_home_output" "Installed wtk at $no_home_install_dir/wtk"
assert_contains "$no_home_output" "Skipping WTK config creation because HOME is unset"

bad_home_parent="$tmpdir/bad-home-parent"
mkdir -p "$bad_home_parent"
bad_home_file="$bad_home_parent/home-file"
: >"$bad_home_file"
bad_home_install_dir="$tmpdir/bad-home-bin"
set +e
bad_home_output=$(HOME="$bad_home_file" PATH="$PATH" RUSTUP_HOME="$RUSTUP_HOME" CARGO_HOME="$CARGO_HOME" WTK_INSTALL_DIR="$bad_home_install_dir" /bin/sh "$installer" 2>&1)
bad_home_status=$?
set -e
[ "$bad_home_status" -eq 0 ] || fail "local installer failed when HOME could not hold config"
[ -x "$bad_home_install_dir/wtk" ] || fail "wtk binary was not installed by local installer when HOME could not hold config"
assert_contains "$bad_home_output" "Installed wtk at $bad_home_install_dir/wtk"
assert_contains "$bad_home_output" "Skipping WTK config creation because $bad_home_file/.wtk could not be created"

printf 'local installer test: ok\n'
