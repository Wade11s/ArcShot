# ArcShot

ArcShot is a macOS 27 menu-bar screenshot tool written in Rust with GPUI. This
branch currently contains only the first vertical slice: a real Accessory app
with a native status item, a working Quit ArcShot command, a real font backend,
and reproducible build/startup checks. Capture and annotation commands are not
shown until implemented.

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

See [TEST.md](TEST.md) for the individual seams and their limits.
