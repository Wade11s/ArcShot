# ADR-0001：GPUI 可访问性基座选用 Zed 官方 `gpui-pre` 0.3.11

- 状态：已决定，待 #52 实施验证
- 日期：2026-11-03（同日修订：先按票据锚点验证 0.3.8，再按“升级到最新版本”的用户意图验证最新快照 0.3.11 并改选）
- 票据：ArcShot#51（父规格 ArcShot#18，I-01 / I-02 / T-01 / T-05；观察合同 `TEST.md` §4）
- 影响范围：`crates/app` 的 GPUI 依赖、窗口创建入口、可访问性（Accessibility）接入点
- 本 ADR 只做决策与可行性证明；**合并分支不含任何产品依赖或产品代码变更**，#52 才执行迁移

## 决策

`crates/app` 的 GPUI 基座改为 Zed 官方发布的预发布快照 **`gpui-pre` 0.3.11**（crates.io 上最新的快照，2026-11-02，`zed@f816e1f`），并同时依赖 **`gpui-pre-platform` 0.3.11** 取得 macOS 平台实现。二者精确 pin `=0.3.11`；同一快照族的 20 个 crate（`gpui-pre-apple/-macos/-macros/-platform/-collections/…`）由上游以 `=0.3.11` 互相锁定，整族只能整组升级，锁文件里实测没有混版。

这条基座选择要解决的不是“能不能画 UI”（官方 `gpui` 0.2.2 可以），而是 #18 的观察合同要求窗口内容可被辅助技术观察与驱动；0.2.2 完全不上报 Accessibility 子树（`TEST.md` §4.2、§4.5）。

版本选择遵循用户意图“把 GPUI 升级到最新版本”，不是票据里最初写下的 0.3.8 锚点：0.3.11 就是当前最新，且实测通过（附录 A），因此选最新；0.3.8 实测也通过，作为**已记录的退路**保留（见“被否/后备选项 D”）。

## 依据

- 官方 `gpui` 0.2.2（2025-10-22，仍是该包名的最新版）源码中 **零** `accesskit` 引用、零 `accesskit` 依赖，也没有 `role()` / `on_a11y_action` 等入口。`TEST.md` §4.2 记录的“窗口内 `AXChildren` 为 0”与此一致。
- `gpui-pre` 是 Zed 在 crates.io 上的同仓快照发布：`repository = https://github.com/zed-industries/zed`，`0.3.11` 的 crate 元数据记录 `zed-rev = f816e1f9263e3a8ef63c4baaa02e1441ce1c6038`，Apache-2.0，checksum `9303c771…1d887d`，publisher 为 crates.io 记为 GitHub 已匹配的账号 `huacnlee`（crates.io 只能证明账号，证明不了雇佣关系；来源判定依据是 `repository` 与 crate 内记录的 `zed-rev`）。它把 AccessKit（`accesskit 0.24.1`、`accesskit_macos 0.26.3`、`accesskit_consumer 0.38.0`）接进元素树：`Element::a11y_role`/`write_a11y_info`、`StatefulInteractiveElement::role`/`aria_label`/`accessibility_id`/`on_a11y_action`、`Element::a11y_synthetic_children`、`Window::on_a11y_action`。
- 实测（附录 A）：同一份 ArcShot 在丢弃分支换成 0.3.11 后，macOS 27.2 上 `agent-desktop snapshot --app` 能读到可选址节点，并且 `AXPress` 能真正驱动产品动作（快捷键窗口按钮、覆盖层“取消截图”→ `capture_phase from=Frozen to=Resident action=CancelAttempt`）。对照：0.2.2 构建的同一窗口只有 AppKit 边框按钮（Close/Zoom/Minimize），没有任何产品节点。

## 版本矩阵

| 快照 | 发布 | 结果 | 首个阻塞点 / 备注 |
| --- | --- | --- | --- |
| **0.3.11** | 2026-11-02 | **Pass（选用）** | 用 0.3.8 适配后的代码零改动编译；文字后端、AX 取证、`check-startup.sh`、`check-no-network.sh`、`inspect-dependencies.py` 全过；仅 `check-architecture.sh` 因包名断言失败（#52 修）。 |
| 0.3.10 | 2026-10-26 | Not tested | 最新已通过，未触发向下回退。 |
| 0.3.9 | 2026-10-12 | Not tested | 同上。 |
| 0.3.8 | 2026-10-05 | Pass（后备） | 用同一份代码通过同样取证；但比最新旧，且拉进遗留 `cocoa 0.26 → block 0.1.6`（future-incompat）、596 个解析包、43 个重复包名。 |

## 被否/后备选项

### A. 官方 `gpui` 0.2.2 + 自建 NSAccessibility 桥

否决。`gpui` 0.2.2 无任何可访问性数据源，桥接必须自行从 GPUI 元素树重建 AX 树、维护 role/name 映射、把 AX 动作回投到 GPUI 命中测试与焦点模型，并在每次 GPUI 升级时重做。这与 #18 I-01 的“不自维护 Swift/Objective-C/C++ 桥接”“平台适配只承担窄接口”直接冲突，也无法在现有测试接缝上验证（AX 边界只能靠实机观测，等于把上游已经做对的工作变成我们的产品实现）。

### B. 社区 fork `gpui-ce` 0.2.2

否决，理由是可追溯性与维护风险，不是能力。crates.io 元数据：`repository = https://github.com/gpui-ce/gpui-ce`，description 明写“A community fork of Zed's GPU-accelerated UI framework”，发布者为个人账号。它**确实**带可访问性（`src/_accessibility.rs`、`src/window/a11y.rs`、`elements/div.rs` 中的 `on_a11y_action`，依赖 `accesskit 0.24`），且 API 形状贴近 0.2.2，迁移量更小；但来源不在 Zed，版本号与上游并行演化，仓库无法用“官方快照 + 精确 pin”完成依赖来源审查（#18 I-01），也会把后续回上游的成本推高。若官方路线失败，可作为重估候选。

### C. GPUI Kit（组件库）

否决。它是组件 + 资产层，不是可访问性基座：`gpui-component 0.8.0` 的 `role(Role::` 只出现在 12 个自有控件文件里，对 ArcShot 的覆盖层、选区、标注画布没有任何支持；采用它会把我们的 gpui 基座版本钉在组件库的发布节奏上，并额外引入资产/主题管线。

版本关系也要讲清（修正此前“`gpui-pre =0.3.9` 尚未发布”的旧判断）：`gpui-kit 0.8.0` 当时确实精确依赖 `gpui-pre =0.3.9`，而 0.3.9 自 2026-10-12 起已发布；今天 `0.8.1 → =0.3.10`、`0.9.0 → =0.3.11`。所以否决理由不是“不可解析”，而是范围与耦合：我们需要的正是“官方基座 + 自己的可访问性标注”，而不是别人的控件集。

### D. 停在 0.3.8（票据初始锚点）

不作为决策。0.3.8 实测同样可用，但用户意图是升级到最新，而最新 0.3.11 实测通过；停在 0.3.8 会保留 0.3.11 已经消除的遗留依赖（`cocoa 0.26` / `block 0.1.6`）并多解析 63 个包。保留为**后备**：若未来升到更新快照时出现回归，可先退回 0.3.8 再定位。

## 依赖集合与 pin 策略

产品直接依赖（#52 实施时写入 `Cargo.toml`，本 ADR 不改动）：

```toml
gpui          = { package = "gpui-pre",          version = "=0.3.11", default-features = false, features = ["font-kit"] }
gpui-platform = { package = "gpui-pre-platform", version = "=0.3.11", default-features = false, features = ["font-kit"] }
```

- 用 `package =` 重命名而不是改 `use`：`gpui-pre` 的 lib 名就是 `gpui`，产品代码里的 `use gpui::…` 不需要机械改写。
- `gpui-platform`（lib 名 `gpui_platform`）是 0.3 系列的新入口：`Application::new()` 已被删除，启动必须 `gpui_platform::application()`（或 `headless()`/`linux(..)`）；macOS 实现被拆到 `gpui-pre-macos`，再下探到 `gpui-pre-apple`，由 `gpui-pre-platform` 按 target 转发。
- `features = ["font-kit"]` 必须显式打开，0.3.11 的链路是 `gpui-pre-platform/font-kit → gpui-pre-macos/font-kit → gpui-pre-apple/font-kit → zed-font-kit`（`gpui-pre` 自身的 `font-kit` 在 0.3.11 已是空 feature，保留只为两版写法一致）。实测 `--check-text-backend` 仍取到真实 Helvetica 度量（`i=3.5546875px < W=15.1015625px`），没有退化成 NoopTextSystem。
- `default-features = false` 还会关掉 `gpui-pre-platform` 的 `default = ["multithreaded"]`（仅影响 wasm 后端）；macOS 上无副作用，实测启动与菜单栏路径正常。
- 快照族内部一律 `=0.3.11` 互相锁定（`gpui-pre-apple/-collections/-derive-refineable/-http-client/-ios/-macos/-macros/-platform/-refineable/-scheduler/-shared-string/-sum-tree/-util/-ztracing/…`，锁文件里共 20 个）。部分升级会产生整族重复链接，因此**升级必须整组原子替换**：改两个 `=0.3.x`、`cargo update -p` 到同一版本、提交 `Cargo.lock`、重跑附录 A 的取证与“依赖来源审查如何仍然成立”一节的仓库门。
- 不使用 `^0.3`、不使用 git 依赖、不使用 `[patch]` 覆盖；`crates.io` 上 `gpui-pre` 是每周快照（0.3.3…0.3.11），pin 一个具体快照才可复现。

## 依赖来源审查如何仍然成立

- 解析来源仅 `path` + `registry+https://github.com/rust-lang/crates.io-index`（`scripts/inspect-dependencies.py` 输出 `dependency_sources=path,registry+…`），没有 git 源、没有 vendored 覆盖、没有新的预编译非 Rust 二进制（既有 `screencapturekit` 的 SwiftPM 桥不变）。
- 快照 crate 均来自 `zed-industries/zed`，Apache-2.0，版本、rev 与 checksum 可核；`Cargo.lock` 提交即记录确切解析结果。
- 解析规模与重复：`Cargo.lock` 554 → 533 个包，重复包名 55 → 29（0.3.11 比 0.2.2 与 0.3.8 都更收敛；`taffy 0.15.1`、单一 `async-channel 2.5`）。
- 0.3.11 已消除 0.3.8 的两个遗留风险：不再有 `cocoa 0.26 → block 0.1.6`（构建不再报 future-incompatibility），也不再解析 63 个多余包。仍存的重复项是 `objc2 0.5.3`（经 `accesskit_macos 0.26.3`）与我们的 `objc2 0.6.5` 并存，记录在案。
- 两个仓库门会因换包名而失败，属于 #52 的迁移范围，不是本 ADR 的阻断项：
  - `scripts/check-architecture.sh` 断言 app 依赖图含 `gpui v`；`gpui-pre` 的 `cargo tree` 名字是 `gpui-pre v0.3.11`，实测失败于 `expected gpui v in app dependency graph`。
  - `scripts/inspect-dependencies.py` 以 `cargo metadata --locked --offline` 读取；lock 中新增了 macOS 上不编译但会被解析的 `gpui-pre-linux/-windows/-web/-wgpu/-ios` 等，需先 `cargo fetch --locked`。

## 观察到的行为差异（供 #52，不在此重复漂移清单）

完整的 0.2.2 → 0.3.11 漂移清单见 [`docs/gpui-pre-0.3.11-drift.md`](../gpui-pre-0.3.11-drift.md)（窗口选项、文字系统、图像/渲染、焦点与事件、执行器、可访问性入口，逐项标注 CONFIRMED / SUSPECTED）；本文只保留会影响“基座选择是否正确”的观察（均在 0.3.11 上复验）：

- 可访问性树只在辅助技术客户端连接后构建：`snapshot` 会触发，产品 stderr 出现 `Accessibility activated`；窗口刚打开时第一次 `snapshot --app` 可能是空树，第二次才出现节点 → E2E 必须重读，不能把首读当结论。
- `AXPress` 只走 `on_a11y_action`，不会调用 `on_click`。实测：只注册 `on_click` 时点击“稍后再说”无任何效果；补 `on_a11y_action(AccessibleAction::Click, …)` 后同一引用即驱动产品队列。
- 节点身份来自元素 `.id()`：无 id 不进入 AX 树；裸 `&str` 文本子节点不上报；`.accessibility_id("…")` 映射成 macOS `AXIdentifier`，是 E2E 需要的稳定外部 id。
- `WindowKind::PopUp` + `titlebar: None` 的冻结覆盖层仍然不出现在 `list-windows`，但**其 AX 子树可读、可驱动**（覆盖层按钮拿到 `ref_id`）。
- `gpui-pre-macos` 新增一行无害 stderr：`sandbox_extension_issue_file_to_process failed for … ArcShot.app: 1 (Operation not permitted)`；0.2.2 没有。日志断言不要依赖 stderr 为空。

## 重估触发条件

1. 官方 `gpui`（同一 crates.io 包名）发布带 AccessKit 的稳定版本，或 0.3 系列以非 `-pre` 名称转正 → 换回官方包名，删除重命名与平台 crate 依赖。
2. crates.io 出现比当前 pin 更新的 `gpui-pre` 快照 → 按“整组原子升级 + 重跑附录 A 取证 + 重跑仓库门”评估一次，不逐周追新；若新快照回归，按版本矩阵回退到已验证版本并记录首个阻塞点。
3. `gpui-pre-apple`/`accesskit_macos` 去掉 `objc2 0.5` 或 AccessKit 升级到新主版本 → 重跑取证并复评 pin。
4. 我们的覆盖层/标注画布发现可访问性模型表达不了（例如必须依赖 `a11y_synthetic_children` 才能暴露命中测试对象，或焦点/IME 组合无法被 AX 观察）→ 先在 #52 的实机矩阵里定性，再决定是否重估基座。
5. 上游把 AX 动作投递语义改成“Click 自动映射 on_click” → 删掉双写 helper，简化按钮封装。

## 附录 A：实测证据

环境：macOS 27.2（26B123，arm64）、Xcode 27.0、rustc/cargo 1.99.1、`agent-desktop 1.1.0`（envelope 2.6，Accessibility 与 Screen Recording 均 granted）。
取证分支：**`ticket/51-spike-gpui-pre`**（丢弃，不合并），worktree **`.worktrees/51-spike-gpui-pre`**，当前提交 `8a28d54`（0.3.11 版；同分支 `817eada` 是 0.3.8 版，基点 `spec/gpui-a11y` `0548295`）。完整原始输出：`/tmp/arcshot-spec-notes/51-proof.log`。

丢弃分支的改动面（供复现，不进合并分支）：`Cargo.toml`/`Cargo.lock` 换 `gpui-pre`/`gpui-pre-platform`（`=0.3.11`）、`main.rs` 改用 `gpui_platform::application()` 与 `background_executor().timer()`、`coordinator.rs` 适配 `Entity::update`/`Window::focus`/`DisplayId`、`ui.rs` 给覆盖层/提示/快捷键窗口加 `id`+`role`+`aria_label`+`accessibility_id`+`on_a11y_action`（外加一个 spike 专用“取消截图”按钮）。

```
$ cargo check --workspace           # CARGO_TARGET_DIR=/tmp/arcshot-51-target
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 22.75s
$ cargo build -p arcshot-app
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 23.38s
$ /tmp/arcshot-51-target/debug/arcshot --check-text-backend
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
```

0.2.2 对照（同一份产品，集成 tip 于 2026-11-03 19:35 构建的 Debug 二进制；未逐位核对 commit SHA，仅作前后对比）：AX 树里只有 AppKit 边框按钮，没有产品节点。

```
$ agent-desktop snapshot --app arcshot
{"data":{"ref_count":3,"tree":{"children":[{"name":"Close","role":"button",…},
 {"name":"Zoom","role":"button",…},{"name":"Minimize","role":"button",…}],"role":"window"}}}
```

0.3.11：快捷键设置窗口（普通标题栏窗口）暴露产品对话框与按钮，`AXPress` 驱动产品动作：

```
$ agent-desktop snapshot --app arcshot
{"data":{"ref_count":5,"snapshot_id":"s7diiwoejuwk2","tree":{"children":[{"children":[{"children":[
 {"available_actions":["Click","SetFocus"],"name":"录入新快捷键","ref_id":"@s7diiwoejuwk2:e1","role":"button"},
 {"available_actions":["Click","SetFocus"],"name":"关闭","native_id":{"kind":"ax_identifier","value":"关闭"},"ref_id":"@s7diiwoejuwk2:e2","role":"button"}],
 "name":"截图快捷键","role":"dialog","states":["focused"]}]…
$ agent-desktop click @s7diiwoejuwk2:e2
… {"steps":[{"label":"AXPress","mechanism":"semantic_api","outcome":"succeeded"}]}
$ agent-desktop list-windows --app arcshot
{"data":[]}
# 产品 stderr:
level=INFO target=arcshot::ui a11y_click label=关闭 command=HotkeySettingsClosed(WindowId(1v1))
level=INFO target=arcshot::coordinator hotkey_dispatch action=SettingsClosed
```

0.3.11：`WindowKind::PopUp` + `titlebar: None` 的冻结覆盖层（仍不在 `list-windows`）把子树交给 macOS，并可被 AX 驱动：

```
$ agent-desktop list-windows --app arcshot
{"data":[]}                          # 无标题覆盖层照旧不可枚举
$ agent-desktop snapshot --app arcshot
{"data":{"ref_count":1,"snapshot_id":"s1b1y2u08libh4","tree":{"children":[{"children":[{"children":[{"children":[
 {"name":"画面已冻结 · Esc 取消（本阶段不输出）","native_id":{"kind":"ax_identifier","value":"capture-status"},"role":"statictext"},
 {"available_actions":["Click","SetFocus"],"name":"取消截图","native_id":{"kind":"ax_identifier","value":"capture-cancel"},"ref_id":"@s1b1y2u08libh4:e1","role":"button"}],
 "name":"ArcShot 截图覆盖层","native_id":{"kind":"ax_identifier","value":"capture-overlay"},"role":"group"}]…
$ agent-desktop click @s1b1y2u08libh4:e1     # 覆盖层按钮
… {"steps":[{"label":"AXPress","mechanism":"semantic_api","outcome":"succeeded"}]}
# 产品 stderr:
level=INFO target=arcshot::coordinator capture_phase from=Frozen to=Resident action=CancelAttempt
```

仓库门与锁文件（丢弃分支，0.3.11）：

```
$ ./scripts/check-startup.sh
bundle_identity_smoke=pass configuration=debug identifier=dev.arcshot.app.debug name=ArcShot Debug
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
capture_menu_smoke=pass queued_commands=3 no_native_capture=true
hotkey_registration_smoke=pass default=⌘⇧X suspend_resume_cleanup=clean
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
$ ./scripts/check-no-network.sh
privacy_smoke=pass resident_pid=865 network_sockets=0 lsof_status=1
$ python3 scripts/inspect-dependencies.py        # 先 cargo fetch --locked
direct[arcshot-app]=…, gpui-pre 0.3.11, gpui-pre-platform 0.3.11, …
dependency_sources=path,registry+https://github.com/rust-lang/crates.io-index
duplicate_package_names=29                        # 0.2.2 基线 55，0.3.8 为 43
$ ./scripts/check-architecture.sh
architecture check failed: expected gpui v in app dependency graph   # 需 #52 改包名断言
# Cargo.lock 审计：20 个 gpui-pre* crate 全部 0.3.11，无混版；533 个包；无 cocoa/block；
# objc2 0.5.3(accesskit_macos)+0.6.5(arcshot-macos)；taffy 0.15.1
```

## 附录 B：本 ADR 未证明的事

- 0.3.10 与 0.3.9 未实测（最新 0.3.11 已通过，没有触发向下回退）；它们只作为“若未来 0.3.11 回归时的候选”存在，不得据本 ADR 声称其可用。
- 未做 IME/组合文字、焦点遍历、控件文本（AXValue/选中区）的可访问性验证：产品目前没有文本输入控件，#52/#53 落地时必须在 MAC-TEXT 里单独取证。
- 未证明 AccessKit 子树在**正式 bundle + 真实 TCC 门禁**下与裸二进制完全一致（本次用小写进程名 `arcshot` 的可执行文件与 Debug bundle 的 `--smoke-test`；AX 读取走的是同一套 `gpui-pre-macos` 适配器，但 bundle 名称/身份差异未逐项对照）。
- 未验证标注画布的多对象 AX 暴露（#52 需要决定 `a11y_synthetic_children` 还是逐对象 `id`+`role`）。
- 未测覆盖层是否仍被 `CGWindowList` 排除（`layer 101` 那条原型结论）与性能门槛；T-04 指标一律未评测。
- 未覆盖 `WindowKind::AnchoredPopup`（0.3.8 起新增）是否更适合覆盖层。
- 未评测 `low_power_frame_interval` / `thermal_frame_interval`（0.3.11 新增默认值）对预览动画的影响。
