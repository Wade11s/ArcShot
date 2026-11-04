# #23 显式保存 PNG、取消与失败恢复验证记录

本记录覆盖 #23 落地的自动化与常驻原生冒烟：显式 Save PNG、保存对话框互斥、
取消非错误、分类失败保留文档、同一快照的 PNG 字节复用、PNG 预算与真实文件
写入回读。真实 `NSSavePanel` 的显示、取消与覆盖写需要有效选区（选区尚无 AX
节点），仍为 **Not tested**。测试过程未捕获用户屏幕、未保存取证图像、未修改
用户剪贴板；唯一写盘是 `--smoke-test` 在 `$TMPDIR` 写入并删除的 87 字节合成
PNG。

## 实测来源与环境

- 基线 SHA：`65343e5`（`spec/v1` = #22 区域捕获切片）。
- 实测实现/脚本 SHA：`fca0cbbe9e6e37e113e13fe629597bc4a0e89d5c`。
- 时间：2026-11-04T04:30:46Z / 2026-11-04T04:30:47Z（Debug/Release record-validation，同一提交）；
  完整入口 `scripts/check-all.sh` 在同一提交上 exit 0。
- 构建配置：Debug 与 Release。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (Build version 27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：preflight ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2；
  未在本轮捕获真实屏幕。
- 权限：证据采集时 Debug bundle 的 Screen Recording preflight=true，
  guide_seen=true；未请求或改动权限，未检查/请求 Input Monitoring、Accessibility
  或通知。显式保存的文件冒烟不依赖屏幕录制权限。
- 夹具：2x 合成不透明 BGRA frame（1×1 与 100×50 逻辑、200×100 物理含 stride）、
  2×2 全 255 RGBA 的 87 字节 PNG、`$TMPDIR` 临时文件、私有命名 pasteboard。
  夹具 v1，不含用户截图。
- 日志：`target/23-final-check-all.log`、`target/startup-smoke-debug.log`、
  `target/startup-smoke-release.log`、`target/23-validation-debug.txt`、
  `target/23-validation-release.txt`。日志是本地构建产物，不入库；本报告保存摘要。
- `worktree_dirty=false`；报告本身是之后的纯证据文档提交。

## 执行

```sh
scripts/check-all.sh > target/23-final-check-all.log 2>&1
target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot --check-capture-environment
scripts/record-validation.sh --configuration debug \
  --logs target/23-final-check-all.log,target/startup-smoke-debug.log
scripts/record-validation.sh --configuration release \
  --logs target/23-final-check-all.log,target/startup-smoke-release.log
cargo test --locked -p arcshot-core --test save
cargo test --locked -p arcshot-app --test output_publish
cargo test --locked -p arcshot-macos --test save_native
```

完整入口 exit 0：fmt、Clippy warnings-as-errors、doc、83 个 Rust 测试、
验证工具/依赖/架构/真实文字后端、Debug/Release 原生驻留/退出、无网络、剪贴板
PNGf 与显式保存文件冒烟全部通过。`--check-capture-environment` 实测输出：

```text
capture_environment display_id=1 origin=0,0 logical=1470x956 scale=2 screen_recording=true guide_seen=true
```

`check-startup.sh` 在两种配置下断言的新增结果行：

```text
save_file_smoke=pass bytes=87 roundtrip=true rejected_destination=true invalid_bytes=true panel_configured=true
```

## 稳定自动化子用例

前置为上述 SHA、锁定依赖与合成夹具；不需要屏幕权限，也不显示真实保存面板。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-SESSION-06 | 冻结并建立选区 → Save 打开面板 → 菜单/热键 Start | 面板期间忽略 Start；取消后面板释放，Start 正常重启 | 无 Start 效果，相位保持 Frozen；取消后 DismissOverlay+CheckPermission | Pass |
| CORE-SESSION-20 | 保存面板打开（2x 选区 (10,5)-(40,25)） | 单一 SaveFile job（version 1、crop (20,10) 60×40）；Finish/Save/FinishAt/SelectRegion/Start 全部无效，面板期不可发布 | 全部返回空效果，选区/版本不变，`may_publish=false` | Pass |
| CORE-SESSION-21 | 保存面板取消 → 重复取消 → 再次 Save | 取消非错误且无效果；文档/版本/选区不变；重试得到同一快照 job | 空效果、Frozen、version 1；重试 job 相等；重复取消被拒绝 | Pass |
| CORE-SESSION-22 | 保存面板失败 | 报告 SavePanel 分类错误，释放互斥，保留文档，可同版本重试 | `ReportOutputFailure(SavePanel)`，版本与 crop 保持 | Pass |
| CORE-SESSION-23 | 无选区 Save、取消会话后旧 ticket Save | 均不打开面板、不占用互斥 | 空效果，`output_busy=false`，文档已释放 | Pass |
| APP-EFFECT-18 | 面板选定 → 合成完成 → 写文件成功 | SaveFile job 合成；仅确认写入后才结束会话并播放完成音效 | `ComposeOutput(job)`；`DismissOverlay + PlayCompletionSound`，转 Resident | Pass |
| APP-EFFECT-19 | 写文件失败 / 超预算 / 编码失败 | 分类错误且保留文档、可同版本重试 | 三类均 `ReportOutputFailure`，Frozen，重试 job 版本/crop 相同 | Pass |
| APP-EFFECT-20 | 取消后迟到面板结果与写入；剪贴板 job 的面板结果；同一面板重复结果 | 一律不合成、不发布、不结束会话 | 全部空效果；`may_publish=false`；第二次面板结果被拒 | Pass |
| APP-EFFECT-21 | 复制失败后显式保存 | 同一 ticket/version/crop 快照，target 不同；两个动作互不触发 | job 相等除 target；剪贴板 job 不可被面板解析；保存期忽略 Finish | Pass |
| APP-EFFECT-22 | 保存 job + 已记住路径 | 只调用文件副作用，路径单次使用；重复发布无路径则 SaveFile 失败 | 剪贴板调用 0 次，文件收到同字节；第二次 Err(SaveFile) | Pass |
| APP-EFFECT-23 | 记住路径后文档版本变化 | 旧路径不写入，失败且不再挂起 | 无副作用，Err(SaveFile)，`is_pending=false` | Pass |
| APP-EFFECT-24 | 剪贴板 job + 无关保存路径 | 只写剪贴板，不写文件、不消费保存路径 | 剪贴板收到字节，文件副作用未调用，路径仍挂起 | Pass |
| APP-EFFECT-25 | 同一文档版本分别走 Finish 与 Save 路由 | PNG 字节完全一致，尺寸为 60×40 物理像素 | 字节相等；PNG 签名与 IHDR 60×40；重复编码一致 | Pass |
| APP-EFFECT-26 | 128 MiB + 1 字节的编码结果 | 副作用前拒绝并保留文档 | Err(ResourceLimit)，无剪贴板/文件副作用，路径已清 | Pass |
| MAC-RECOVERY-23-01 | worker 线程创建面板、写文件、取结果、取消面板 | AppKit 副作用前明确拒绝 | NotMainThread / NotMainThread / None / 无操作，无文件产生 | Pass |
| MAC-RECOVERY-23-02 | Debug/Release `--smoke-test` | 真实进程写 87 字节 PNG 并逐字节读回、拒绝不存在目标与非 PNG 字节、配置真实 `NSSavePanel` | 两种配置均 `save_file_smoke=pass bytes=87 … panel_configured=true` | Pass |
| #22 回归 | CORE-SESSION-08～13、APP-EFFECT-12～17、APP-KEY、IMAGE-* | 原有复制/音效/栅栏/预算行为不变 | 83 个测试全部通过（CORE-SESSION-06 改由真实保存流程驱动） | Pass |
| PRIVACY-01、ARCH-01、TEXT-01、EVIDENCE-01、DEPENDENCY-01 | 完整入口 | 无网络/监听、四 crate 方向、真实文字后端、报告格式、依赖来源 | 全部通过；新增直接依赖 `block2` 与 `objc2-uniform-type-identifiers` 已在锁定图中且来源为 crates.io | Pass |

## 说明与限制

- 自动化只配置 `NSSavePanel`（文件名、PNG UTType、父子层级），从不显示面板；
  `panel_configured=true` 不等于面板已在真实冻结覆盖层之上可见。
- 保存面板层级设为 102，理由是其必须高于 GPUI `PopUp` 覆盖层的
  `NSPopUpWindowLevel`（101）；真实叠放顺序与可操作性需实机确认。
- PNG 预算由共享编码器的预算 writer 与 `arcshot_app::publish_output` 双重检查；
  真实 128 MiB 编码结果不可由 24M 像素文档构造，故 APP-EFFECT-26 用注入字节
  长度覆盖发布前拒绝，原生适配器另有签名/预算/读回校验。
- 文件写入是原子写入后校验字节数；读回不一致会删除刚写入的文件并报失败。
  只读卷、磁盘满与覆盖确认属于 MAC-RECOVERY 人工项。
- 会话在“面板已完成、app 尚未取走结果”的同一 tick 内结束时，适配器不会再次
  cancel 已关闭的面板，并且会丢弃该结果；job 栅栏同时禁止发布。这个时序由代码
  路径保证，本轮未构造原生竞态注入。
- 完成音效沿用 #22 现有行为（无条件播放）；音效偏好开关属后续票据，本轮未实现
  也未验证关闭后无声音。

## 仍为实机/用户参与的原生子用例

以下项目需要真实拖拽建立选区、真实面板交互或音效偏好，本轮未执行。所有步骤
都不能用 `--headed` 坐标点击替代验收证据。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-RECOVERY-MANUAL-23-01 | 已授权；开始截图并拖出有效选区 → 点右上“保存” | 原生 PNG 保存面板出现在冻结覆盖层之上、成为键盘焦点，默认文件名 `ArcShot.png`，类型过滤为 PNG；点取消或按 Esc | 未执行（有效选区尚无 AX 节点；坐标拖拽不作为验收证据） | Not tested |
| MAC-RECOVERY-MANUAL-23-02 | 面板中取消 | 返回原会话，选区/文档/编辑状态不变，无错误、无音效、无剪贴板变化、无文件写入 | 未执行 | Not tested |
| MAC-RECOVERY-MANUAL-23-03 | 面板中选定路径并保存 | 文件出现且可在 Preview/其他应用打开，尺寸与选区物理像素一致；剪贴板未被修改；会话结束并播放完成音效 | 未执行 | Not tested |
| MAC-RECOVERY-MANUAL-23-04 | 对已存在文件保存 | 出现系统覆盖确认；确认后原文件替换，取消则原文件与文档都保留 | 未执行 | Not tested |
| MAC-RECOVERY-MANUAL-23-05 | 选只读目录/磁盘满等不可写目标后保存 | 明确错误提示，文档与选区保留，可重试或 Esc 放弃；不声称成功、不留下损坏文件 | 未执行（原生适配器的写入失败与读回校验已自动化） | Not tested |
| MAC-RECOVERY-MANUAL-23-06 | 面板打开期间按截图快捷键、点“完成”/“保存”/拖动覆盖层 | 全部被忽略；取消面板后恢复编辑与热键 | 未执行（互斥由 CORE-SESSION-06/14 覆盖，真实按键投递未驱动） | Not tested |
| MAC-COLOR-OUTPUT-MANUAL-23-01 | 同一版本先 Finish 复制，再对同一选区 Save | 保存文件字节与剪贴板 PNG 字节一致（hash 比对），且保存不触发复制 | app 级字节一致已自动化；系统剪贴板与第三方应用打开未验证 | Not tested |

每行须填写实际及 Pass/Fail/Not tested，附实测 SHA、配置、UTC、工具链、
macOS build、显示几何/scale、权限与夹具版本。
