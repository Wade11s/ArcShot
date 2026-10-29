# 可复现构建与依赖记录

适用范围：#19 的“菜单栏驻留 → 退出”切片。最低运行与 API/SDK 基线均为
macOS 27；不支持 macOS 26 或更早版本。V1 产品目标为用户 MacBook Air 的
Apple Silicon 单一内建显示器。

## 固定工具链

| 项目 | 固定/本次解析结果 |
| --- | --- |
| Rust / Cargo | 1.99.1；`rust-toolchain.toml` 固定 channel，含 clippy/rustfmt |
| Rust host / target | `aarch64-apple-darwin`；部署目标由 `.cargo/config.toml` 设为 27.0 |
| Xcode | 27.0 (27A266a) |
| macOS SDK | 27.0，SDK build 26A425，`MacOSX27.0.sdk` |
| Swift | Apple Swift 6.4 (swiftlang-6.4.0.34.1)，driver 1.168.6 |
| Metal 编译器 | 32023.921 (metalfe-32023.921.6)，AIR-LLD 32023.921 |
| 本次构建/验收系统 | macOS 27.2 (26B123) |

GPUI 的 build script 会调用 `xcrun -sdk macosx metal/metallib` 编译其自带
Metal shader，并用 bindgen/cbindgen 生成其维护的绑定。这是第三方依赖自己的
构建步骤，不是 ArcShot 自维护的非 Rust 桥。其 shader 命令中的旧
`-mmacosx-version-min` 参数不构成 ArcShot 对旧系统的支持承诺；产品可执行文件
和 bundle 仍以 27.0 为下限。

## 获取与构建

```sh
rustup toolchain install 1.99.1 --component cargo,clippy,rustfmt
cargo fetch --locked
scripts/build-app.sh --release
open target/bundle/release/ArcShot.app
```

- 所有 registry 依赖来自 crates.io，确切版本与 checksum 固定在 `Cargo.lock`；
  本切片没有 Git、未记录 URL 或预编译第三方二进制依赖。
- 构建期允许从 crates.io 获取上述可信依赖；产品运行期不获取依赖、不联网。
- bundle 设置 `LSUIElement=true`、`LSMinimumSystemVersion=27.0`，使用本地
  ad-hoc signature，不包含正式签名、公证或安装器。
- `.cargo/config.toml` 为 arm64 最终链接加入 `/usr/lib/swift` rpath。当前菜单栏
  切片未链接 Swift；后续 screencapturekit/apple-* Swift 桥接入时必须保留该标准
  系统运行库搜索路径。

## 直接依赖与维护风险

| 依赖（锁定版本） | 用途 | 替代与风险决策 |
| --- | --- | --- |
| GPUI 0.2.2，默认特性关闭 + `font-kit` | app 的 run loop、文字系统、后续窗口/渲染 | pre-1.0 API 与 macOS 生命周期变化风险高；锁定版本，升级须重跑文字与原生矩阵。未采用 `NoopTextSystem`。 |
| anyhow 1.0.104 | 可执行文件启动错误上下文 | 不进入其他 crate 的公共错误 API。 |
| objc2 0.6.5、objc2-app-kit / foundation 0.3.2 | NSStatusItem、NSMenu、NSImage、NSApplication 的 Rust 绑定 | 替代是更老 cocoa/objc 或手写 FFI；当前选择减少 unsafe 并与主线程类型集成。generated binding feature/可用性风险由锁版与 MAC-START 检查控制。 |
| dispatch2 0.3.1 | 将退出 action 安全排队回主队列 | 替代为自写 GCD FFI，未采用；`MainThreadBound` 避免自写 `unsafe impl Send`。 |

四个本地 crate 均为 path dependency。依赖方向由 `scripts/check-architecture.sh`
检查：app → core/image/macos；image/macos → core；image 与 macos 不互依赖；
core 无产品反向依赖或系统 I/O，GPUI 只在 app。

### GPUI Kit 评估（本切片不引入）

用户提出的 longbridge/gpui-kit 已按 v0.8.1 manifest 与 facade 源码核对。其默认
上游后端锁定 `gpui-pre 0.3.9`，不是本切片已验证的 crates.io GPUI 0.2.2；即使
关闭默认 component/assets，也仍引入 Base 层。它提供应用窗口与后续组件/测试
facade，但不提供本票的 NSStatusItem 生命周期。本切片没有 GPUI 窗口或组件可复用，
因此不迁移 GPUI、不形成两套后端，也不增加无实际职责的依赖。后续工具坞/标注 UI
切片可在存在具体组件复用时重新评估并完整记录迁移风险。

来源：https://github.com/longbridge/gpui-kit/tree/v0.8.1 （`crates/kit/Cargo.toml`、
`crates/kit/src/lib.rs`、`crates/base/Cargo.toml`）。

## 重要传递原生/构建依赖

- GPUI 0.2.2 带入 cocoa 0.26.0 / objc 0.2.7、metal 0.29.0、core-text 21.0.0、
  core-graphics 0.24.0 与 zed-font-kit 0.14.1-zed，分别用于其既有 AppKit、
  Metal 与真实文字后端。它们链接 Apple 系统框架；升级风险随 GPUI 锁版管理。
- GPUI 构建使用 bindgen 0.71.1（需要 Xcode 自带 libclang）、cbindgen 0.28.0
  以及上述 Metal 工具链。没有 ArcShot 维护的 Swift/Objective-C/C/C++ 产品桥。
- GPUI 的传递图包含 reqwest、TLS、socket 与图像 codec 等通用能力，但
  `Application::new()` 在本版本安装 `NullHttpClient`，本切片没有设置真实 HTTP
  client 或发起请求。后续不得擅自改接网络 client；驻留 socket 由
  `scripts/check-no-network.sh` 实测。重复版本可由 `cargo tree --duplicates`
  审查，不能据此推断实际网络行为。
- 重要传递依赖包含 ring（TLS 的 C/汇编）与 GPUI 自带 Metal shader；这些均随
  crates.io 源码构建。其用途来自 GPUI 通用能力，本切片未调用网络路径。若需
  消除该供应面，应在 GPUI 依赖治理中评估，而不是另写未批准的替代路线。

## 后续捕获基线（本切片尚未链接）

父规格已选 screencapturekit 11.1.1、apple-cf 0.11.7 的 `cg` 能力，接受
apple-metal 传递依赖及第三方 SwiftPM 源码桥。引入时按实际 feature 配置并重新
锁定、记录；不得虚构 macOS 27 feature。替代路线 objc2-screen-capture-kit 重写
未获采用，不擅自增加。第三方 Swift 编译/运行库、SDK 可用性与上游维护是主要
风险；不得引入未经确认的预编译组件。
