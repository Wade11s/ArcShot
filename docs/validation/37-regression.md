# #37 完整功能、故障与隐私的实机回归验证记录

本记录覆盖 #37 的回归与证据范围（T-02 自动化、T-03 macOS 实机矩阵、
T-05 证据口径）：在完整候选构建上重跑全部自动化与 E2E，补齐此前单票
未覆盖的组合——全部工具在 Region 与长图上、密度/悬停与文字焦点、
选区/对象历史/裁剪、显式保存/剪贴板/部分结果/成功预览、
取消/重启/迟到结果——并形成逐项证据。证据来自 Rust 自动化套件、
`scripts/check-all.sh`（Debug + Release 冒烟）以及
`scripts/e2e-agent-desktop.py` 的 MAC-E2E-01…37（Debug 与 Release）。

本票只修复一个实际缺陷（文字提交后焦点滞留，见下）；其余均为回归
取证。模拟/注入证据明确标注，不替代原生行为；未跑的项记为
**Not tested**，绝不记 Pass。

## 实测来源与环境

- 基线：`spec/v1` = `54f27cb28e57715ff5d2612a15b0ab889fdc12f0`（Merge
  ticket/36-third-party-scroll）；本票分支从该 tip 分出，合并检查确认
  tip 未移动（`git merge spec/v1` 为 Already up to date）。
- 实测实现/脚本 SHA：`3527babbe2c9492dbbbda80db4d5baa4af82df28`
  （`fix(app): return overlay focus after text commit; add regression E2E combinations`）。
  本证据文档与 `TEST.md` 更新是后续纯文档提交，不改产品源码或脚本。
- 时间：2026-11-05T03:55:48Z（Debug E2E、Release E2E 与
  `scripts/check-all.sh` 全部完成之后）。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` /
  `ArcShot Debug`，Release 为 `dev.arcshot.app` / `ArcShot`；两者均由
  `scripts/build-app.sh` 重建并自签名（身份 `ArcShot Dev`）。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1
  (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)，
  `/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX27.0.sdk`
  （`xcrun --show-sdk-path` 另报 CommandLineTools SDK 路径，构建实际使用
  Xcode SDK）。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)，
  Target arm64-apple-macosx27.2.0。
- Metal/AIR-LLD：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB（与验收基线一致）。
- macOS：27.2 (26B123)（与验收基线一致）。
- 显示器：内建屏，1470×956 逻辑点，scale=2；`agent-desktop
  list-displays` 报 bounds `1470x956`、scale `2.0`。产品日志帧
  `capture_request_us=… pixels=2940x1912 stride=11760 scale=2`。未保存
  屏幕图像。
- 权限：`agent-desktop 1.1.0`（envelope `2.6`），`agent-desktop status`
  为 `accessibility=granted`、`screen_recording=granted`、
  `automation=granted`、`platform=macos`；被测 bundle 三权限预检为
  `missing=none`。未改动 TCC，未请求通知权限，未注册/注销登录项。
  完成音效、密度、热键偏好在 E2E 结束时均已还原为用户原值。
- 夹具：`fixture=scroll-single-frame-e2e-v1`（自有 GPUI 精确像素夹具，
  origin=(600,120) size=760x720）、`fixture=textedit-36-v1`（400 行编号
  纯文本）、`fixture=safari-36-v1`（400 行编号 `file://` 静态页，无远端
  资源）。三个夹具均由脚本在 `target/` 下生成，不入库。
- 日志：`target/e2e-agent-desktop-debug.log`、
  `target/e2e-agent-desktop-release.log`、`/tmp/37-e2e-debug6.log`、
  `/tmp/37-e2e-release1.log`、`/tmp/37-check-all1.log`。日志是本地构建
  产物，不入库；本报告只保存文本/数值/hash 摘要。Debug E2E 另有多次
  中间运行（`/tmp/37-e2e-debug1.log` … `debug5`），均因本票新增用例的
  定位/门禁问题失败，最终以 `debug6`（exit 0）为准；中间失败不计入
  通过证据。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述实现 SHA 完全一致；
  本证据文档是后续纯文档提交。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
scripts/check-all.sh                      # Debug + Release startup smoke
scripts/e2e-agent-desktop.py              # Debug
scripts/e2e-agent-desktop.py --release    # Release
```

所有原生门禁运行都在同一个 `mkdir /tmp/arcshot-native-gate.lock`
目录锁下串行执行（`flock` 未安装），并在每次运行前确认
`agent-desktop list-apps --app "ArcShot"` 为空。另一个子代理（#38）在
独立 worktree 中运行，争锁时本票等待而非并发启动第二个实例。

- `cargo fmt --all`：exit 0。
- `cargo clippy … -D warnings`：exit 0，无 error。
- `cargo test --locked --workspace --all-features`：exit 0，47 个测试
  二进制全部 `test result: ok`（合计 308 passed）。
- `scripts/check-all.sh`：exit 0；关键行：

```text
validation_reports=pass directory=docs/validation
dependency_inspection=pass
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=debug identifier=dev.arcshot.app.debug name=ArcShot Debug
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
capture_menu_smoke=pass queued_commands=6 menu_items=7 no_native_capture=true
privacy_smoke=pass resident_pid=9997 network_sockets=0 lsof_status=1
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
clipboard_pngf_smoke=pass width=2 height=2 private_pasteboard=true
save_file_smoke=pass bytes=87 roundtrip=true rejected_destination=true invalid_bytes=true panel_configured=true
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示
  （非 Clippy `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug log=target/e2e-agent-desktop-debug.log cases=37`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release log=target/e2e-agent-desktop-release.log cases=37`。

注：脚本打印的 `cases=37` 指最高编号 MAC-E2E-37（05a/05b 为同一编号的
两个子步）；编号 01…37 连续，无新增缺号。

### 修复的实际缺陷：文字提交后焦点滞留，覆盖层键失效

`sync_text_editor` 在编辑器被移除后没有把窗口焦点还给覆盖层根元素，
焦点滞留在已移除的编辑器元素上。此后覆盖层的 `on_key_down`
（`Cmd+Z` / `Cmd+Shift+Z` 撤销/重做、`Esc` 分层）收不到按键：文字
提交后按 `Cmd+Z` 无任何产品诊断。修复：`commit_editor` 不再提前清除
`text_focus`，`sync_text_editor` 在观察到编辑消失（编辑器或其焦点句柄
仍在）时把焦点返回覆盖层根（`window.focus(&self.focus, cx)`，只有该
路径拥有 `Window`）。

修复前（探针，文字提交后）：

```text
undo1 after text commit: NO UNDO (0 lines)
undo2 after empty-area click: NO UNDO (0 lines)
undo3 after bg click: NO UNDO (0 lines)
undo4 after AX tool click: NO UNDO (0 lines)
```

修复后（同一探针）：

```text
undo1 after text commit: ['… capture_edit action=Undo version=4 annotations=1 … tool=Select …']
```

Debug/Release E2E 的 MAC-E2E-32/35 撤销/重做断言即为该缺陷的回归证据。

## T-02 自动化组（全部 Pass，证据类型逐组标注）

前置均为上述 SHA、`cargo test --locked --workspace --all-features`
（308 passed）。「证据类型」区分固定图像/注入式语义与原生行为：
注入式通过不证明权限、焦点、IME 或原生捕获。

| 组 | 稳定子 ID（节选，完整清单见 TEST.md §3） | 证据类型 | 结果 |
| --- | --- | --- | --- |
| CORE-SESSION | 01～13、14～23、24～27、28～45、46～66、67～76、77～82（`session.rs`、`output.rs`、`save.rs`、`scroll.rs`、`scroll_loop.rs` 等） | core 语义动作（纯逻辑） | Pass |
| CORE-EDIT | 01～12（`editor.rs`）、13～24（`objects.rs`）、25～31（`lines.rs`）、32～34（`blur.rs`）、35～39（`text.rs`） | core 文档动作（纯逻辑） | Pass |
| CORE-COORD | 01～10（`frame.rs`、`document.rs`、`scroll_loop.rs`） | 坐标换算（纯逻辑） | Pass |
| IMAGE-COMPOSE | 01～11、12～18、26～34、35（`compose.rs`、`annotations.rs`、`lines.rs`、`text.rs`） | 固定图像/可复现字体像素真值 | Pass |
| IMAGE-BLUR | 19～25（`blur.rs`） | 固定图像像素真值 | Pass |
| IMAGE-STITCH | 61～72（`stitch.rs` 单元测试，含 #36 边缘空白行回归 72） | 固定帧序列真值 | Pass |
| IMAGE-LIMIT | 01～04（`preview.rs`、`compose.rs`、`document.rs`） | 预算输入（不分配超预算） | Pass |
| APP-EFFECT | 01～03、04～11、12～26、27～37、38～49、50～52（各 `app/tests/*`） | 注入式副作用结果 | Pass |
| APP-ASYNC | 01～04、05～07、08～09、10～12、13～22（取消/重启/过期/迟到/worker panic/队列背压） | 可控调度注入 | Pass |
| APP-FRAME | 01～05（`frame.rs`、`scroll_frame_tasks.rs`） | 注入畸形帧（命名 sRGB/不透明合同） | Pass |
| APP-KEY | 01～18（`key_policy.rs`） | 键位路由纯函数 | Pass |
| CORE-HOTKEY / CORE-DENSITY | 01～04 / 01～02 | 编解码与状态保持（纯逻辑） | Pass |

受控负面族与故障分级的自动化覆盖（注入式，**不冒充原生注入**）：
已知状态停止（内容变化/歧义/缺口/目标/输入保护/帧/资源）且有已验证
历史时提供显式部分选择（CORE-SESSION-67～69、APP-EFFECT-45～49）；
采集前失败只放弃（CORE-SESSION-70）；原生不确定终态（SCK 错误/
超时/worker panic，APP-ASYNC-03/06/14/15、CORE-SESSION-38/74）禁止
任何输出与会话内重启；内屏失效取消整个会话（CORE-SESSION-04/39/74）；
取消/过期/迟到结果不污染新会话（APP-ASYNC-01/02/04/08/09/16/19/22、
CORE-SESSION-41/59/66/75/76）。

## T-03 macOS 实机组（逐稳定用例）

前置均为上述 SHA 的 Debug/Release 正式 bundle。`--headed` 拖拽/点击
只作选区与对象创建的**前置**（选区与对象无 AX 节点，见 TEST.md §4.5）；
验收证据始终是 AX 激活行、`capture_edit`/`output_publish` 等产品文本
诊断与内存 PNG 解码。

| ID | 前置/步骤 | 预期 | 实际（Debug / Release） | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-01/02/09 | 启动；状态项快照；`list-apps`；驻留 `list-windows` | 恰一个状态项、七菜单项；无可捕获自有窗口；唯一 bundle | `mac_e2e_01=pass menu_items=7`、`02=pass own_windows=0`、`09=pass bundle_id=…` | Pass |
| MAC-E2E-03/05a/05b/04 | 菜单开始截图；`--headed` Esc；未注册/已注册全局组合 | 冻结序列三行；`to=Resident action=CancelAttempt`；阴性对照无捕获、已注册恰一次 | 全部 `pass`（热键 `⌃⌥A`） | Pass |
| MAC-E2E-06/10/11 | 截图快捷键…；屏幕录制权限…；AX 录入/保存/关闭 | 窗口出现；权限提示可读可 AX 关闭；组合往返 | `06=pass settings_size=480x363`、`10=pass action=arcshot.notice.close`、`11=pass round_trip=⌃⌥A->⌃⌥9->⌃⌥A` | Pass |
| MAC-E2E-07/17 | 权限提示 Esc；菜单快照读登录项 | `560x282` 窗口 Esc 关闭；`login_item_status` 菜单/启动一致且从不写入 | `07=pass`、`17=pass login_item_actual=Disabled read_only=true` | Pass |
| MAC-E2E-12/13/14 | 冻结覆盖层 AX 取消；工具坞；悬停矩形选项 | `group 截图覆盖层` + cancel；Rectangle checked；`width=5`/`#007AFF` 版本不变 | 全部 `pass` | Pass |
| MAC-E2E-18/19/20/21/23 | Line/Arrow/Blur AX + 悬停选项 | 各工具 checked 互换；样式诊断逐项出现；矩形记忆独立 | 全部 `pass` | Pass |
| MAC-E2E-25 | Text AX + 悬停色板/字号 + 创建提交 | 默认红 16；`size=16.5`、`stroke=#007AFF`；`TextCommit annotations=1` | `pass size=16->16.5 commit=1` | Pass |
| MAC-E2E-15/16/22 | 完成音效 AX 切换；两次重启；密度切换/重启/还原 | 每次 `persisted=true`；重启后 `preferences_launch` 等于切换值；密度切换后工具/样式/选项集保持 | 全部 `pass`（结束时偏好已还原） | Pass |
| MAC-E2E-24 | 无选区点滚动入口；拖拽建选区后点入口 | 无选区 `POLICY_DENIED` 且无 a11y 行；有选区 `scroll_permission missing=none`；目标/拒绝诊断后区域会话保留 | `pass missing=none` | Pass |
| MAC-E2E-26/26_esc | 自有夹具单帧：准备→采集→单击拦截→原始滚动→Enter/Esc | 像素真值逐格相等；`clicks_blocked≥1`；`output_publish Clipboard` + 预览过期；Esc 回原选区 | `pass pixel_oracle=exact`、`26_esc=pass` | Pass |
| MAC-E2E-27 | 自有夹具右/左/内侧三位置慢速双向滚动 | `placed=right/left/inner`；扩展后反向 `duplicate` 且扩展数不增；内侧预览上滚动达目标且像素真值精确 | 三例均 `pass reverse_dedup=true` | Pass |
| MAC-E2E-28 | 慢速扩展后夹具原子跳变 Gap；放弃/接受 | `scroll_stitch_refused gap` + 两项选择 + 不完整警告；点击前无输出；放弃回原选区；接受后才复制 | `abandon=pass`、`accept=pass` | Pass |
| MAC-E2E-29 | 单帧完成→预览→进入标注→a11y 矩形→重新完成；取消轮 | 整图带标注复制并结束；取消不再写剪贴板且字节不变 | `refinish=pass bytes=35957->36438`、`cancel=pass` | Pass |
| MAC-E2E-30/31 | TextEdit/Safari 固定几何：内侧预览、点击拦截、慢速双向、单帧、Enter、双击、Esc | 目标身份/前台/排除集；下滚扩展、上滚去重；内存 PNG 解码 `1200×600` sRGB 不透明 | `30=pass scroll=extended sha=…`、`31=pass … offline=file` | Pass |
| MAC-E2E-37（新） | TextEdit/Safari 单帧完成→缩略图→AX 进入标注→a11y 矩形→AX 完成 | `AddDefaultRectangle annotations=1`；整图 `1200×600` sRGB 复制并结束 | `37_textedit=pass whole_image=1200x600/sRGB`、`37_safari=pass …` | Pass |
| MAC-E2E-32（新） | Region 大选区内五工具各建一对象→Cmd+Z/Cmd+Shift+Z→选对象 AX 删除→AX 完成 | `annotations` 1→5→4→5→4；`output_publish Clipboard pixels=1800x1100 annotations=4`；内存解码等大；磁盘无新增 PNG | `pass tools=Rectangle,Line,Arrow,Blur,Text commit=5 undo_redo=4->5 delete=4 pixels=1800x1100 sRGB=true no_disk_png=true` | Pass |
| MAC-E2E-33（新） | Text 编辑焦点中状态菜单切密度→悬停字号步进→工具切换提交→切回密度 | 编辑器/字号 16/Text 工具保持；同套选项在新密度可达；`size=16.5`；`TextCommit annotations=1` | `pass density=Compact->Standard->Compact text_focus=preserved size=16->16.5 commit=1` | Pass |
| MAC-E2E-34（新） | Region：AX 保存→原生面板 AX 取消；新会话 AX 保存→改名→AX 保存 | 取消：`save_panel_outcome=cancelled`、无输出、剪贴板 hash 不变、会话保留；选定：`save_panel_outcome=chosen`、`output_publish target=SaveFile pixels=1200x700`、文件解码等大 sRGB、剪贴板不变（文件随后删除） | `34=pass save_cancel=true clipboard_unchanged=true`、`34_save=pass chosen=true target=SaveFile sha256=44298b991e7f file_removed=true` | Pass |
| MAC-E2E-35（新） | 自有夹具扩展拼接→完成→预览→进入标注→五工具各建一对象→撤销/重做→重新完成 | 长图文档 `230×196/212`；滚动入口缺席；`annotations` 1→5→4→5；`output_publish pixels=460×392/424 annotations=5` 等于已验证底；解码高度一致 | `pass long_image=230x196 tools=… commit=5 undo_redo=4->5 pixels=460x392 sRGB=true`（Release 424） | Pass |
| MAC-E2E-36（新） | Region 建对象后全局热键重启；工具激活时 Esc；采集中全局热键重启 | 重启后无旧选区、`SelectRegion annotations=0`、无旧票输出；首 Esc 只回 Select 工具；采集中重启后 `scroll_input_retired`、无旧票输出、新会话需重选 | `36_region=pass`、`36_esc=pass`、`36_scroll=pass` | Pass |

原生诊断抽样（Debug，Release 同形）：

```text
capture_edit action=Undo version=7 annotations=4 selection=Some([200.0, 167.0, 900.0, 550.0]) tool=Select selected=None style=none
capture_edit action=Redo version=8 annotations=5 selection=Some([200.0, 167.0, 900.0, 550.0]) tool=Select selected=None style=none
save_panel_open version=1
save_panel_outcome=cancelled version=1
save_panel_outcome=chosen version=1
output_publish target=SaveFile bytes=115146 pixels=1200x700 annotations=0
completion_annotation_opened ticket=CaptureTicket(28) document=230x196 zoom=1.000
```

## Not tested（诚实清单，含原因）

- **运行中直接撤销 Screen Recording / 三权限缺失通知 / 设置返回重检**：
  本机三项权限均已授权，改动 TCC 需用户参与（§4.4）。设置切换若先
  触发焦点停止只能记录实际时序；本票未切换系统设置，故不冒充直接
  撤权路径已验。缺失分支仅由 APP-EFFECT-01～03/38～41 注入覆盖。
- **选区移动/八向调整、对象/端点控制点拖动、重选指针手势**：控制点
  与对象无 AX 节点，`--headed` 坐标拖拽不作验收证据。对象创建拖拽
  仅作前置，语义由 core `editor.rs`/`objects.rs` 与 `capture_edit`
  诊断覆盖；指针手势本身保持 **Not tested**。
- **中文 IME 组合/候选窗、emoji 原生字形、字号直接输入**：仅以物理
  按键输入 ASCII `a` 并以 `TextCommit` 为证；IME 候选窗属 §4.4。
  字号直接输入由 core 取整与步进覆盖。
- **动态内容、固定表头/浮层、重复/低纹理、横向/对角/嵌套滚动**：
  原生仅覆盖自有夹具 Gap 受控停止（MAC-E2E-28）；其余负例仅由
  IMAGE-STITCH 注入证明安全处理，负面通过不外推为兼容。
- **目标移动/关闭/前台变化、几何/尺度变化、外部遮挡的原生复现**：
  仅由注入式 CORE-SESSION-33～42/49/57/58/65/74 覆盖；本票原生只
  驱动了 Esc、热键重启与取消。原生 SCK 错误/超时/worker panic 注入
  同样仅注入式（APP-ASYNC-06/14/15），未做原生故障注入。
- **热键冲突恢复、登录项真实注册/注销与失败提示**：会改动系统状态，
  未执行（APP-EFFECT-05～11/28～32 注入覆盖）。
- **保存覆盖写提示、保存失败原生形态、向第三方应用实际粘贴**：
  显式保存已证选定写入与内存解码；覆盖/失败/第三方粘贴未跑。
- **多自有窗口任意数量的精确排除**：已证固定排除集（控制窗 + 固定
  预览窗 + 覆盖层）与预览 id 入集；任意多窗口场景未逐一构造。
- **T-04 性能与资源门槛（PERF-*/RESOURCE-MAX）**：不在本票
  T-02/T-03/T-05 范围内，未测量。
- **视觉观感、悬停不延长的计时、真机滚轮手感、多显示器**：
  属 §4.4 用户项，保持 **Not tested**。

## 隐私

- `scripts/check-no-network.sh`（经 `check-all.sh`）：驻留进程
  `network_sockets=0`（`lsof -i` 无网络/监听 socket）。运行期无网络、
  监听或回环。
- MAC-E2E-32 断言剪贴板完成前后仓库树 PNG 文件集不变（内存解码）；
  MAC-E2E-34 断言保存取消与显式保存均不改变剪贴板。显式保存的
  `~/Documents/ArcShot-37-regression.png` 在哈希与解码取证后已删除。
  除该用户显式保存外，无截图落盘；证据仅文本/数值/hash。

## 给下一票的接缝

- 焦点修复的接缝在 `CaptureView::sync_text_editor`：编辑消失时把焦点
  返回覆盖层根（`self.focus`）。任何新增「临时接管焦点」的编辑器类
  产品面（例如字号直接输入）必须在退出时走同一返回路径，否则会重现
  本票缺陷。
- 新 E2E 组合依赖的按钮均已 AX 化，无需新接缝：`arcshot.overlay.delete`
  （有选中对象时）、原生保存面板的 `CancelButton` / `OKButton` /
  `saveAsNameTextField`（AppKit 自带 AX id，经 `snapshot --app` 可见）。
  保存面板没有标题窗口，`wait_frontmost` 不适用于它，须用 AX 按钮。
- 长图拖拽坐标按 `coordinator::open_completion_annotation` 的居中公式
  换算（viewport 1470×956、agent `--xy` 的 y 需 +33），见 MAC-E2E-35
  的 `drag_long`；文档内留足边距可同时吸收 ±33 的坐标约定差异。
- 文字提交不得用「选区内点击」作手势（Text 工具下会开始新编辑器，
  焦点语义混乱）；MAC-E2E-32/33/35 统一用工具切换提交（AX 证据）。
