# Source map

Use this map to locate product code and primary-source research without copying
prototype implementations into main.

## Product

| Area | Path |
| --- | --- |
| Neutral data and future core behavior | `crates/core/` |
| Assets and future image processing | `crates/image/` |
| Safe AppKit/native adapters | `crates/macos/` |
| GPUI process and app coordination | `crates/app/` |
| Bundle metadata | `packaging/Info.plist` |
| Build, checks, validation tools | `scripts/` |
| Dependency/toolchain decisions | `docs/build.md` |
| Test seams and commands | `TEST.md` |
| Recorded product evidence | `docs/validation/` |
| CI scope and runner contract | `docs/agents/ci.md` |

For a dependency API, read the exact locked crate source after `cargo fetch --locked`.
`cargo metadata --locked --format-version 1` reports each package's `manifest_path`;
use that path instead of guessing an API or relying on a different version's docs.
`scripts/inspect-dependencies.py` provides a bounded direct-dependency/source summary.

## Research and prototypes

Research and prototype branches are primary sources, not product code to merge.
When a branch has no local worktree, inspect it directly:

```sh
git ls-tree -r --name-only research/gpui-macos
git show research/gpui-macos:docs/research/gpui-macos-capabilities.md
```

The `.worktrees/` directory is ignored, so the `find` tool may not index an
existing prototype worktree. Prefer the branch path above when that happens.

| Branch | Known primary sources |
| --- | --- |
| `research/gpui-macos` | `docs/research/gpui-macos-capabilities.md` |
| `research/macos-system` | `docs/research/macos-capture-hotkey-permissions.md` |
| `research/rust-image` | Image pipeline research; list branch paths before reading |
| `research/macos-scroll`, `research/scroll-manual-feasibility` | Scroll feasibility studies; list branch paths before reading |
| `prototype/manual-scroll` | `prototypes/manual-scroll/VALIDATION.md` and throwaway source |
| `prototype/overlay-toolbar` | `overlay-toolbar-prototype.md` and browser prototype |
| `prototype/line-styles` | Flat/Inward visual references and prototype evidence |

Prototype evidence never substitutes for formal product validation in
`docs/validation/`.
