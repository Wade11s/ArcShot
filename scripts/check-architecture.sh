#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

fail() {
    printf 'architecture check failed: %s\n' "$1" >&2
    exit 1
}

require() {
    case "$2" in
        *"$1"*) ;;
        *) fail "expected $1 in $3 dependency graph" ;;
    esac
}

reject() {
    case "$2" in
        *"$1"*) fail "unexpected $1 in $3 dependency graph" ;;
        *) ;;
    esac
}

core_tree=$(cargo tree --locked -p arcshot-core --edges normal)
image_tree=$(cargo tree --locked -p arcshot-image --edges normal)
macos_tree=$(cargo tree --locked -p arcshot-macos --edges normal)
app_tree=$(cargo tree --locked -p arcshot-app --edges normal)

# ADR-0001: `crates/app` depends on the crates.io snapshot packages
# `gpui-pre` and `gpui-pre-platform`, renamed to `gpui`/`gpui-platform` in
# Cargo.toml. `cargo tree` prints upstream package names, so match those; the
# version marker keeps checkout paths (which may contain "gpui") out of the
# match. The old `gpui` package stays rejected so a partial revert cannot pass.
gpui_base='gpui-pre v'
gpui_platform='gpui-pre-platform v'
gpui_legacy='gpui v'

reject 'arcshot-image' "$core_tree" core
reject 'arcshot-macos' "$core_tree" core
reject 'arcshot-app' "$core_tree" core
reject "$gpui_base" "$core_tree" core
reject "$gpui_platform" "$core_tree" core
reject "$gpui_legacy" "$core_tree" core

require 'arcshot-core' "$image_tree" image
reject 'arcshot-macos' "$image_tree" image
reject 'arcshot-app' "$image_tree" image
reject "$gpui_base" "$image_tree" image
reject "$gpui_platform" "$image_tree" image
reject "$gpui_legacy" "$image_tree" image

require 'arcshot-core' "$macos_tree" macos
reject 'arcshot-image' "$macos_tree" macos
reject 'arcshot-app' "$macos_tree" macos
reject "$gpui_base" "$macos_tree" macos
reject "$gpui_platform" "$macos_tree" macos
reject "$gpui_legacy" "$macos_tree" macos

require 'arcshot-core' "$app_tree" app
require 'arcshot-image' "$app_tree" app
require 'arcshot-macos' "$app_tree" app
require "$gpui_base" "$app_tree" app
require "$gpui_platform" "$app_tree" app
reject "$gpui_legacy" "$app_tree" app

printf 'architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies\n'
