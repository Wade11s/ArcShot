# ArcShot

ArcShot is a macOS 27 menu-bar screenshot tool written in Rust with GPUI. It is
a real Accessory app with a native status item, global capture hotkey, frozen
screen on the single built-in display, drag region selection, and Finish to copy
the selected 2x region as an sRGB PNG. Annotation tools, scroll capture and
explicit saving are not implemented yet.

## Build

Requirements and dependency risks are recorded in [docs/build.md](docs/build.md).

```sh
scripts/build-app.sh --release
open target/bundle/release/ArcShot.app
```

The app has no Dock main window. Click the crop icon in the system menu bar and
choose **退出 ArcShot** to exit.

## Checks

```sh
scripts/check-all.sh
```

See [TEST.md](TEST.md) for the individual seams and their limits. Region
selection and PNG output semantics are in [docs/output.md](docs/output.md).
