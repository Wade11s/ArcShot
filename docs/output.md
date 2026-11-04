# #22 Region Capture：框选、Finish 与 PNG 输出

本切片在冻结内屏上建立唯一选区，并以 Enter、选择态空白双击或右上“完成”复制
PNG，或以右上“保存”显式写入 PNG 文件。它落地统一文档、2x 坐标转换、共享
合成/编码入口、固定快照、版本栅栏与尺寸/内存预算。标注工具、选区移动/调整/
重选与滚动截图仍属后续票据，行为保持 Not tested。

自动化与常驻原生冒烟的稳定入口见 `TEST.md`；实机证据见
`docs/validation/22-region-output.md` 与 `docs/validation/23-save-png.md`。

## 文档与坐标

- core 的 `Document` 持有不可变 `CapturedFrame`、唯一 `Option<Selection>` 与
  `version`。选区变化才递增版本；相同矩形不是变化；无效矩形不替换旧选区。
- `Selection` 使用 display-local 逻辑点。`Selection::pixel_rect` 直接从逻辑坐标
  乘以 2x scale 并对每条边取整一次；不接受来自视图缩放或位置的值，避免重复
  缩放/取整。结果要求非空、位于冻结屏内、任一边 ≤16,384 且 ≤24,000,000 物理
  像素。
- `Document::crop` 是唯一裁剪入口；`ComposedImage` 只接受与尺寸一致的 RGBA
  字节。2x 选区 `(10,5)-(40,25)` → 物理 `(20,10) 60×40`。

## 选区交互

- 拖动建立选区，起点/终点反向也可；任一边为零的拖动不建立选区，单击不改变
  已有选区。指针限制在冻结屏内，单次选区不跨屏。
- 无有效选区时 Enter、双击与“完成”均不输出。选择态下双击只在其点落在选区内
  时完成；双击对象/控制点/工具栏（后续功能）不经过该路径。
- “完成”与“保存”入口位于右上；Esc 取消会话，不复制、不保存。输出处理期间
  视图禁用拖动、重复完成与保存，core 同时忽略 Finish/Save/Start，迟到结果必须
  通过版本栅栏。
- “保存”只在存在有效选区且输出互斥空闲时可用；保存对话框打开期间与输出处理
  期间，编辑、重复完成、重复保存与截图快捷键都被忽略。

## 输出管线

1. 冻结成功后 core 保留 `Document`；`Finish` 需要当前 ticket、有效选区、
   `output_mutex` 空闲。
2. core 只产生 `OutputJob { ticket, version, crop }`，app 在启动 worker 前克隆
   `CapturedFrame`（共享字节，不复制底图）。同一文档最多一个在途合成/编码。
3. `arcshot-image::compose_crop` 按物理矩形复制所选像素、去掉行 padding、BGRA
   → RGBA；`encode_png` 输出带 sRGB chunk 的 8-bit RGBA PNG，字节上限 128 MiB。
4. worker 返回后，app 在写剪贴板前调用 `CaptureSession::may_publish(job)`，要求
   当前 ticket、文档版本与 crop 均未变化。栅栏失败时字节被丢弃，不产生副作用。
5. 通过栅栏后写入系统剪贴板：`clearContents` → `setData(PNGf)` 必须返回 true，
   且 PNG 类型必须可读回；只复制，不写盘。失败保留选区与文档供重试。
6. 只有剪贴板确认成功才结束会话并播放一次简短 `Tink` 音效；不显示成功文字或
   通知。失败显示分类错误并保留文档，可重新 Finish。
7. 取消会让在途任务的结果栅栏失效，因此不会在取消后再复制。

## 显式保存（Save PNG）

1. 右上“保存”与“完成”走同一个 core 入口 `begin_output`：先执行输出前提交钩子
   `Document::commit_edits`（本切片无文字工具，钩子已就位，文字/IME 提交扩展此处
   而不是另建输出路线），再固定 ticket、文档 `version` 与物理 `crop` 快照。
2. `OutputJob` 携带 `OutputTarget`：`Clipboard`（完成）或 `SaveFile`（保存）。
   core 的输出状态机只有一个槽位：`Panel(job)`（保存对话框打开，尚无编码字节）
   或 `Encoding(job)`（固定快照合成/编码中）。`Finish`、`Save`、`SelectRegion`
   在槽位占用时都不产生效果，`Start`（菜单与全局快捷键共用的入口）同样被忽略。
3. 保存路径先请求原生 `NSSavePanel`（`CaptureEffect::RequestSavePanel`）；面板由
   `crates/macos` 的安全适配器以 `beginWithCompletionHandler` 非阻塞显示，完成
   回调只写入一个主线程结果槽，由 app 的下一次 tick 取走，因此没有嵌套模态
   run loop 会重入 GPUI。面板层级设为 102，高于 GPUI 冻结覆盖层的
   `NSPopUpWindowLevel`（101）。会话结束或取消时 `cancel_save_panel` 关闭仍打开
   的面板，其迟到结果由 job 栅栏丢弃。
4. 面板结果：取消 → 释放互斥、无错误、文档与版本不变；失败 → 释放互斥并报告
   `OutputFailure::SavePanel`，保留文档；选定路径 → 同一快照进入合成/编码，路径
   仅保存在 app（core 不持有路径）。
5. 编码完成、`may_publish(job)` 通过后，`arcshot_app::publish_output` 只执行一个
   副作用：`Clipboard` 写系统剪贴板，`SaveFile` 写用户选择的文件。没有匹配路径
   的保存 job 报告 `OutputFailure::SaveFile`，不会写文件也不会回退到剪贴板。
6. 保存成功才结束会话并播放完成音效（音效偏好在后续票据落地前沿用 #22 的无条件
   播放）；保存成功不复制剪贴板，完成也不写文件。保存/复制失败显示分类错误并
   保留选区、对象与版本，可再次保存重试；Esc 放弃不产生输出。

文件写入由 `crates/macos::write_png_to_file` 完成：先校验 PNG 签名与 128 MiB
预算再创建 `NSData`（预分配前检查），`writeToURL:atomically:` 返回 true 后读回
文件长度必须与编码字节一致；不一致时删除刚写入的文件并报告失败，不把发起请求
当成功。

## 预算与失败分级

| 项目 | 规则 |
| --- | --- |
| 文档 | ≤24,000,000 物理像素，任一边 ≤16,384；冻结帧接纳时已检查 |
| 裁剪/合成 | 分配前 checked 尺寸、像素与 `try_reserve_exact`；失败不生成损坏输出 |
| PNG | 编码结果 ≤128 MiB；超限保留文档，不写剪贴板/磁盘 |
| 文件写入 | 签名与 128 MiB 预算在 `NSData` 分配前检查；原子写入后校验字节数，不一致则删除 |
| 任务 | 同一文档最多 1 个合成/编码任务；`OutputTasks` 单槽 |
| 栅栏 | 接纳结果和副作用前各检查 ticket/version/crop/target |

`OutputFailure` 区分 ResourceLimit、Encode、Clipboard、SavePanel、SaveFile 与
Worker。编码 worker
panic 只让本次输出失败并保留文档；它不表示原生捕获状态不确定，因此不要求
重启应用。原生捕获的 RelaunchRequired 规则不变。

## 常驻冒烟

`--smoke-test` 在真实应用进程中用 `arcshot_image` 编码 2×2 PNG，写入私有命名
pasteboard，校验 PNGf 字节回读一致，并用 `NSBitmapImageRep` 在内存中解码尺寸；
`check-startup.sh` 断言该结果行。同一 `--smoke-test` 还用编码出的 PNG 写临时
文件并逐字节读回、拒绝不存在的目标路径与非 PNG 字节、配置真实 `NSSavePanel`
（不显示），输出 `save_file_smoke=pass`。私有 pasteboard 不触碰用户剪贴板，也
不保存取证图像。剪贴板、音效与保存适配器在非主线程返回明确错误/不播放，由原生
测试覆盖；面板的显示、取消与覆盖写属于需人工确认的 MAC-RECOVERY 用例。

## 需用户参与的稳定子用例

使用用户授权的正式 bundle；优先 Release。除用户显式保存测试文件外，不保存
取证截图；日志只记录几何、尺寸、hash 与结果。

| ID | 前置 | 步骤 | 必须观察的预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- | --- |
| MAC-REGION-MANUAL-22-01 | 已授权；仅内屏；受控静态内容 | 开始截图 → 拖出一个已知矩形 → Enter | 完成音效；剪贴板粘贴尺寸 = 逻辑尺寸×2；内容与所选区域一致（无偏移/翻转/通道交换）；会话结束；没有文件写入 | 未执行 | Not tested |
| MAC-REGION-MANUAL-22-02 | 已冻结 | 单击不拖动；再拖出有效选区；空白双击选区内；双击选区外 | 单击不建立选区；双击选区内完成并复制；选区外双击不完成 | 未执行 | Not tested |
| MAC-REGION-MANUAL-22-03 | 已冻结且有有效选区 | 点右上“完成”；再测 Esc；按住拖动时按 Esc/Enter | “完成”与 Enter 输出一致；Esc 取消且无复制；拖动中 Esc/Enter 被忽略；输出处理中拖动/重复完成/截图快捷键被忽略 | 未执行 | Not tested |
| MAC-COLOR-MANUAL-22-01 | 最新一次 Finish 已成功 | 在非 ArcShot 应用（如 TextEdit）粘贴；另用系统 PNG 解码/预览核对 | 常规粘贴成功；尺寸/像素与选区一致；PNG 类型报告为 PNGf；sRGB 颜色无通道/方向错误 | 未执行 | Not tested |
| MAC-RECOVERY-MANUAL-22-01 | 可注入剪贴板失败或超预算选区 | Finish 失败；再次 Finish；Esc | 明确错误且选区/文档保留；重试可再次输出；Esc 不复制 | 未执行 | Not tested |
| MAC-COLOR-MANUAL-22-02 | 已授权；固定四象限夹具 | 框选四象限并 Finish；内存解码核对角点 | 内存解码的尺寸与四角 RRGGBB 与夹具一致；不依赖截图落盘 | 未执行 | Not tested |

每行须填写实际及 Pass/Fail/Not tested，附实测 SHA、配置、UTC、工具链、
macOS build、显示几何/scale、权限与夹具版本。
