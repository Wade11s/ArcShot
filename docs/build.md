# 可复现构建与依赖记录

适用范围：#19 的启动切片及 #20 的权限引导、捕获和冻结覆盖层。最低运行与 API/SDK 基线均为
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
  本切片没有 Git、未记录 URL 或预编译第三方二进制依赖。运行
  `scripts/inspect-dependencies.py` 可在不展开整棵依赖树的情况下复核直接依赖、
  来源和重复包名数量；该检查及验证报告工具使用 Python 3 标准库，其余构建/验证
  脚本使用 POSIX shell。
- 构建期允许从 crates.io 获取上述可信依赖；产品运行期不获取依赖、不联网。
- bundle 设置 `LSUIElement=true`、`LSMinimumSystemVersion=27.0`，不包含正式签名、
  公证或安装器。本地测试构建优先使用固定的自签名身份 `ArcShot Dev`
  （`scripts/create-dev-signing-identity.sh` 一次性创建，私钥存于登录钥匙串并在
  当前用户域信任 code signing）；其指定需求为
  `identifier + certificate leaf`，跨重建稳定，因此 TCC 屏幕录制授权可跨构建保留。
  未创建该身份时回退 ad-hoc；ad-hoc 的指定需求绑定 CDHash，同一配置重新编译也会
  改变它，不能沿用旧签名的权限结论。该身份仅用于本机测试，不可用于发布或分发，
  首次签名时系统可能询问钥匙串访问（选“始终允许”）。
- Release 的 bundle ID 为 `dev.arcshot.app`，名称为 ArcShot；Debug 使用独立的
  `dev.arcshot.app.debug` 与 ArcShot Debug。构建脚本在签名前写入 Debug 元数据，
  两者的 LaunchServices 解析、TCC 授权与偏好分别归属，避免系统重开时选错构建。
  Debug 与 Release 的 TCC 授权互不覆盖，手动验收须每次确认正在运行的那个 bundle。
- `.cargo/config.toml` 为 arm64 最终链接加入 `/usr/lib/swift` rpath。
  screencapturekit/apple-* 使用第三方 Swift 源码桥，必须保留该标准系统运行库
  搜索路径。

## 直接依赖与维护风险

| 依赖（锁定版本） | 用途 | 替代与风险决策 |
| --- | --- | --- |
| GPUI 0.2.2，默认特性关闭 + `font-kit` | app 的 run loop、文字系统、后续窗口/渲染 | pre-1.0 API 与 macOS 生命周期变化风险高；锁定版本，升级须重跑文字与原生矩阵。未采用 `NoopTextSystem`。 |
| anyhow 1.0.104 | 可执行文件启动错误上下文 | 不进入其他 crate 的公共错误 API。 |
| objc2 0.6.5、objc2-app-kit / foundation 0.3.2 | NSStatusItem、NSMenu、NSImage、NSApplication 的 Rust 绑定 | 替代是更老 cocoa/objc 或手写 FFI；当前选择减少 unsafe 并与主线程类型集成。generated binding feature/可用性风险由锁版与 MAC-START 检查控制。 |
| dispatch2 0.3.1 | 将退出 action 安全排队回主队列 | 替代为自写 GCD FFI，未采用；`MainThreadBound` 避免自写 `unsafe impl Send`。 |
| image 0.25.10（app，关闭默认特性） | 把内存 BGRA 预览包装成 GPUI 要求的 Frame 类型 | 已由 GPUI 带入同版本；不使用编码、磁盘或网络加载。替代为 PNG 中转会增加编码和内存副本，未采用。 |
| png 0.18.1（image） | 把裁剪后的 RGBA 合成为 8-bit sRGB PNG | 只编码内存中的选定区域，不读盘、不联网；写入前用预算 writer 限制 128 MiB。替代为 `image` 的编码特性会扩大依赖与内存副本，未采用。 |
| log 0.4.34（app） | 文本诊断和捕获/窗口提交耗时 | 已在依赖图中；自有 stderr logger 无网络、遥测和像素日志。替代为每处直接打印未采用。 |
| log 0.4.34（macos，#21） | Carbon 热键适配器在注销/移除失败与 Drop 异常路径记录诊断 | 与 app 共用同一 crate 与 stderr logger；适配器不做窗口或用户界面，失败仍以 `Result` 返回。 |

四个本地 crate 均为 path dependency。依赖方向由 `scripts/check-architecture.sh`
检查：app → core/image/macos；image/macos → core；image 与 macos 不互依赖；
core 无产品反向依赖或系统 I/O，GPUI 只在 app。

### GPUI Kit 评估（本切片不引入）

用户提出的 longbridge/gpui-kit 已按 v0.8.1 manifest 与 facade 源码核对。其默认
上游后端锁定 `gpui-pre 0.3.9`，不是本切片已验证的 crates.io GPUI 0.2.2；即使
关闭默认 component/assets，也仍引入 Base 层。它提供应用窗口与后续组件/测试
facade，但不提供本票的 NSStatusItem 生命周期。#19 时没有 GPUI 窗口或组件可复用；
#20 的少量权限按钮与底图覆盖层沿用已验证 GPUI，不迁移后端或额外引入 Base 层。
不形成两套后端，也不增加无实际职责的依赖。后续工具坞/标注 UI
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
  重复版本先由 `scripts/inspect-dependencies.py` 报告数量；只在需要审查时以
  `scripts/inspect-dependencies.py --show-duplicates` 展开有界列表。

## 捕获依赖与安全边界

macos crate 锁定 screencapturekit 11.1.1（关闭默认特性，启用真实的
`macos_15_0`）及 apple-cf 0.11.7（关闭默认特性，启用 `cg`）。Cargo.lock
同时记录 apple-metal 0.10.4 和 doom-fish-utils 0.4.3。它们从 crates.io 源码
构建，包含第三方 SwiftPM 源码桥；编译通过不意味着原生验收通过。
系统仍以 macOS 27 为最低基线，不虚构 macOS 27 feature。替代路线
objc2-screen-capture-kit 重写未获采用，不擅自增加。第三方 Swift 编译/运行库、
SDK 可用性与上游维护是主要风险；不得引入未经确认的预编译组件。

捕获适配器同步调用 SCK；app 必须在第一次 SCK 调用之前开始独立的 5 秒绝对
看门狗，捕获 worker panic，worker 未退出前不得启动新请求。SCK 自身等待时间
可受环境变量影响，不是安全看门狗；任何 SCK 请求错误、超时或 worker panic
均须进入禁止输出及会话内重启的终态。调用方在主线程列举全部可见自有窗口，
捕获前隐藏状态栏图标以防其菜单重新打开，捕获完成前不得显示或新建窗口；
任一窗口未能精确映射时拒绝捕获。正式产品的
权限、显示几何、窗口排除、命名 sRGB 色彩及生命周期证据仍须实机取得。

## 全局热键适配（#21）

普通键全局热键不引入新第三方依赖，由 macos crate 直接绑定 Carbon
HIToolbox 系统框架的 `RegisterEventHotKey` / `InstallEventHandler` /
`UnregisterEventHotKey`（`crates/macos/src/hotkey.rs`）。常量取值以本机
macOS 27 SDK 头文件为准（`CarbonEvents.h`：`kEventClassKeyboard='keyb'`、
`kEventHotKeyPressed=5`、`typeEventHotKeyID='hkid'`；`Events.h` 修饰键位），
未沿用研究报告的数值记忆。research 分支评估过 global-hotkey 0.9.0
（同样基于该 Carbon 路径）；本票不引入它：一是维持 Rust 自有代码绑定系统
框架的路线，二是避免新增依赖的供应链与主线程约束需要重新记录。注册为
非独占（options=0），注册成功只表示本进程会收到通知，不承诺系统独占；
注册失败以 `Result` 返回，失败路径不 panic。

适配器安全边界：Carbon 路径（`install` / `set_active` / `suspend` / `resume`）
仅限主线程并由 `MainThreadMarker` 检查；NSUserDefaults 持久化按 Apple 文档为
线程安全，不附加主线程检查。安装时把按键计数器以 `Rc<RefCell<u32>>` 传入
Carbon handler；`Drop` 先移除 handler 再释放，移除失败时 `mem::forget` 该
计数器副本，避免 handler 仍被调用时释放其引用数据；注销失败的
`EventHotKeyRef` 记入 retired 列表并在 `Drop` 重试。`set_active` 采用
“先注册新组合、成功后注销旧组合”，拒绝时旧注册保持原状。持久化使用
NSUserDefaults 的 `GlobalCaptureHotkey` 键，格式为
`command+control+option+shift+<键>`，非法或缺省回退默认 `command+shift+x`。
真实按键投递、系统冲突与跨重启记忆属 MAC-START，需要用户实机参与，本票只
提供注册/暂挂/恢复/清理的自动化冒烟（MAC-HOTKEY-21-01）。
