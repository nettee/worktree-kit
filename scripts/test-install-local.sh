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
home_dir="$tmpdir/home"
mkdir -p "$home_dir"

output=$(cd "$repo_root" && HOME="$home_dir" WTK_INSTALL_DIR="$install_dir" sh "$installer")
[ -x "$install_dir/wtk" ] || fail "wtk binary was not installed as executable"
version_output=$("$install_dir/wtk" --version)
assert_contains "$version_output" "dev commit="
assert_contains "$version_output" "built="
assert_contains "$output" "Installed wtk at $install_dir/wtk"
assert_contains "$output" "Version:"

missing_path="$tmpdir/missing-path"
mkdir -p "$missing_path"
for cmd in git date mkdir chmod; do
  cmd_path=$(command -v "$cmd") || fail "required test command not found: $cmd"
  ln -s "$cmd_path" "$missing_path/$cmd"
done

set +e
missing_output=$(PATH="$missing_path" HOME="$home_dir" WTK_INSTALL_DIR="$tmpdir/missing-go-bin" /bin/sh "$installer" 2>&1)
missing_status=$?
set -e
[ "$missing_status" -ne 0 ] || fail "local installer succeeded with go missing"
assert_contains "$missing_output" "missing required command: go"

printf 'local installer test: ok\n'
