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
  rm -rf "$tmpdir"
}
trap cleanup EXIT INT TERM

sh -n "$installer"

install_dir="$tmpdir/bin"
home_dir="$tmpdir/home"
mkdir -p "$home_dir"

output=$(cd "$repo_root" && HOME="$home_dir" WTK_INSTALL_DIR="$install_dir" WTK_MODULE="./cmd/wtk" WTK_VERSION="" WTK_SKIP_PATH_UPDATE=1 sh "$installer")
[ -x "$install_dir/wtk" ] || fail "wtk binary was not installed"
"$install_dir/wtk" --help >/dev/null || fail "installed wtk --help failed"
assert_contains "$output" "Installed wtk at $install_dir/wtk"
assert_contains "$output" "Add $install_dir to PATH:"
assert_contains "$output" "Shell completion examples:"

path_output=$(cd "$repo_root" && HOME="$home_dir" PATH="$install_dir:$PATH" WTK_INSTALL_DIR="$install_dir" WTK_MODULE="./cmd/wtk" WTK_VERSION="" WTK_SKIP_PATH_UPDATE=1 sh "$installer")
assert_contains "$path_output" "$install_dir is already in PATH"

no_go_dir="$tmpdir/no-go-path"
mkdir -p "$no_go_dir"
set +e
missing_output=$(PATH="$no_go_dir" HOME="$home_dir" WTK_INSTALL_DIR="$install_dir" /bin/sh "$installer" 2>&1)
missing_status=$?
set -e
[ "$missing_status" -ne 0 ] || fail "installer succeeded with go missing"
assert_contains "$missing_output" "missing required command: go"

printf 'installer test: ok\n'
