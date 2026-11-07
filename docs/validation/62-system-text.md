# #62 系统无衬线字体栅格化标注文字：验证记录

本记录覆盖 `ticket/62-system-text`（基点 `spec/v1` `e941a69`）上的产品改动：预览、输出与
实时编辑器改用 macOS 系统无衬线字态度量与栅格化，CJK/emoji/IME 结果不再落成 5×7 空心方框。
它**不是** V1 的完整验收证据：本轮只运行 `quiet` 与 `ax` 两档（使用者正在用这台 Mac），
`physical` / `full` 未运行，Release 侧只做了构建与启动冒烟。

## 候选与环境

- 实测实现/脚本 SHA：`c687b2711c825624624843ece17061b9a3f1c80e`（产品：
  `feat(text): rasterize annotation text with the macOS system face`，
  `worktree_dirty=false`）。E2E 脚本与 `TEST.md` 的 SHA：
  `aeca7847238cb0474495c415d6a3c8b3f99f6ded`（`test(e2e): drive system-font text through AX
  and decode the PNG`）；本记录是后续纯文档提交。
- 时间：2026-11-07T10:32:30Z（`scripts/record-validation.sh` 采集）。
- 构建配置：Debug 与 Release。
- E2E 在 Debug 上运行；Release 只做 `check-all.sh` 的构建与启动冒烟，未跑 Release E2E。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 显示器：内建单屏 1470×956 逻辑点，scale=2。
- 权限：`accessibility=granted`、`screen_recording=granted`；未改动任何 TCC 设置。
- 夹具：无；MAC-E2E-39 在真实冻结帧上运行，不启动 `arcshot-scroll-fixture`、TextEdit 或 Safari。
- `agent-desktop`：1.2.0（envelope 2.6）。
- 日志：`target/e2e-62-text.log`（脚本 stdout）、
  `target/e2e-agent-desktop-debug.log`（产品 stderr）。日志是本地构建产物，不入库。

## 三个决定

1. **直接依赖 Core Text，不新增供应链来源。** `core-text 21.0.0`、`core-graphics 0.24.0`、
   `core-foundation 0.10.0` 已由 GPUI 基座以同一锁版带入；`arcshot-macos` 精确 pin 后
   作为直接依赖使用，`scripts/inspect-dependencies.py` 只报同样的 crates.io 来源。
   `image` 与 `macos` 仍互不依赖：桥接 `TextFont` 只在同时依赖两者的 `arcshot-app`。
2. **字体同时负责度量与栅格化。** `TextFont` 新增 `measure`，`rasterize` 接收样式颜色并
   返回**预乘** RGBA。文字颜色由字体在绘制时应用，彩色 emoji 因此保留自身颜色；合成器按
   覆盖率混合，抗锯齿边缘不再被压成不透明像素。5×7 字体只保留给 image 的 golden 与
   `compose_document` 默认入口。
3. **编辑器的语义整值写入与实时诊断。** `arcshot.overlay.text.input` 注册 `SetValue`，
   把整段草稿作为一次 `TextEdited` 送进权威文档；该动作与键入共用 `TextInput::sync` 的
   度量/队列路径。`CaptureAction::TextEdited` 现在也写 `capture_edit action=TextEdited`
   文本诊断，使实时编辑可被外部观察。Text 工具的 `arcshot.overlay.object.new` 在
   `Click` 时于选区中心打开编辑器，因此整条用例无需指针或按键。

## 命令与结果

```sh
cargo fmt --all
cargo clippy --locked --offline --workspace --all-targets --all-features -- -D warnings
cargo test --locked --offline --workspace --all-features
scripts/check-all.sh
scripts/e2e-agent-desktop.py --tier quiet --no-build
scripts/e2e-agent-desktop.py --tier ax --only text_system
scripts/e2e-agent-desktop.py --tier ax --no-build
```

- `cargo fmt --all`：exit 0。
- `cargo clippy … -D warnings`：exit 0。
- `cargo test --locked --workspace --all-features`：exit 0，348 passed；新增 `crates/app/tests/text_font.rs`
  5 个用例。
- `scripts/check-all.sh`：exit 0。关键行：

```text
direct[arcshot-macos]=apple-cf 0.11.7, arcshot-core 0.1.0, block2 0.6.2, core-foundation 0.10.0, core-graphics 0.24.0, core-text 21.0.0, dispatch2 0.3.1, log 0.4.34, objc2 0.6.5, objc2-app-kit 0.3.2, objc2-foundation 0.3.2, objc2-service-management 0.3.2, objc2-uniform-type-identifiers 0.3.2, screencapturekit 11.1.1
dependency_sources=path,registry+https://github.com/rust-lang/crates.io-index
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=debug identifier=dev.arcshot.app.debug name=ArcShot Debug
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

- `scripts/e2e-agent-desktop.py --tier quiet --no-build`：exit 0，
  `mac_e2e=partial configuration=debug tier=quiet units=identity,sound_prefs cases=7/39 acceptance=false`。
- `scripts/e2e-agent-desktop.py --tier ax --only text_system`：exit 0，
  `mac_e2e_39=pass text='中文 😀' source=ax_set_value commit=1 pixels=1000x800 cjk_ink=627 run=26 emoji_ink=473`，
  `e2e_timing unit=text_system seconds=7.0`。
- `scripts/e2e-agent-desktop.py --tier ax --no-build`（全部 ax 单元）：exit 0，
  `mac_e2e=partial … tier=ax units=identity,notice_ax,selection_ax,text_system,scroll_gate,sound_prefs cases=11/39 acceptance=false`，
  `e2e_timing total_s=38.6 agent_desktop_calls=122`。同一树上 MAC-E2E-38（选区/对象手势）
  与 MAC-E2E-24（滚动入口）继续通过，说明字体/编辑器改动没有破坏 #63 的 AX 词表。

### MAC-E2E-39 引用的日志行

`target/e2e-agent-desktop-debug.log`（同一轮）：

```text
a11y_action=arcshot.overlay.selection action=SetValue value=200,200,700,600
capture_edit action=SelectRegion version=1 annotations=0 selection=Some([200.0, 200.0, 500.0, 400.0]) tool=Select …
a11y_action=arcshot.overlay.tool.text action=Click
capture_edit action=SelectTool version=1 annotations=0 selection=Some([200.0, 200.0, 500.0, 400.0]) tool=Text selected=None style=stroke=#FF3B30 size=16
a11y_action=arcshot.overlay.object.new action=Click
a11y_action=arcshot.overlay.text.input action=SetValue
capture_edit action=TextEdited version=1 annotations=0 selection=Some([200.0, 200.0, 500.0, 400.0]) tool=Text …
a11y_action=arcshot.overlay.tool.select action=Click
capture_edit action=SelectTool version=2 annotations=1 selection=Some([200.0, 200.0, 500.0, 400.0]) tool=Select selected=Some(0) style=stroke=#FF3B30 size=16
a11y_action=arcshot.overlay.finish action=Click
capture_edit action=TextCommit version=2 annotations=1 …
output_encode_us=319870 pixels=1000x800
output_publish target=Clipboard bytes=112075 pixels=1000x800 annotations=1
capture_phase from=Frozen to=Resident action=OutputCompleted
```

像素证据来自 AppKit 读出的剪贴板 PNG，在内存解码为 1000×800、完全不透明、带 sRGB 块；
锚点是选区中心 (450,400)，在裁剪 PNG 中对应物理 (500,400)。CJK 的红色笔画覆盖 627 个像素，
某行最长连续 26 像素；旧 5×7 空心方框的内行只有两侧边缘。emoji 区域有 473 个暖色饱和像素，
证明彩色 emoji 没有被样式红覆盖。临时 PNG 在解码后立即删除，仓库树不新增 PNG 文件。

## 逐用例结果

| 用例 | 档位 | 结果 | 证据 |
| --- | --- | --- | --- |
| APP-TEXT-01（系统字态度量 CJK/emoji） | 自动化 | Pass | `crates/app/tests/text_font.rs`；CJK 宽度大于拉丁、行高含 leading、两行计数 |
| APP-TEXT-02（逻辑度量与物理位图一致） | 自动化 | Pass | 位图尺寸 `ceil(metrics*scale)`；覆盖率非空且超过二值 |
| APP-TEXT-03（字形方向与非空心方框） | 自动化 | Pass | “T” 顶部横条像素多于底部竖干；系统 CJK 宽 32px 与 5×7 的 24px 不同 |
| APP-TEXT-04（彩色 emoji） | 自动化 | Pass | alpha>128 的暖色像素 >10 |
| APP-TEXT-05（提交输出用系统字体） | 自动化 | Pass | 系统字体合成与固定字体字节不同；样式红到达合成面；框外像素不变 |
| MAC-E2E-39（AX 写入 CJK/emoji 并解码 PNG） | `ax` | Pass | 上节日志行与 `mac_e2e_39=pass …` |
| MAC-E2E-38/24（#63 手势与滚动入口） | `ax` | Pass | 同一轮 `mac_e2e_38=pass … gestures=8`、`mac_e2e_24=pass … missing=none` |
| MAC-E2E-25（Text 工具与选项） | `physical` | Not tested | 本轮只跑 `quiet`/`ax`；编辑器行高改由系统字体度量后未重跑该物理单元 |
| 其余 MAC-E2E-01…38 | `physical`/`full` | Not tested | 既有结论不因本票改变；完整矩阵待发布候选重跑 |
| 中文 IME 候选窗、真实键盘组合、视觉观感 | — | Not tested | 属 §4.4 用户项；本票只证明 AX 整值写入与系统字体栅格化 |

## Not tested（诚实清单）

- **中文 IME 候选窗与真实键盘组合过程**：`set-value` 是语义整值写入，不经过输入法
  组合；候选窗位置、marked text 与真实键入仍属 MAC-TEXT 的人工项。
- **视觉观感**：字号观感、行距、emoji 与周围文字的排版平衡需要人眼判断（§4.4）。
- **物理/完整 E2E 与 Release E2E**：本轮只跑 `quiet`/`ax` 与 Debug；完整矩阵与 Release
  证据留给发布候选。
- **非 2x 缩放与多显示器**：系统字体路径只在本机 2x 内建屏验证；V1 单内屏之外不声称。

## 隐私

- 运行期无网络、无遥测；`scripts/check-all.sh` 内的 `scripts/check-no-network.sh` 通过。
- 剪贴板 PNG 由 AppKit 读出后在内存解码；临时文件 `target/39-text-system.png` 解码后立即
  删除，仓库树不新增截图。运行结束后剪贴板已清空。

## 给下一票的接缝

- `arcshot-app::SystemFont` 是唯一字体实例；任何新增文字面（例如字号直接输入）必须继续
  走同一 `measure`/`rasterize`，否则编辑框与输出会再次漂移。
- `TextInput::set_content` 是 AX 整值写入的入口；IME 组合仍走 `replace_and_mark_text_in_range`，
  两者共享 `sync` 的度量与队列路径。
- 下一个发布候选仍需在用户明确机器空闲时重跑 `physical`/`full` 与 Release E2E，并更新
  #39 的放行判定。
