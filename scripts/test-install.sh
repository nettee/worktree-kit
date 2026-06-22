#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
installer="$repo_root/scripts/install.sh"

fail() {
  printf 'installer test: %s\n' "$1" >&2
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
home_dir="$tmpdir/home"
mkdir -p "$home_dir"
fixture_dir="$tmpdir/releases"
fixture_work="$tmpdir/fixture-work"
mkdir -p "$fixture_dir" "$fixture_work"

cat >"$fixture_work/wtk" <<'EOF'
#!/bin/sh
case "${1:-}" in
  --version) printf 'wtk version 0.0.1\n' ;;
  --help) printf 'wtk help\n' ;;
  *) printf 'wtk fixture\n' ;;
esac
EOF
chmod 0755 "$fixture_work/wtk"
tar -C "$fixture_work" -czf "$fixture_dir/wtk_0.0.1_linux_amd64.tar.gz" wtk
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$fixture_dir" && sha256sum wtk_0.0.1_linux_amd64.tar.gz > checksums.txt)
else
  command -v shasum >/dev/null 2>&1 || fail "sha256sum or shasum is required for installer tests"
  (cd "$fixture_dir" && shasum -a 256 wtk_0.0.1_linux_amd64.tar.gz > checksums.txt)
fi

output=$(cd "$repo_root" && HOME="$home_dir" WTK_INSTALL_DIR="$install_dir" WTK_VERSION="0.0.1" WTK_OS="linux" WTK_ARCH="amd64" WTK_DOWNLOAD_BASE_URL="file://$fixture_dir" WTK_SKIP_PATH_UPDATE=1 sh "$installer")
[ -x "$install_dir/wtk" ] || fail "wtk binary was not installed"
[ -f "$home_dir/.wtk/config.toml" ] || fail "global WTK config was not created"
version_output=$("$install_dir/wtk" --version)
assert_contains "$version_output" "0.0.1"
assert_contains "$output" "Installed wtk at $install_dir/wtk"
assert_contains "$output" "Created WTK config at $home_dir/.wtk/config.toml"
assert_contains "$output" "Add $install_dir to PATH:"
assert_contains "$output" "Shell completion examples:"
[ ! -s "$home_dir/.wtk/config.toml" ] || fail "installer should create an empty global WTK config"

printf 'custom config\n' >"$home_dir/.wtk/config.toml"

path_output=$(cd "$repo_root" && HOME="$home_dir" PATH="$install_dir:$PATH" WTK_INSTALL_DIR="$install_dir" WTK_VERSION="0.0.1" WTK_OS="linux" WTK_ARCH="amd64" WTK_DOWNLOAD_BASE_URL="file://$fixture_dir" WTK_SKIP_PATH_UPDATE=1 sh "$installer")
assert_contains "$path_output" "$install_dir is already in PATH"
assert_contains "$path_output" "WTK config already exists at $home_dir/.wtk/config.toml"
[ "$(cat "$home_dir/.wtk/config.toml")" = "custom config" ] || fail "installer overwrote existing global WTK config"

missing_path="$tmpdir/missing-path"
mkdir -p "$missing_path"
set +e
missing_output=$(PATH="$missing_path" HOME="$home_dir" WTK_INSTALL_DIR="$install_dir" WTK_VERSION="0.0.1" WTK_DOWNLOAD_BASE_URL="file://$fixture_dir" /bin/sh "$installer" 2>&1)
missing_status=$?
set -e
[ "$missing_status" -ne 0 ] || fail "installer succeeded with curl missing"
assert_contains "$missing_output" "missing required command: curl"

set +e
unsupported_output=$(HOME="$home_dir" WTK_INSTALL_DIR="$install_dir" WTK_VERSION="0.0.1" WTK_OS="plan9" WTK_ARCH="amd64" WTK_DOWNLOAD_BASE_URL="file://$fixture_dir" /bin/sh "$installer" 2>&1)
unsupported_status=$?
set -e
[ "$unsupported_status" -ne 0 ] || fail "installer succeeded with unsupported OS"
assert_contains "$unsupported_output" "unsupported platform OS: plan9"

bad_fixture_dir="$tmpdir/bad-releases"
mkdir -p "$bad_fixture_dir"
cp "$fixture_dir/wtk_0.0.1_linux_amd64.tar.gz" "$bad_fixture_dir/wtk_0.0.1_linux_amd64.tar.gz"
printf '0000000000000000000000000000000000000000000000000000000000000000  wtk_0.0.1_linux_amd64.tar.gz\n' >"$bad_fixture_dir/checksums.txt"
set +e
checksum_output=$(HOME="$home_dir" WTK_INSTALL_DIR="$install_dir" WTK_VERSION="0.0.1" WTK_OS="linux" WTK_ARCH="amd64" WTK_DOWNLOAD_BASE_URL="file://$bad_fixture_dir" /bin/sh "$installer" 2>&1)
checksum_status=$?
set -e
[ "$checksum_status" -ne 0 ] || fail "installer succeeded with checksum mismatch"
assert_contains "$checksum_output" "checksum mismatch"

no_home_install_dir="$tmpdir/no-home-bin"
set +e
no_home_output=$(env -u HOME PATH="$PATH" WTK_INSTALL_DIR="$no_home_install_dir" WTK_VERSION="0.0.1" WTK_OS="linux" WTK_ARCH="amd64" WTK_DOWNLOAD_BASE_URL="file://$fixture_dir" WTK_SKIP_PATH_UPDATE=1 /bin/sh "$installer" 2>&1)
no_home_status=$?
set -e
[ "$no_home_status" -eq 0 ] || fail "installer failed with WTK_INSTALL_DIR set and HOME unset"
[ -x "$no_home_install_dir/wtk" ] || fail "wtk binary was not installed when HOME was unset"
assert_contains "$no_home_output" "Installed wtk at $no_home_install_dir/wtk"
assert_contains "$no_home_output" "Skipping WTK config creation because HOME is unset"

printf 'installer test: ok\n'
