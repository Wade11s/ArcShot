# #22 区域框选、Finish 与 PNG 输出验证记录

本记录只覆盖 #22 落地的自动化与常驻原生冒烟。#22 保持开放：真实鼠标拖拽、
系统剪贴板粘贴、音效与失败注入需要用户参与，仍为 **Not tested**；PR 不自动
合并。测试过程未捕获屏幕、未保存取证图像、未修改用户剪贴板。

## 实测来源与环境

- 基线 SHA：`0be599e4b2a6fbc6db7b3df4a0c2c2fdf0f6c7bd`（main）。
- 实测实现/脚本 SHA：`2293234d7998d4079c9506ec5598d33da5c5d7d6`。
- 时间：2026-10-30T19:19:16Z（record-validation 采集）；完整入口在同一提交上完成。
- 构建配置：Debug 与 Release。自动化与原生冒烟分别使用 Debug/Release bundle。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (Build version 27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：preflight ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2；预期物理输出 2940×1912，本次未真实捕获。
- 权限：证据采集时 Debug bundle 的 Screen Recording preflight=false，guide_seen=true；未请求或改动权限，未检查/请求 Input Monitoring、Accessibility 或通知。复制冒烟不依赖屏幕录制权限。
- 夹具：2x、200×100 物理像素的合成不透明 BGRA frame（含 stride/padding）；4×3/stride 24 的通道真值 frame；2×2 全 255 RGBA PNG；私有命名 pasteboard。夹具 v1，不含用户截图。
- 日志：`target/22-check-all.log`、`target/startup-smoke-debug.log`、
  `target/startup-smoke-release.log`、`target/resident-smoke.log`、
  `target/22-capture-environment.log`、`target/22-validation-debug.txt`、
  `target/22-validation-release.txt`。日志是本地构建产物，不入库；本报告保存摘要。
- `worktree_dirty=false`；报告本身是之后的纯证据文档提交。

## 执行

```sh
scripts/check-all.sh > target/22-check-all.log 2>&1
target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot --check-capture-environment
scripts/record-validation.sh --configuration debug \
  --logs target/22-check-all.log,target/startup-smoke-debug.log
scripts/record-validation.sh --configuration release \
  --logs target/22-check-all.log,target/startup-smoke-release.log
```

完整入口 exit 0：fmt、Clippy warnings-as-errors、doc、69 个 Rust 测试、验证
工具/依赖/架构/真实文字后端、Debug/Release 原生驻留/退出、无网络与剪贴板
PNGf 冒烟全部通过。测试只在开发中按单文件运行，最终以完整入口为准。

## 稳定自动化子用例

前置为上述 SHA、锁定依赖和合成夹具；不需要屏幕权限。剪贴板冒烟使用私有
pasteboard，不读取或覆盖用户剪贴板。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-COORD-03 | 2x 屏，拖动 (10,5)→(40,25) | 物理 (20,10) 60×40 | 完全一致 | Pass |
| CORE-COORD-04 | 反向拖动 | 归一化且不重复缩放 | 完全一致 | Pass |
| CORE-COORD-05 | 零宽/零高/NaN/负尺寸 | 不建立选区 | Empty/Geometry | Pass |
| CORE-COORD-06 | 越界与 0.2 点亚像素选区 | 拒绝，不取整成 0 宽输出 | OutOfBounds/Empty | Pass |
| CORE-COORD-07 | 整屏选区 | 保持 200×100 物理尺寸 | 完全一致 | Pass |
| CORE-DOCUMENT-01 | 选择→相同选择→无效选择 | 仅真实变化递增版本；无效不替换 | 版本 1 保持 | Pass |
| IMAGE-LIMIT-03 | 16385 边、零边、16,384×1,466 | 分配前拒绝超限 | Dimensions | Pass |
| CORE-SESSION-08 | 零尺寸拖动→Enter | 无选区、不完成、不忙 | 无效果 | Pass |
| CORE-SESSION-09 | 有效选区→Finish→重复 Finish/Start/改选区 | 单一 job；输出期全部忽略 | 单一 2x job，选区与版本不变 | Pass |
| CORE-SESSION-10 | 选区内/外双击 | 仅选区内完成 | 选区内 ComposeOutput，选区外空 | Pass |
| CORE-SESSION-11 | 无选区点击后 Enter/双击 | 永不合成 | 无效果 | Pass |
| CORE-SESSION-12 | 输出中 Esc→迟到成功结果 | 取消有效，副作用栅栏拒绝迟到结果 | may_publish=false 且无输出效果 | Pass |
| CORE-SESSION-13 | 输出中 Start | 忽略快捷键/菜单开始 | 保持 Frozen | Pass |
| APP-EFFECT-12 | 栅栏通过后剪贴板成功 | 结束会话+完成音效 | DismissOverlay+PlayCompletionSound，Resident | Pass |
| APP-EFFECT-13 | 剪贴板失败 | 保留文档并允许同版本重试 | Frozen，重试 job 版本/crop 相同 | Pass |
| APP-EFFECT-14 | 改选区后旧 job、取消后迟到 job | 不复制、不重开会话 | 空效果，Resident | Pass |
| APP-EFFECT-15 | 注入编码成功 | 单次交付 job 与字节 | 一次结果，槽位释放 | Pass |
| APP-EFFECT-16 | 编码 worker panic | 可重试的 Worker 失败 | Worker，槽位可再次提交 | Pass |
| APP-EFFECT-17 | 第二个 job 在途提交 | Busy 拒绝；完成后恢复 | 拒绝后成功后恢复 | Pass |
| IMAGE-COMPOSE-01 | stride 24、crop (1,1,2,2) | 只读所选像素，BGRA→RGBA | 像素与 padding 正确 | Pass |
| IMAGE-COMPOSE-02 | crop 越过 frame | 复制前拒绝 | Crop | Pass |
| IMAGE-COMPOSE-03 | 编码后内存解码 | 尺寸/RGBA/sRGB chunk 与输入一致 | 2×2、Rgba、sRGB 均一致 | Pass |
| IMAGE-LIMIT-04 | 合成尺寸/预算/字节长度 | 分配前按类别拒绝 | Dimensions/ResourceLimit/Length | Pass |
| APP-KEY-01 | 拖动中按 Esc/Enter | 两者都不取消/完成，拖动保持可撤销 | None/None | Pass |
| APP-KEY-02 | 输出中按 Enter/Esc | Enter 忽略；Esc 取消且副作用由栅栏拒绝 | None / Cancel | Pass |
| APP-KEY-03 | 空闲 Enter/Esc/其他键 | 仅映射 Finish/Cancel | Finish/Cancel/None | Pass |
| MAC-COLOR-OUTPUT-22-01 | Debug/Release `--smoke-test` | 真实进程编码 PNG、私有 pasteboard PNGf 回读一致、AppKit 内存解码 2×2；脚本断言该行 | 两配置均 `clipboard_pngf_smoke=pass` | Pass |
| MAC-COLOR-OUTPUT-22-02 | worker 线程调用剪贴板/音效 | AppKit 副作用前明确拒绝 | NotMainThread / NotMainThread / false | Pass |
| PRIVACY-01 | 完整入口驻留 0.5 秒 | 无网络/监听 socket | 两配置通过，0 sockets | Pass |

## 说明与限制

- PNG 字节预算由 `encode_png` 的预算 writer 在每次写入前检查，超限即停止并
  返回 ResourceLimit；受 24M 像素上限约束，测试无法构造真实超限输入，故没有
  可达的 128 MiB 集成 fixture。
- 真实拖拽/双击/完成按钮、系统剪贴板粘贴、音效播放、失败注入和颜色真值仍需
  用户参与。合成 PNG 的内存解码已证明尺寸/像素/sRGB chunk，但不等于原生
  AppKit 解码后的颜色真值。

## 仍为用户参与的原生子用例

以下项目需要真实鼠标、系统剪贴板、音效或权限，本轮未执行。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-REGION-MANUAL-22-01 | 授权后拖出已知矩形并 Enter | 2x 尺寸/内容正确、无文件写入 | 未执行（Screen Recording preflight=false） | Not tested |
| MAC-REGION-MANUAL-22-02 | 单击、双击选区内/外 | 单击不建选区；仅选区内双击完成 | 未执行 | Not tested |
| MAC-REGION-MANUAL-22-03 | 右上“完成”与 Esc；输出期输入 | 输出一致；Esc 无复制；输出期忽略编辑/重复 | 未执行 | Not tested |
| MAC-COLOR-MANUAL-22-01 | 非 ArcShot 应用粘贴与系统解码 | 粘贴成功、尺寸/像素/PNGf 正确 | 未执行（需要用户操作剪贴板） | Not tested |
| MAC-RECOVERY-MANUAL-22-01 | 注入剪贴板失败/超预算后重试 | 保留文档、可重试、Esc 不复制 | 未执行 | Not tested |
| MAC-COLOR-MANUAL-22-02 | 四象限夹具框选并内存解码 | 四角颜色/方向正确 | 未执行 | Not tested |
