#!/bin/sh
# check.sh [<artoo-version>] [<target>]: what a commit to main must pass, against that Artoo release's SDK — rustfmt,
# clippy with warnings as errors, and the tests, of the plugin, its contract (`api/`) and the example tracker plugin.
# The version defaults to the first line of artoo-versions.txt; ARTOO_SDK names an unpacked SDK folder instead.
set -e
cd "$(dirname "$0")/.."
version=${1:-$(head -1 artoo-versions.txt)}
target=${2:-$(rustc -vV | sed -n 's/^host: //p')}
if [ -n "$ARTOO_SDK" ]; then
  sdk=$ARTOO_SDK
else
  sdk=$(sh scripts/fetch-sdk.sh "$version" "$target")
fi
sdk=$(cd "$sdk" && pwd)
build=$(sed -n 's/^build=//p' "$sdk/sdk.txt")
rust=$(sed -n 's/^rustc=rustc \([^ ]*\).*/\1/p' "$sdk/sdk.txt")
case "$target" in
  *apple*) lib=libartoo_plugin_sdk.dylib ;;
  *windows*) lib=artoo_plugin_sdk.dll ;;
  *) lib=libartoo_plugin_sdk.so ;;
esac
rustup component add --toolchain "$rust" rustfmt clippy >/dev/null 2>&1
# build.sh's flags: the SDK given to rustc as std is, its identity among them.
us=$(printf '\037')
CARGO_ENCODED_RUSTFLAGS="-L${us}dependency=$sdk/deps${us}-L${us}native=$sdk/deps${us}--extern${us}artoo_plugin_sdk=$sdk/deps/$lib${us}--cfg${us}artoo_sdk_build=\"$build\"${us}--check-cfg${us}cfg(artoo_sdk_build,values(any()))"
export CARGO_ENCODED_RUSTFLAGS
# The doctests are compiled by rustdoc, which reads flags of its own.
CARGO_ENCODED_RUSTDOCFLAGS=$CARGO_ENCODED_RUSTFLAGS
export CARGO_ENCODED_RUSTDOCFLAGS
export ARTOO_SDK_BUILD=$build
# A test is a program linked to the SDK and to Rust's own std as libraries, which Cargo puts on no path.
std=$(rustc "+$rust" --print sysroot)/lib/rustlib/$target/lib
case "$target" in
  *apple*) export DYLD_LIBRARY_PATH="$sdk/deps:$std${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" ;;
  *windows*) export PATH="$sdk/deps:$std:$PATH" ;;
  *) export LD_LIBRARY_PATH="$sdk/deps:$std${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ;;
esac
for manifest in Cargo.toml examples/sample-tracker/Cargo.toml; do
  echo "== $manifest" >&2
  cargo "+$rust" fmt --all --check --manifest-path "$manifest"
  cargo "+$rust" clippy -q --workspace --all-targets --target "$target" --manifest-path "$manifest" -- -D warnings
  cargo "+$rust" test -q --workspace --target "$target" --manifest-path "$manifest"
done
