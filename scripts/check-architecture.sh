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

reject 'arcshot-image' "$core_tree" core
reject 'arcshot-macos' "$core_tree" core
reject 'arcshot-app' "$core_tree" core
reject 'gpui v' "$core_tree" core

require 'arcshot-core' "$image_tree" image
reject 'arcshot-macos' "$image_tree" image
reject 'arcshot-app' "$image_tree" image
reject 'gpui v' "$image_tree" image

require 'arcshot-core' "$macos_tree" macos
reject 'arcshot-image' "$macos_tree" macos
reject 'arcshot-app' "$macos_tree" macos
reject 'gpui v' "$macos_tree" macos

require 'arcshot-core' "$app_tree" app
require 'arcshot-image' "$app_tree" app
require 'arcshot-macos' "$app_tree" app
require 'gpui v' "$app_tree" app

printf 'architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies\n'
