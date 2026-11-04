# #24 选区移动、调整、重选与可撤销历史验证记录

本记录只覆盖 #24 落地的自动化证据。选区移动/八向调整、重选与 `Cmd+Z`/`Cmd+Shift+Z`
的真实指针与键盘手势、控制点视觉以及在原生覆盖层上的裁剪输出仍为 **Not tested**：
控制点目前没有 AX 节点，`TEST.md` §4.3 禁止用 `--headed` 坐标点击补足验收。测试过程
未捕获屏幕、未保存取证图像、未读写用户剪贴板。

## 实测来源与环境

- 基线 SHA：`65343e56fec7fafc3149f4c84877f962aa59ebac`（`spec/v1`，#22 合并点）。
- 实测实现/脚本 SHA：`b2f2e2dce4698a8025ae017680b394b3c4c9d522`。
- 时间：2026-11-04T04:34:20Z（record-validation 采集）；完整入口在同一提交上完成。
- 构建配置：Debug 与 Release。Rust 自动化运行在 Debug 测试配置；完整入口另构建并冒烟
  Debug/Release 正式 bundle。报告本身是之后的纯证据/文档提交。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (Build version 27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建屏 display_id=1，origin=(0,0)，1470×956 逻辑点，scale=2（2940×1912 物理像素）。
- 权限：证据采集时 Screen Recording preflight=true，guide_seen=true；未请求或改动
  Input Monitoring、Accessibility、通知。没有执行真实捕获，权限值不代表捕获通过。
- 夹具：2x 的 200×100 物理像素不透明 BGRA 全 255 frame；4×3/stride 24 的通道真值
  frame；均为合成夹具，不含用户截图。夹具 v1。
- 日志：`target/24-check-all.log`、`target/24-capture-environment.log`、
  `target/24-build.log`、`target/24-validation-debug.txt`。日志是本地构建产物，不入库；
  本报告保存摘要。
- `worktree_dirty=true`（record-validation 采集时只有 `TEST.md` 与本文档未提交；
  两者都是文档，产品代码、测试与构建输入均与实测 SHA 一致）。

## 执行

```sh
scripts/check-all.sh > target/24-check-all.log 2>&1             # exit 0
scripts/build-app.sh > target/24-build.log 2>&1                 # exit 0
target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot \
  --check-capture-environment > target/24-capture-environment.log 2>&1
scripts/record-validation.sh --configuration debug \
  --logs target/24-check-all.log,target/24-capture-environment.log \
  > target/24-validation-debug.txt
cargo test --locked -p arcshot-core --test editor
cargo test --locked -p arcshot-core --test session_edit
cargo test --locked -p arcshot-core --test document
cargo test --locked -p arcshot-app --test key_policy
cargo test --locked -p arcshot-image --test compose
```

完整入口 exit 0：fmt、Clippy warnings-as-errors、doc、95 个测试、验证工具/依赖/架构/真实
文字后端、Debug/Release 原生驻留/退出、无网络与 PNGf 剪贴板冒烟全部通过。原生冒烟摘要：
`startup_smoke=pass`（Debug 与 Release）、`bundle_identity_smoke=pass`（Debug/Release
身份正确）、`clipboard_pngf_smoke=pass width=2 height=2 private_pasteboard=true`、
`privacy_smoke=pass network_sockets=0`、`capture_menu_smoke=pass queued_commands=3`、
`hotkey_registration_smoke=pass default=⌘⇧X suspend_resume_cleanup=clean`、
`capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true`。
环境预检输出：`capture_environment display_id=1 origin=0,0 logical=1470x956 scale=2 screen_recording=true guide_seen=true`。

## 原生 E2E（agent-desktop）与 AX 合同

在同一个 worktree（代码 SHA 同上，文档未提交）上用 Debug bundle 执行：

```sh
scripts/e2e-agent-desktop.py > target/24-e2e-debug.log 2>&1   # exit 0
```

`agent-desktop 1.1.0`（envelope 2.6），`status` 为 accessibility=granted、
screen_recording=granted。12 个稳定用例全部通过，其中与本票直接相关的是 MAC-E2E-12：
冻结覆盖层的 AX 树仍为 `group "ArcShot 截图覆盖层"` + `button arcshot.overlay.cancel`，
AX 激活后记录 `a11y_action=arcshot.overlay.cancel action=Click`，会话回到
`capture_phase … to=Resident`。其余用例覆盖菜单、热键、权限提示与退出，未被本票改动。
E2E 未保存截图，也未触发 Finish/剪贴板。日志：`target/24-e2e-debug.log`、
`target/e2e-agent-desktop-debug.log`。

最终代码提交上的 E2E 首次运行即 12/12 通过。作为完整记录：本票早期代码提交
`f499cdcb3a02e843cebb70e810043957cd8b860d` 的首次尝试在 MAC-E2E-04 等待注册热键触发的
第二个 `capture_request_us=` 时超时（此前 MAC-E2E-01/02/03/05 均通过）；未改热键代码、
未改权限，立即重跑后 12/12 通过。该次失败按全局热键投递的瞬时环境问题记录，不作为产品
通过，也不隐藏。本票没有修改热键路径。

E2E 无法驱动选区拖动、八向控制点、重选或 `Cmd+Z`：控制点没有 AX 节点，`--headed` 坐标
点击按 `TEST.md` §4.3 不能作为验收证据。因此这些行为保持 Not tested。

## 稳定自动化子用例

前置为上述 SHA、锁定依赖和合成夹具；不需要屏幕权限，也不触发原生捕获。断言都从
`Document`/`CaptureSession` 的公开状态与 crop 观察，不读私有字段。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-COORD-08 | 选区 (10,5) 30×20；内部拖动 +2/+1 点；再从右下角 +5/+5 点 | 裁剪分别为 (24,12) 60×40 与 (24,12) 70×50，只做一次 2x 换算 | 与预期完全一致 | Pass |
| CORE-COORD-09 | 30×20 选区；八个控制点中心、选区中心、1×1 极小选区中心 | 中心命中各自 handle；选区中心无 handle；极小选区中心仍是移动区 | 完全一致 | Pass |
| CORE-EDIT-01 | 两个标注；在选区内按下拖动 +4/+3 点 | 拖动中实时跟随且版本不变；一次 undo 恢复整个拖动及两个标注；redo 重放；拖动中撤销被忽略 | 全部一致 | Pass |
| CORE-EDIT-02 | 选区 (10,5) 30×20；拖到屏幕右下/左上之外 | 选区停在 (70,30)/(0,0)；标注按实际位移平移，保持相对位置 | 完全一致 | Pass |
| CORE-EDIT-03 | 选区内原地点按；整屏选区再移动；随后 undo | 两次 no-op 不推进版本、不入历史；undo 直接回到上一次标注创建 | 完全一致 | Pass |
| CORE-EDIT-04 | 从右下角控制点拖动；检查标注 | 选区变为 10,5 40×30；标注位置/尺寸不变；整次拖动一个版本；undo 恢复选区 | 完全一致 | Pass |
| CORE-EDIT-05 | 依次拖动八个控制点到指定位置 | 每个 handle 只改变自己负责的边；标注几何始终不变 | 八向全部一致 | Pass |
| CORE-EDIT-06 | 右下角拖出屏幕再反向越过对边，再拖回 | 先夹在 (10,5) 90×45，再夹到 0.5×0.5 逻辑点（2x 下 1 像素），不反转；拖回恢复 | 完全一致 | Pass |
| CORE-EDIT-07 | 已有选区与两个标注；重选新矩形 | 旧标注立即清空且无提示；undo 恢复旧选区及两个标注；redo 再替换 | 完全一致 | Pass |
| CORE-EDIT-08 | 标注 25,25 10×10，选区初始 10,10 20×20 | 预览裁剪 5×5；扩大选区显示完整标注；缩小到不重叠时为 None，但 `bounds` 数据保留 | 完全一致 | Pass |
| CORE-EDIT-09 | 两个标注按创建顺序；移动选区后 undo | 层叠顺序保持自下而上；undo 恢复原始顺序与几何 | 完全一致 | Pass |
| CORE-EDIT-10 | 同一矩形重选两次 | 第二次不推进版本；一次 undo 回到无选区而非重复步骤 | 完全一致 | Pass |
| CORE-EDIT-11 | 八轮重选/新增标注/undo/redo | 每次编辑后底图仍是同一分配，不按步骤复制整图 | 指针地址始终一致 | Pass |
| CORE-EDIT-12 | 选区右/下边缘带 5e-7 点舍入余量；整屏拖动再拉回右边缘 | 夹取范围不得反转/panic；无移动不产生历史；边缘拉回 100.0 | 修正后无 panic，选区精确为 100×50 | Pass |
| CORE-SESSION-14 | Frozen 后建立选区；`SelectionDragStarted/Updated/Ended` | 拖动中会话文档实时变化且版本不动；结束时 +1；undo/redo 恢复/重放 | 完全一致 | Pass |
| CORE-SESSION-15 | 重选新矩形后 undo/redo | 整文档替换为一步；undo 恢复旧选区，redo 再替换 | 完全一致 | Pass |
| CORE-SESSION-16 | 已有历史时再次 Start 并完成新捕获 | 新文档无选区、版本 0、历史为空；undo 无操作 | 完全一致 | Pass |
| CORE-SESSION-17 | Finish 输出在途时 Undo/Redo/拖动 | 全部忽略、保持 Frozen；剪贴板失败释放互斥后 undo 生效，旧 job `may_publish=false` | 完全一致 | Pass |
| CORE-SESSION-18 | 旧 ticket 的重选、拖动与 undo/redo 到达新 attempt | 全部忽略；新文档选区与拖动状态不改变 | 完全一致 | Pass |
| CORE-SESSION-19 | 新会话无历史时 Undo/Redo | 不改版本、不产生选区 | 完全一致 | Pass |
| APP-KEY-01 | 鼠标按住时 Esc/Enter | 两者均不取消/完成 | None/None | Pass |
| APP-KEY-02 | 输出期 Enter/Esc | Enter 忽略；Esc 仍可取消，迟到结果由版本栅栏拒绝 | None / Cancel | Pass |
| APP-KEY-03 | 空闲 Enter/Esc/其他键 | 仅映射 Finish/Cancel | Finish/Cancel/None | Pass |
| APP-KEY-04 | `Cmd+Z`、`Cmd+Shift+Z`、裸 Z、`Cmd+Option+Z`、`Cmd+Control+Z` | 仅前两者映射 Undo/Redo | 完全一致 | Pass |
| APP-KEY-05 | 输出期或拖动中的撤销/重做 | 全部忽略 | 四个组合均为 None | Pass |
| APP-KEY-06 | 选区内点、左上控制点点 | 路由到 core 拖动（移动或调整） | Drag/Drag | Pass |
| APP-KEY-07 | 选区外点、无选区点 | 路由为新选区的 view band | Band/Band | Pass |
| APP-KEY-08 | 选区内双击、选区外双击、控制点双击、输出期按下 | 空白双击 FinishAt；控制点双击保留调整；输出期全部忽略 | 完全一致 | Pass |
| IMAGE-COMPOSE-04 | 合成 4×3 真值 frame；移动 +0.5 点、再调整右下角 | crop (1,0) 2×2 与 (1,0) 3×2，RGBA 像素与夹具一致 | 像素完全一致 | Pass |

## 设计与合同说明

- 历史只保存 `selection + annotations + version` 编辑状态，底图仍是同一个
  `CapturedFrame`（内部 `Arc` 字节）；没有按步骤复制整图，也没有第二条裁剪路径。
  移动/调整后的输出继续走 `Document::crop` → `pixel_rect` → `compose_crop`。
- 一次拖动在按下时记录 baseline，移动只改实时状态，松开时若与 baseline 相同则不入历史，
  否则只推入一个 baseline 并推进一个版本；undo/redo 也各推进一个版本，所以输出栅栏不会
  把撤销前的 job 当作当前文档。
- 选区移动被夹在单一内屏内；标注按实际（夹取后的）位移平移。八向调整只改选区边，
  标注数据完全不动；每次候选选区都先过同一个 `pixel_rect` 上限检查，再应用。
- 标注列表按创建顺序自下而上，几何使用图像内逻辑坐标；越界数据保留，预览/输出通过
  `Annotation::visible_bounds` 裁剪。本票没有绘制工具，所以标注只进入模型，不参与栅格化。
- 覆盖层绘制八个 8 点控制点，命中区域与控制点中心同源（半宽 4 点，且不超过选区边长
  三分之一）；保留 `arcshot.overlay.cancel`、`arcshot.overlay.finish` 与根分组名称。
- 新增纯文本诊断 `capture_edit action=… version=… annotations=… selection=…`，只在重选、
  拖动提交、撤销与重做后打印，供后续 E2E 观察；不打印像素。本票未用真实手势触发它。

## 限制与未测试

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-REGION-MANUAL-24-01 | 授权后用真实指针在冻结画面上拖出选区，再拖动选区内空白与八个控制点 | 选区只在内屏内移动/调整；标注相对位置保持、调整不缩放；crop 与显示尺寸一致 | 控制点与选区没有 AX 节点，`--headed` 坐标点击不能作为验收证据；未执行 | Not tested |
| MAC-REGION-MANUAL-24-02 | 已有选区与（未来）标注时在选区外拖动重选，再 `Cmd+Z` / `Cmd+Shift+Z` | 立即替换且可整体撤销/重做；单一活动选区 | 依赖真实键盘手势与覆盖层焦点，未执行 | Not tested |
| MAC-REGION-MANUAL-24-03 | 按住鼠标拖动期间按 Esc，松开后再撤销 | Esc 被忽略，松开后整次拖动可撤销 | 只有 APP-KEY 纯策略证据，未在真实焦点/重复按键下执行 | Not tested |
| MAC-REGION-MANUAL-24-04 | 移动/调整后 Enter 完成，读取剪贴板 PNG | 输出等于新选区的裁剪，尺寸/像素正确 | 需要真实指针手势与剪贴板读取；本票未执行 | Not tested |
| MAC-ANNOTATION-MANUAL-24-01 | 标注工具落地后验证对象随选区移动、调整不缩放、越界恢复 | 与 I-05/I-06 一致 | 本票按范围不实现绘制工具；模型合同由 CORE-EDIT-02/05/08 覆盖 | Not tested |
| PERF-INTERACT-24-01 | Release、30 个拖动/撤销样本 | p95 ≤ 100 ms | 未执行性能测量 | Not tested |

原生 E2E 的 MAC-E2E-12 只证明覆盖层根分组与取消按钮仍可寻址，不证明选区手势；因此不把
它列为本票选区行为的通过证据。

控制点观感、贴边布局与标注排版属于 `TEST.md` §4.4 的用户视觉判断，本报告不作通过结论。
