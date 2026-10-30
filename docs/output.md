# #22 Region Capture：框选、Finish 与 PNG 输出

本切片在冻结内屏上建立唯一选区，并以 Enter、选择态空白双击或右上“完成”复制
PNG。它落地统一文档、2x 坐标转换、共享合成/编码入口、固定快照、版本栅栏与
尺寸/内存预算。标注工具、选区移动/调整/重选、显式保存与滚动截图仍属后续票据，
行为保持 Not tested。

自动化与常驻原生冒烟的稳定入口见 `TEST.md`；实机证据见
`docs/validation/22-region-output.md`。

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
- “完成”入口位于右上；Esc 取消会话，不复制、不保存。输出处理期间视图禁用
  拖动与重复完成，core 同时忽略 Finish/Start，迟到结果必须通过版本栅栏。

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

## 预算与失败分级

| 项目 | 规则 |
| --- | --- |
| 文档 | ≤24,000,000 物理像素，任一边 ≤16,384；冻结帧接纳时已检查 |
| 裁剪/合成 | 分配前 checked 尺寸、像素与 `try_reserve_exact`；失败不生成损坏输出 |
| PNG | 编码结果 ≤128 MiB；超限保留文档，不写剪贴板/磁盘 |
| 任务 | 同一文档最多 1 个合成/编码任务；`OutputTasks` 单槽 |
| 栅栏 | 接纳结果和副作用前各检查 ticket/version/crop |

`OutputFailure` 区分 ResourceLimit、Encode、Clipboard 与 Worker。编码 worker
panic 只让本次输出失败并保留文档；它不表示原生捕获状态不确定，因此不要求
重启应用。原生捕获的 RelaunchRequired 规则不变。

## 常驻冒烟

`--smoke-test` 在真实应用进程中用 `arcshot_image` 编码 2×2 PNG，写入私有命名
pasteboard，校验 PNGf 字节回读一致，并用 `NSBitmapImageRep` 在内存中解码尺寸。
私有 pasteboard 不触碰用户剪贴板，也不保存取证图像。

## 需用户参与的稳定子用例

使用用户授权的正式 bundle；优先 Release。除用户显式保存测试文件外，不保存
取证截图；日志只记录几何、尺寸、hash 与结果。

| ID | 前置 | 步骤 | 必须观察的预期 |
| --- | --- | --- | --- |
| MAC-REGION-MANUAL-22-01 | 已授权；仅内屏；受控静态内容 | 开始截图 → 拖出一个已知矩形 → Enter | 完成音效；剪贴板粘贴尺寸 = 逻辑尺寸×2；内容与所选区域一致（无偏移/翻转/通道交换）；会话结束；没有文件写入 |
| MAC-REGION-MANUAL-22-02 | 已冻结 | 单击不拖动；再拖出有效选区；空白双击选区内；双击选区外 | 单击不建立选区；双击选区内完成并复制；选区外双击不完成 |
| MAC-REGION-MANUAL-22-03 | 已冻结且有有效选区 | 点右上“完成”；再测 Esc | “完成”与 Enter 输出一致；Esc 取消且无复制；输出处理中拖动/重复完成/截图快捷键被忽略 |
| MAC-COLOR-MANUAL-22-01 | 最新一次 Finish 已成功 | 在非 ArcShot 应用（如 TextEdit）粘贴；另用系统 PNG 解码/预览核对 | 常规粘贴成功；尺寸/像素与选区一致；PNG 类型报告为 PNGf；sRGB 颜色无通道/方向错误 |
| MAC-RECOVERY-MANUAL-22-01 | 可注入剪贴板失败或超预算选区 | Finish 失败；再次 Finish；Esc | 明确错误且选区/文档保留；重试可再次输出；Esc 不复制 |
| MAC-COLOR-MANUAL-22-02 | 已授权；固定四象限夹具 | 框选四象限并 Finish；内存解码核对角点 | 内存解码的尺寸与四角 RRGGBB 与夹具一致；不依赖截图落盘 |

每行须填写实际及 Pass/Fail/Not tested，附实测 SHA、配置、UTC、工具链、
macOS build、显示几何/scale、权限与夹具版本。
