# #25 Rectangle 标注的绘制、对象编辑与输出验证记录

本记录只覆盖 #25 落地的自动化证据：Rectangle 对象语义、固定像素合成、app 键位/指针
路由，以及冻结覆盖层工具坞与悬停选项的原生 AX 激活。真实指针绘制矩形、对象八控制点
拖拽/翻转、选中后删除键与预览观感仍属 MAC-ANNOTATION，保持 **Not tested**：对象与
控制点没有 AX 节点，`TEST.md` §4.3 禁止用 `--headed` 坐标拖拽补足验收。测试过程未
捕获用户屏幕、未保存取证图像、未读写用户剪贴板。

## 实测来源与环境

- 基线 SHA：`8749793`（`spec/v1`，#24 合并点）。
- 实测实现/脚本 SHA：`f60b5d8f2559668f4c26c6ddf0f7dde94f41de93`。
- 时间：2026-11-04T05:40:02Z（record-validation 采集）。
- 构建配置：Debug 与 Release。完整入口构建并冒烟两者，Rust 自动化运行在 Debug 测试配置。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (Build version 27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建屏 display_id=1，origin=(0,0)，1470×956 逻辑点，scale=2（2940×1912 物理像素）。
- 权限：Screen Recording preflight=true；`agent-desktop status` 的 accessibility 与
  screen_recording 均为 granted；未请求或改动通知、输入监控。权限值不代表完整原生矩阵。
- 夹具：合成夹具 v1——40×20 逻辑点（2x，80×40 物理像素）统一不透明 BGRA 底图，RGBA
  为 (30,20,10,255)；100×50 逻辑点（2x）全 255 底图；不含用户截图。像素断言使用固定
  literal 真值，不读取产品私有实现。
- 日志：`target/25-check-all.log`、`target/25-e2e-debug.log`、
  `target/25-product-debug.log`、`target/25-validation-debug.txt`、
  `target/25-manual-pointer.log`。日志是本地构建产物，不入库；本报告保存摘要。
- `worktree_dirty=false`（record-validation 在提交 `f60b5d8` 上采集）。

## 执行

```sh
cargo test --locked -p arcshot-core --test objects      # 12 passed
cargo test --locked -p arcshot-core --test session_edit # 7 passed
cargo test --locked -p arcshot-image --test annotations # 7 passed
cargo test --locked -p arcshot-app --test key_policy     # 12 passed
scripts/check-all.sh > target/25-check-all.log 2>&1      # exit 0
scripts/e2e-agent-desktop.py > target/25-e2e-debug.log 2>&1  # exit 0, cases=14
scripts/record-validation.sh --configuration debug \
  --logs target/25-check-all.log,target/25-e2e-debug.log,target/25-product-debug.log \
  > target/25-validation-debug.txt
```

完整入口 exit 0：fmt、Clippy warnings-as-errors、doc、119 个测试、验证工具/依赖/架构/
真实文字后端、Debug/Release 原生驻留/退出、无网络与 PNGf 剪贴板冒烟全部通过。关键输出：

```text
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
privacy_smoke=pass resident_pid=53272 network_sockets=0 lsof_status=1
validation_reports=pass directory=docs/validation
dependency_inspection=pass
```

Debug 与 Release bundle 的 `bundle_identity_smoke`、`capture_window_exclusion_smoke`、
`capture_menu_smoke`、`hotkey_registration_smoke`、`clipboard_pngf_smoke` 均为 pass。

## 原生 E2E（agent-desktop）与 AX 合同

在 `f60b5d8` 的同一字节树上用 Debug bundle 执行 `scripts/e2e-agent-desktop.py`。
`agent-desktop 1.1.0`（envelope 2.6），`status` 原文：

```text
permissions={"accessibility": {"state": "granted"}, "automation": {"state": "unknown"}, "screen_recording": {"state": "granted"}}
```

14 个稳定用例全部通过（`mac_e2e=pass configuration=debug ... cases=14`），其中本票新增：

- MAC-E2E-13：`snapshot --app "ArcShot Debug"` 后，`group "标注工具"` 下有
  `button 选择 / arcshot.overlay.tool.select` 与 `button 矩形 / arcshot.overlay.tool.rectangle`；
  Select 的 checked/toggled 为 true、Rectangle 为 false；无选中对象时没有删除按钮。
  AX 激活 Rectangle 后日志出现
  `a11y_action=arcshot.overlay.tool.rectangle action=Click` 与
  `capture_edit action=SelectTool version=0 annotations=0 selection=None tool=Rectangle selected=None style=stroke=#FF3B30 width=3 fill=false`，
  checked 状态互换。
- MAC-E2E-14：`hover @<rectangle> --headed` 只作为浮现前置；AX 激活
  `arcshot.overlay.option.rectangle.width.5` 与 `.color.blue` 后日志分别为
  `capture_edit action=RectangleOption version=0 annotations=0 selection=None tool=Rectangle selected=None style=stroke=#FF3B30 width=5 fill=false`
  和 `... style=stroke=#007AFF width=5 fill=false`；版本保持 0，width.5 与 color.blue 的
  checked 变为 true、color.red 为 false。
- MAC-E2E-12 仍通过：覆盖层根分组与 `arcshot.overlay.cancel` 可寻址，AX 激活后回到
  `capture_phase from=Frozen to=Resident action=CancelAttempt`。

底层命令序列（每条引用只在其快照内使用）：

```sh
agent-desktop find --surface menu-bar-extras --app "ArcShot Debug" --role menuitem --name "开始截图"
agent-desktop click @<menu-ref>
agent-desktop snapshot --app "ArcShot Debug" --compact
agent-desktop click @<rectangle-tool-ref>
agent-desktop hover @<rectangle-tool-ref> --headed
agent-desktop click @<width-5-ref>
agent-desktop click @<color-blue-ref>
agent-desktop click @<cancel-ref>
```

`hover --headed` 是物理指针移动，属于浮现前置，不作为验收动作；验收证据是 AX 激活
（`a11y_action=` 行）与产品 `capture_edit` 诊断行。选中态经 `aria_toggled` 映射为 macOS
AX value/checked（`aria_selected` 单独在 button 上不会被 AccessKit macOS 暴露，因此两者
同时设置）。E2E 未保存截图，也未触发 Finish/剪贴板。

首次 MAC-E2E-14 尝试失败：脚本用点击前的旧快照根节点查找悬停选项，报
`hovered rectangle options are not addressable`。这是测试脚本缺陷，不是产品失败；修正为
重新读取快照后重跑 14/14 通过，产品代码未改。

## 稳定自动化子用例

前置为上述 SHA、锁定依赖和合成夹具；不需要屏幕权限，也不触发原生捕获。断言都从
`Document`/`CaptureSession`/`compose_document` 的公开状态与像素观察，不读私有字段。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-EDIT-13 | 选区 (10,5,30,20)，Rectangle 工具；选区外按下，再在选区内两次拖动 | 外部按下不创建；每次手势创建一个对象、自动选中、工具保持 Rectangle；层叠按创建顺序；版本各 +1 | 完全一致 | Pass |
| CORE-EDIT-14 | 零宽拖动、拖出后回到零高、原地点按 | 均不创建、不推进版本、不入历史 | 完全一致 | Pass |
| CORE-EDIT-15 | 从选区内 (12,8) 拖到 (55,45) | 起点必须在选区内；对象越界数据保留，`visible_bounds` 裁剪为 12,8,28×17 | 完全一致 | Pass |
| CORE-EDIT-16 | 两个重叠对象；选择工具按在重叠处 | `annotation_at` 返回最上层；对象拖动优先于选区拖动；选区几何不变 | 完全一致 | Pass |
| CORE-EDIT-17 | 八控制点各自拖动；再从左上拖过右下 | 每个 handle 只改自己的边；越边后归一化为 30,15,5×5（双轴翻转）；一次版本；undo 恢复 | 八向一致，翻转与撤销一致 | Pass |
| CORE-EDIT-18 | 选中对象后按空白内部并拖动 +2/+1 | 点击清空对象选择；选区与全部对象按实际位移移动，一次历史步；undo 恢复 | 完全一致 | Pass |
| CORE-EDIT-19 | 两个对象选中下层后删除 | 只删选中项、其余层序不变、版本 +1；undo 恢复两项与原层序；redo 再删；无选中时删除为 no-op | 完全一致 | Pass |
| CORE-EDIT-20 | 选中对象改线宽/颜色/填充；无选中时改样式；重复同值 | 有选中：对象与工具样式同步、一步历史，undo 恢复对象样式而工具样式保留；无选中：只改工具样式、不动对象、版本不变；同值不入历史 | 完全一致 | Pass |
| CORE-EDIT-21 | 工具改绿/2 点/填充后创建；新建 Document | 新对象使用当前工具样式；新会话恢复红/3/无填充、默认 Select | 完全一致 | Pass |
| CORE-EDIT-22 | 对象拖动原地释放，再真实拖动，undo/redo | 原地不入历史；真实拖动一步；undo/redo 恢复/重放 | 完全一致 | Pass |
| CORE-EDIT-23 | 把对象拖到选区外 | 数据保留为 42,28,5×5，`visible_bounds` 为 None；undo 恢复原位置 | 完全一致 | Pass |
| CORE-EDIT-24 | 选中对象后 deselect、切工具两次 | 不推进版本、不入历史；undo 仍回到对象创建前 | 完全一致 | Pass |
| CORE-SESSION-24 | Frozen+选区；SelectTool、PointerPressed/Moved/Released、RectangleOption、DeleteSelected、Undo/Redo；Finish 在途时重复这些动作 | 手势实时预览、松开一步；样式/删除/撤销共享文档；输出在途时工具/样式/删除/取消选择全部忽略，版本不动 | 完全一致 | Pass |
| IMAGE-COMPOSE-05 | 选区全屏；矩形 (10,5,10,5)，默认红/3 点/无填充 | 2x 路径 (20,10)-(40,20)，6 像素描边中心线；内部保持底图；四边带像素逐点吻合 | 与 literal 真值完全一致 | Pass |
| IMAGE-COMPOSE-06 | 同上加 20% 同色填充 | 内部 (75,28,18,255)；描边保持不透明红 | 完全一致 | Pass |
| IMAGE-COMPOSE-07 | 七色板 × 2/3/5 点线宽 | 每色 stroke 像素等于固定 sRGB；4/6/10 物理像素带宽、中心位置正确 | 全部一致 | Pass |
| IMAGE-COMPOSE-08 | 选区 (5,2,10,5)，矩形越界 (6,1,8,6) | 只输出 crop (10,4,20,10)；上下左右描边在选区边缘被裁剪，内部不填充 | 完全一致 | Pass |
| IMAGE-COMPOSE-09 | 同 bounds 先红填充后蓝填充/描边 | 后建对象覆盖先建；半透明填充按叠放顺序再合成 | 完全一致（[60,47,65,255]） | Pass |
| IMAGE-COMPOSE-10 | 同一文档分别用选区 crop 与子 crop 合成 | 重叠区域逐像素一致，裁剪只影响可见范围 | 完全一致 | Pass |
| IMAGE-COMPOSE-11 | 合成 RGBA 与 `preview_bgra` | BGRA 交换 R/B、保留 alpha，长度不变 | 完全一致 | Pass |
| APP-KEY-09 | Esc 在选中对象/工具/会话三层，拖动中与输出中 | 依次 Deselect → SelectTool → Cancel；拖动中 None；输出中 Cancel | 完全一致 | Pass |
| APP-KEY-10 | Delete/Backspace 无对象、有对象、输出期、拖动中、带修饰键 | 仅“有对象且空闲”映射 DeleteObject；其余 None | 完全一致 | Pass |
| APP-KEY-11 | Rectangle 工具下选区内/外按下、双击、Enter | 区内 Create（双击也不 Finish）；区外 Band；Enter 仍 Finish | 完全一致 | Pass |
| APP-KEY-12 | 选择工具下对象命中/控制点/空白/双击 | 对象命中一律 ObjectDrag（优先于选区控制点与双击）；否则选区 Drag/FinishAt；输出期 None | 完全一致 | Pass |
| MAC-E2E-13 | 开始截图后 AX snapshot 与激活工具坞 | Select/Rectangle 可寻址，checked 反映当前工具；AX 激活 Rectangle 后日志与 checked 互换，无对象时无删除按钮 | 与预期一致，14/14 运行中通过 | Pass |
| MAC-E2E-14 | 悬停 Rectangle 后 AX 激活 5 点线宽与蓝色 | 选项条可寻址；日志 `style=stroke=#007AFF width=5`、版本不变；checked 状态更新 | 与预期一致 | Pass |

## 设计与合同说明

- `Document` 仍是唯一权威：`tool`、每会话 `rectangle_style`、`selected`、对象列表与
  历史都在文档内。工具/选择/样式记忆是可交互状态，不推进版本、不入历史；对象创建、
  删除、移动、调整、样式修改与撤销/重做各推进版本一次。历史只保存选区+对象列表，
  不保证恢复选中态，恢复时清空选中索引。
- 对象坐标是屏幕内逻辑点；预览与输出都调用同一个 `compose_document(frame, crop,
  annotations)`，先按唯一 2x 换算裁剪，再按创建顺序画填充与描边。描边以路径为中心，
  宽度 2/3/5 逻辑点映射为 4/6/10 物理像素；crop 之外自然裁剪，不丢弃越界数据。
- 覆盖层只画交互 chrome（选区、控制点、尺寸标签）；标注像素来自 image 合成结果。
  coordinator 在后台执行器上合成选区预览，最多一个在途请求，完成时用当前文档的
  crop+annotations 校验，过期结果丢弃；替换/关闭窗口时 `cx.drop_image` 释放旧纹理。
- 输出任务在提交时快照 `crop + annotations`，worker 不读取可变文档；发布前仍用
  ticket/version/crop 栅栏。预览与最终 PNG 因此共用同一栅格化语义。
- 左侧工具坞只有 Select 与 Rectangle；悬停行包含工具与其右侧选项，悬停不切换工具，
  移入选项区保持展开，离开收起。选项与工具有 tooltip、`aria_label`、`accessibility_id`、
  `aria_selected` 与 `aria_toggled`（macOS AX 暴露为 checked/value）。
- 新增纯文本诊断 `capture_edit action=… version=… annotations=… selection=… tool=…
  selected=… style=stroke=#RRGGBB width=N fill=…`，在按下/松开、工具、样式、删除、
  取消选择、重选与撤销/重做后打印；不打印像素。E2E 以它作为状态真值。

## 限制与未测试

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-ANNOTATION-MANUAL-25-01 | 授权后用真实指针在选区内拖出多个矩形，再拖动/调整/翻转控制点 | 连续创建、自动选中、越边翻转、层叠与裁剪符合 I-06 | 对象与控制点无 AX 节点，`--headed` 坐标拖拽按 §4.3 不能作为验收证据；未计入通过 | Not tested |
| MAC-ANNOTATION-MANUAL-25-02 | 选中对象后按 Delete/Backspace，再 `Cmd+Z`/`Cmd+Shift+Z` | 删除一步可撤销；重做再删除 | 只有 APP-KEY 与 CORE-EDIT 纯语义证据；探索性真实按键见下，未按验收记录 | Not tested |
| MAC-ANNOTATION-MANUAL-25-03 | 已有选中对象时改样式选项，再用 Esc 分层 | 只改选中对象与工具样式；Esc 先取消选择、再退工具、再取消会话 | 只有 APP-KEY/CORE-EDIT 与探索性记录；未按验收记录 | Not tested |
| MAC-ANNOTATION-MANUAL-25-04 | 移动到边缘的对象、极小选区、贴边工具坞与悬停选项视觉 | 裁剪、控制点、选项条不遮挡关键内容，观感可接受 | 视觉判断属于 §4.4 用户项目；未执行 | Not tested |
| MAC-COLOR-OUTPUT-25-01 | Enter 完成带多个彩色矩形的选区并读回剪贴板 PNG | 输出像素与 `compose_document` 一致、sRGB 正确 | 未读取用户剪贴板；共享路径由 IMAGE-COMPOSE 覆盖，原生输出仍属 MAC-COLOR-OUTPUT | Not tested |
| PERF-INTERACT-25-01 | Release、30 个创建/拖动/样式/撤销样本 | p95 ≤ 100 ms | 未执行性能测量 | Not tested |
| RESOURCE-25-01 | 大步长拖动与全屏选区预览合成 | peak footprint ≤ 2 GiB，预览只有一个在途 | 只做了代码级单在途约束，未采样 | Not tested |

探索性原生观察（明确不作为通过证据）：两次非验收的真实指针会话在同一 Debug bundle 上
记录了选区 `(300,217,600,400)`、Rectangle 拖出后
`capture_edit action=PointerReleased version=2 annotations=1 ... selected=Some(0) style=stroke=#FF3B30 width=3 fill=false`，
对象拖动 `version=3`，选中对象改蓝色 `version=7`，`press backspace --headed` 得到
`action=DeleteSelected version=8 annotations=0`，`press cmd+z --headed` 得到
`action=Undo version=9 annotations=1`，两次 Esc 依次得到 `action=SelectTool` 与
`capture_phase from=Frozen to=Resident action=CancelAttempt`。原始行见
`target/25-manual-pointer.log`。这些是坐标拖拽/真实按键，`TEST.md` §4.3 不允许当作验收
通过，因此仍列 Not tested。

同一探索发现一个本票范围外的区域覆盖层现象：物理屏幕坐标与覆盖层窗口局部坐标存在约
33 点的纵向偏移（菜单栏高度），选区的逻辑坐标因此比屏幕落点高约 33 点。冻结画面与
crop 仍彼此一致（用户看到的就是被裁剪的内容），但覆盖层窗口定位/全屏贴边值得由区域
或原生窗口票单独核查；本票未修改窗口定位。

对象/控制点观感、工具坞图标与选项条排版属于 `TEST.md` §4.4 的用户视觉判断，本报告
不作通过结论。
