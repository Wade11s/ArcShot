# #63 选区与标注对象的可访问性接缝：验证记录

本记录覆盖 `ticket/63-a11y-selection-objects`（基点 `spec/v1` `fbfb7a6`）上的产品改动：
选区、八个选区控制点、每个标注对象、对象控制点与线/箭头两个端点的 AX 节点与动作。
它**不是**完整 38 用例矩阵的验收证据：本轮只运行 `quiet` 与 `ax` 两档（用户在用这台
Mac），`physical` / `full` 未运行。

## 环境

- 实测实现/脚本 SHA：`e9fde1a8c2d445b33a085575b6cca9f5e47d1f32`（`worktree_dirty=false`；
  产品与脚本在此提交，`TEST.md` 与本文档的后续 `docs:` 提交不改行为）。
- 时间：2026-11-06T07:02:35Z（`scripts/record-validation.sh` 采集）。
- 构建配置：Debug。bundle `dev.arcshot.app.debug` / 显示名 `ArcShot Debug`，签名身份 `ArcShot Dev`。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (Build version 27A266a)。
- SDK：27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64。
- macOS：27.2 (26B123)。
- 显示器：内建单屏 1470×956 逻辑点，scale=2（选区节点 AX bounds 1470×956 与之一致）。
- 权限：`accessibility=granted`、`screen_recording=granted`；未改动任何 TCC 设置。
- 夹具：本票不使用夹具；MAC-E2E-38 与 MAC-E2E-24 在真实冻结帧上运行，不涉及
  `arcshot-scroll-fixture`、TextEdit 或 Safari。
- 日志：`target/e2e-63-ax.log`（脚本 stdout）、`target/e2e-agent-desktop-debug.log`
  （产品 stderr）、`target/63-check-all.log`（仓库门）。日志是本地构建产物，不入库。
- `agent-desktop`：1.2.0（envelope 2.6）。

## 两个决定

实现前已登记在 #63 的评论（`gh issue comment 63`），摘要：

1. **手势证据取 (a)**：AX 动作驱动与指针相同的 core 动作序列。`SetValue` 的载荷经
   `a11y_gestures::affordance_commands` 翻译成与 overlay 鼠标处理器**同一组纯函数**
   （`capture_press_action` / `capture_long_image_press_action`）解析出的
   `UiCommand` 序列，进入同一个 `UiQueue`，再由 `CaptureAction` → `Document` 执行。
   证明的是 core 手势逻辑与结果文档状态；真实指针动态不在此列。
2. **多对象暴露取逐对象 `id` + `role`**，不用 `a11y_synthetic_children`：与 #53/#54/#55
   同一机制，snapshot 可见、`native_id` 即稳定外部 id、驱动方式一致。

## 实现中由证据修正的两点

- **角色必须是 `textfield`，不是 `group`/`button`**。`group` 节点拿不到 ref
  （`agent-desktop` 只为可交互角色或声明主动作的节点分配 ref），`button` 节点拿不到
  `SetValue`（`set-value` 报 `editable (role button is not editable)` 与
  `supported_action (SetValue is not available)`）。`textfield` 同时满足两者，实测
  `available_actions: ["SetValue", "SetFocus"]`。因此 #63 评论里的 `group` 角色在实现
  时改为 `textfield`；`accessibility_id` 清单未变。
- **控制点落在选区外时与指针一致地开始新框选**。第一次跑 MAC-E2E-38 时对象右下控制点
  在选区外，AX 手势复现了指针语义（`capture_press_action` 要求 `inside`），产生
  `capture_edit action=SelectRegion` 并清空对象。这不是缺陷，是同一权威路径的证据；
  用例几何因此改为让所有控制点落在选区内。

## 命令与结果

```sh
# 单元表与档位
scripts/e2e-agent-desktop.py --list
# → 38 cases in 21 units；ax 档：notice_ax(10)、selection_ax(38)、scroll_gate(24)

# 只跑不打扰使用者的两档（用户在用这台 Mac）
scripts/e2e-agent-desktop.py --tier ax --no-build > target/e2e-63-ax.log 2>&1
# → exit 0；mac_e2e=partial … cases=10/38 acceptance=false（部分运行，非完整矩阵证据）
# → e2e_timing total_s=23.1 agent_desktop_calls=108
```

`target/e2e-63-ax.log` 的逐用例结果：

```
mac_e2e_09=pass bundle_id=dev.arcshot.app.debug name=ArcShot Debug
mac_e2e_01=pass menu_items=7 refs=8
mac_e2e_17=pass login_item_actual=Disabled source=menu source=launch read_only=true
mac_e2e_02=pass own_windows=0
mac_e2e_10=pass notice_texts=2 action=arcshot.notice.close readable=46
mac_e2e_38=pass selection=ax handles=8 object=ax object_handles=8 endpoints=2 annotations=2 gestures=8
mac_e2e_24=pass entry_gate=ax selection_precondition=ax_set_value missing=none
mac_e2e_15=pass sound=true->false->true selector=toggleCompletionSound:
mac_e2e_08=pass exit=0 status_item=removed
mac_e2e_16=pass restart_restored=false restart_restored=true preference=local
```

### MAC-E2E-38 引用的日志行

`target/e2e-agent-desktop-debug.log`（同一轮，行号为该文件）：

```
30 a11y_action=arcshot.overlay.selection action=SetValue value=200,200,700,600
31 capture_edit action=SelectRegion version=1 annotations=0 selection=Some([200.0, 200.0, 500.0, 400.0])
33 a11y_action=arcshot.overlay.selection.handle.bottom_right action=SetValue value=800,700
34 capture_edit action=PointerPressed version=1 … selection=Some([200.0, 200.0, 500.0, 400.0])
36 capture_edit action=PointerReleased version=2 … selection=Some([200.0, 200.0, 600.0, 500.0])
38 a11y_action=arcshot.overlay.selection.body action=SetValue value=500,430
41 capture_edit action=PointerReleased version=3 … selection=Some([200.0, 180.0, 600.0, 500.0])
43 a11y_action=arcshot.overlay.selection action=SetValue value=20,30,320,330
44 capture_edit action=SelectRegion version=4 … selection=Some([20.0, 30.0, 300.0, 300.0])
46 a11y_action=arcshot.overlay.tool.rectangle action=Click
47 capture_edit action=SelectTool version=4 … tool=Rectangle
49 a11y_action=arcshot.overlay.object.new action=SetValue value=40,50,140,120
52 capture_edit action=PointerReleased version=5 annotations=1 … selected=Some(0)
57 a11y_action=arcshot.overlay.object.0 action=Click
60 capture_edit action=PointerReleased version=5 annotations=1 … selected=Some(0)
62 a11y_action=arcshot.overlay.object.0.handle.bottom_right action=SetValue value=180,150
65 capture_edit action=PointerReleased version=6 annotations=1 … selected=Some(0)
67 a11y_action=arcshot.overlay.object.0 action=SetValue value=60,70,200,170
70 capture_edit action=PointerReleased version=7 annotations=1 … selected=Some(0)
75 a11y_action=arcshot.overlay.object.new action=SetValue value=30,40,110,120
78 capture_edit action=PointerReleased version=8 annotations=2 … selected=Some(1)
83 a11y_action=arcshot.overlay.object.1 action=Click
86 capture_edit action=PointerReleased version=8 annotations=2 … selected=Some(1)
88 a11y_action=arcshot.overlay.object.1.endpoint.end action=SetValue value=120,130
91 capture_edit action=PointerReleased version=9 annotations=2 … selected=Some(1)
93 a11y_action=arcshot.overlay.cancel action=Click
94 capture_phase from=Frozen to=Resident action=CancelAttempt
116 a11y_action=arcshot.overlay.selection action=SetValue value=400,300,700,500   # MAC-E2E-24
117 capture_edit action=SelectRegion version=1 … selection=Some([400.0, 300.0, 300.0, 200.0])
```

节点清单（同一轮 `snapshot --app`，`native_id` 即 `AXIdentifier`）：

```
textfield  arcshot.overlay.selection                        value=200,200,700,600  acts=[SetValue, SetFocus]
textfield  arcshot.overlay.selection.handle.top_left         value=200,200
textfield  arcshot.overlay.selection.handle.top              value=450,200
textfield  arcshot.overlay.selection.handle.top_right        value=700,200
textfield  arcshot.overlay.selection.handle.right            value=700,400
textfield  arcshot.overlay.selection.handle.bottom_right     value=700,600
textfield  arcshot.overlay.selection.handle.bottom           value=450,600
textfield  arcshot.overlay.selection.handle.bottom_left      value=200,600
textfield  arcshot.overlay.selection.handle.left             value=200,400
textfield  arcshot.overlay.selection.body                    value=450,400
textfield  arcshot.overlay.object.new                        value=none → 最后一个对象范围
textfield  arcshot.overlay.object.0                          value=40,50,140,120   acts=[Click, SetValue, SetFocus]
textfield  arcshot.overlay.object.0.handle.top_left … .left   value=各控制点中心
textfield  arcshot.overlay.object.1                          value=30,40,110,120
textfield  arcshot.overlay.object.1.endpoint.start           value=30,40
textfield  arcshot.overlay.object.1.endpoint.end             value=110,120
```

## 逐用例结果

| 用例 | 档位 | 结果 | 证据 |
| --- | --- | --- | --- |
| MAC-E2E-38（新增，ax） | `ax` | **Pass** | 上节日志行；选区/控制点/对象/端点全部经 `set-value` 与 `Click` 驱动，每次都有 `a11y_action=` 与 `capture_edit` |
| MAC-E2E-24（由 physical 降为 ax） | `ax` | **Pass** | `mac_e2e_24=pass entry_gate=ax selection_precondition=ax_set_value missing=none`；选区前置改为 `set-value`，不再有 `--headed` 拖拽 |
| MAC-E2E-09/01/17/02（quiet） | `quiet` | **Pass** | `mac_e2e=partial … cases=10/38` 中的对应行 |
| MAC-E2E-10（ax） | `ax` | **Pass** | `mac_e2e_10=pass notice_texts=2 …` |
| MAC-E2E-15/08/16（quiet） | `quiet` | **Pass** | 同上 |
| MAC-E2E-03/04/05a/05b/06/07/11/12/13/14/18/19/20/21/22/23/25/26/27/28/29/30/31/32/33/34/35/36/37 | `physical`/`full` | **Not tested** | 本轮按用户在用机器的规则只跑 `quiet`/`ax`；`physical`/`full` 会冻结内建屏并发送真实键鼠，未运行。这些用例的既有结论不因本票改变 |
| MAC-REGION / MAC-ANNOTATION 的**真实指针拖拽动态** | — | **Not tested** | AX 手势证明的是 core 手势逻辑与结果文档状态，不是按下—移动—释放的连续动态与手感；不得据本票声称指针手势已验收 |
| 长图上非矩形工具的创建 | — | **Not tested** | 长图表面不注册选区与 `object.new`；`arcshot.overlay.completion.add_rectangle` 只覆盖矩形。其余工具在长图上仍只能拖拽创建 |

## 仓库门

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-architecture.sh
./scripts/check-all.sh
```

结果（同一候选）：

| 门 | 命令 | 结果 | 证据 |
| --- | --- | --- | --- |
| 格式 | `cargo fmt --all --check` | Pass | 无输出，exit 0 |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | Pass | `Finished dev profile`，无 warning |
| 单测 | `cargo test --workspace` | Pass | 全部 `test result: ok`；`arcshot-app` 26 项含 24 项 `a11y_gestures` |
| 架构 | `./scripts/check-architecture.sh` | Pass | `architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies` |
| 完整入口 | `./scripts/check-all.sh` | Pass | `target/63-check-all.log`，exit 0（含 fmt/clippy/test/doc/依赖/架构/文字后端/启动/禁网/Release 启动与 `validation_reports=pass`） |
| E2E（ax 档） | `scripts/e2e-agent-desktop.py --tier ax --no-build` | Pass（部分运行） | `target/e2e-63-ax.log`，exit 0，`mac_e2e=partial … cases=10/38 acceptance=false`，`total_s=23.1 agent_desktop_calls=108` |

## 未能验证与残余风险

- **未运行 `physical` / `full`**：用户在用这台 Mac，两档会冻结显示器并发送真实输入。
  因此完整 38 用例矩阵（`mac_e2e=pass … cases=38`）未取得，本轮只有
  `mac_e2e=partial … cases=10/38`。这不是验收证据，只是本票新增面的证据。
- **真实指针动态未验证**：AX 手势中途只有一次 `PointerMoved`（锚点 → 目标），
  与真实拖拽的多次移动不同。core 的钳制与最小尺寸逻辑由单测覆盖，但连续移动下的
  视觉预览未在 AX 路径上验证。
- **`textfield` 角色的语义代价**：选区与对象在 AX 树里是文本字段，屏幕阅读器会按
  “可编辑文本”朗读。这是 `agent-desktop` 只对可编辑角色提供 `set-value` 的直接后果
  （`group` 无 ref、`button` 无 `SetValue`）。若上游放开任意角色的 `SetValue`，应改回
  `group`/`button` 并重跑 MAC-E2E-38。
- **状态项枚举的时序**：本机首次 `find --surface menu-bar-extras --app …` 可能在状态项
  发布前返回 `ELEMENT_NOT_FOUND`。脚本的 `find_ref` 因此改为有界重试（10 s，仅对
  `ELEMENT_NOT_FOUND`）。这是观察竞态，不是产品行为改变。
- **`arcshot.overlay.object.new` 的 value 是派生值**（最后一个对象的范围），不是独立
  状态；撤销/重做后会指向另一个对象。它只作为“上一次创建产生了什么”的提示，载荷才是
  权威输入。

## 审查后的修复（本机未重跑 E2E）

两轴审查（Standards / Spec）在本票提出后指出三项缺陷，已修复并有单元证据：

| 缺陷 | 修复 | 证据 |
| --- | --- | --- |
| `arcshot.overlay.selection.body` 按选区中心取值；对象覆盖中心时该节点移动的是对象而不是选区 | 选区体锚点改为“选区内未被对象覆盖的点”（优先中心，否则向内探测，全被覆盖时回退中心） | `the_selection_body_presses_a_point_no_object_covers` |
| Select 下双击已有文字重编没有 AX 路径 | 文字对象新增 `arcshot.overlay.object.<i>.edit`，Click 走 `capture_text_press_action` | `a_text_object_exposes_the_select_tool_re_edit` |
| AX 动作在文字编辑中只入队 `TextCommit`，不调用 `commit_editor`，实时编辑器要等下一个 tick 才消失 | 两个 AX 闭包与指针一致：先 `commit_editor`，本身不再开始新动作 | 代码路径对齐 `ui.rs` 指针处理器；无无头测试 |

另修正本文件的 `total_s` 笔误（25.8 → 23.1）与 `TEST.md` §4.2 的重复编号（两个 6 → 6/7）。

### 未重跑 E2E 的原因

`scripts/e2e-agent-desktop.py --tier ax` 在本机无法完成，两个原因都与本票代码无关：

1. **显示前置不满足**：`selection_ax` 之前报 `the display is not clean`，列出 Notification Center 与 `loginwindow` layer 2001/2004 —— 录制期间本机已锁屏。这是前置条件，不是产品失败。
2. **`notice_ax` 在未改动的基线上同样失败**：在 `e2edb74`（本票上一提交，未含上述修复）上跑 `--only notice_ax`，得到完全相同的
   `ACTION_NOT_SUPPORTED: Window 'w-…' exists but is not exposed through accessibility`。本机 `agent-desktop` 已是 1.2.0（本票原记录为 1.1.0），因此这是工具或环境变化，不是本票回归。

**因此本文件的 E2E 证据仍属于 `e2edb74`**，上述三项修复只有单元证据，没有新的 `mac_e2e` 记录。`mac_e2e=partial … acceptance=false` 的结论不变；全矩阵与物理输入项仍为 Not tested。合并前若要求 E2E 覆盖这三项，须在解锁且显示干净的机器上重跑 `--tier ax` 与 `--tier physical`。

## agent-desktop 1.2.0 适配后的重跑

本机 `agent-desktop` 由 1.1.0 升到 1.2.0 后，`--tier ax` 在**未改动的基线** `e2edb74` 上
也失败：`ACTION_NOT_SUPPORTED: Window 'w-…' exists but is not exposed through accessibility`。
原因是 1.2.0 对从未成为前台的应用窗口只返回 AppKit 边框按钮，GPUI 子树不出现。

接缝改动（`scripts/e2e-agent-desktop.py`）：

- `start_product` 在直接启动被测二进制后用一次 `osascript` 把该 pid 设为 `frontmost`，
  子树随后发布并在失去前台后保持。这是 ax 档允许的唯一 `osascript`：不注入按键或指针。
- 直接启动保留启动终端的屏幕录制授权；经 `open`（LaunchServices）启动需要该 bundle
  自己的 TCC 授权，属于用户决定，因此没有采用。
- 代价：MAC-E2E-01 的“不要求 ArcShot 在前台”不再是独立证明，`TEST.md` §4.5 已记录。

重跑结果（同一候选，`--tier ax`，Debug）：

```
scripts/e2e-agent-desktop.py --tier ax
# → exit 0；mac_e2e=partial … cases=10/38 acceptance=false
# → mac_e2e_10=pass notice_texts=2 action=arcshot.notice.close
# → mac_e2e_38=pass selection=ax handles=8 object=ax object_handles=8 endpoints=2
#              annotations=2 gestures=8
# → mac_e2e_24=pass entry_gate=ax selection_precondition=ax_set_value missing=none
```

`scripts/check-all.sh` 同时重跑（fmt、clippy `-D warnings`、全部测试、doc、依赖/架构/文字
后端/启动/禁网、Release 启动），exit 0。

仍未覆盖：`physical` / `full` 档（真实指针与键盘），以及文字内容写入——编辑器不提供
`AXValue` 写回，`type` 在 ax 档报 `POLICY_DENIED`，所以 `object.new` 的 Click 与
`object.<i>.edit` 只有单元证据，端到端仍记 Not tested。
