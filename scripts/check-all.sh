#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo doc --locked --workspace --no-deps
scripts/test-validation-tools.sh
scripts/check-validation.sh
scripts/test-dependency-inspection.py
# The inspector reads `cargo metadata --locked --offline`; the lock resolves
# packages for targets this machine never builds (gpui-pre-linux/-windows/-web),
# so the lock must be fully fetched before the offline read. This is a build-time
# fetch of the committed, checksum-pinned lock, not a product runtime request.
cargo fetch --locked
scripts/inspect-dependencies.py
scripts/check-architecture.sh
scripts/check-text-backend.sh
scripts/check-startup.sh
scripts/check-no-network.sh
scripts/check-startup.sh --release

# Opt-in: the agent-desktop end-to-end tiers that cannot take over the keyboard
# or pointer (see TEST.md 4.3.1). Off by default because it needs a login
# session and TCC grants; scripts/e2e-agent-desktop.py stays a separate entry.
case "${ARCSHOT_E2E_TIER:-}" in
    "") ;;
    quiet | ax) scripts/e2e-agent-desktop.py --tier "$ARCSHOT_E2E_TIER" ;;
    *)
        printf 'ARCSHOT_E2E_TIER must be quiet or ax, got %s\n' "$ARCSHOT_E2E_TIER" >&2
        exit 2
        ;;
esac
