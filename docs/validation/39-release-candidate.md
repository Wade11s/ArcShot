# #39 发布候选复评：V1 自动化与实机门禁

本记录把 #62（系统字体）与 #63（选区/对象 AX 接缝）并入 `spec/v1` 后，在同一候选上重跑
完整 Debug/Release E2E 矩阵、T-04 性能门禁与资源上界，并补齐 #39 的两个阻塞接缝：
第三方应用实际粘贴（MAC-E2E-40）与采集期目标窗口关闭（MAC-E2E-41）。

结论：**自动化与代理可驱动的实机项全部通过，但 §4.4 用户项与剩余原生故障类仍未验证，
因此仍不能放行 V1。** 本记录不关闭 #18，也不执行签名、公证或自动更新。

## 候选与环境

- 实测实现/脚本 SHA：`48973ed6e5e3adabd426cfc7307f9452cf4a89c4`
  （`test(e2e): cover native target loss and third-party paste`，`worktree_dirty=false`；
  产品修复在 `cd4d668`）。本记录是后续纯文档提交。
- 时间：2026-11-07T12:53:26Z（`scripts/record-validation.sh` 采集）。
- 构建配置：Release。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 显示器：内建单屏 1470×956 逻辑点，scale=2。
- 权限：`accessibility=granted`、`screen_recording=granted`；未改动任何 TCC 设置。
- `agent-desktop`：1.2.0（envelope 2.6）。
- 夹具：`arcshot-scroll-fixture`（24M 几何）、TextEdit `e2e-36-textedit.txt` /
  `e2e-41-target-loss.txt`、Safari `safari-36-v1` 与 `e2e-40-paste.html`；均由脚本在
  `target/` 下生成，不入库。
- 日志：`target/e2e-full-62-debug-11.log`、`target/e2e-full-62-release-4.log`、
  `target/perf-62-idle.log`、`target/perf-62-scroll.log`、`target/perf-62-24m-perf.log`、
  `target/perf-62-24m-resource-1.log`、`target/check-all-62-final.log`。
  日志是本地构建产物，不入库。

## 执行

```sh
scripts/check-all.sh
scripts/e2e-agent-desktop.py --no-build
scripts/e2e-agent-desktop.py --release --no-build
scripts/perf-measure.py --no-build --no-sampler \
  --metrics region_sck,interact,clipboard_export --log target/perf-62-idle.log
scripts/perf-measure.py --no-build --no-sampler \
  --metrics scroll,scroll_start --log target/perf-62-scroll.log
scripts/perf-measure.py --no-build --no-sampler \
  --metrics export_24m --runs 5 --log target/perf-62-24m-perf.log
scripts/perf-measure.py --no-build \
  --metrics export_24m --runs 1 --log target/perf-62-24m-resource-1.log
```

- `scripts/check-all.sh`：exit 0（fmt、clippy `-D warnings`、348 Rust tests、doc、validation、
  依赖、架构、文字后端、Debug/Release 启动与隐私冒烟）。
- `scripts/e2e-agent-desktop.py --no-build`：exit 0，
  `mac_e2e=pass configuration=debug cases=41`，`total_s=409.0`、`agent_desktop_calls=851`。
- `scripts/e2e-agent-desktop.py --release --no-build`：exit 0，
  `mac_e2e=pass configuration=release cases=41`，`total_s=382.3`、`agent_desktop_calls=853`。
- 五条性能/资源命令的 `perf_gate=PASS`；24M 性能 5 次为
  `601853/677657/665688/712473/666956 µs`，PNG `1500×16000`、`5,593,285` 字节。

## T-04 门槛（同一候选）

| 稳定指标 | n | median | p95 | max | 门槛 | 结果 |
| --- | --- | --- | --- | --- | --- | --- |
| PERF-INTERACT | 151 | 1 | 5 | 18 | ≤100000 | Pass |
| PERF-REGION | 93 | 127370 | 143892 | 491001 | p95 ≤300000 | Pass |
| PERF-SCROLL-START | 31 | 341736 | 368691 | 372716 | ≤1000000 | Pass |
| PERF-FRAME-PREVIEW | 137 | 84477 | 90233 | 101661 | p95 ≤200000, max ≤500000 | Pass |
| PERF-FRAME-PREVIEW（24M 构建） | 1587 | 98997 | 122720 | 150333 | p95 ≤200000, max ≤500000 | Pass |
| PERF-EVENT-PREVIEW | 30 | 67211 | 133346 | 146186 | p95 ≤400000, max ≤750000 | Pass |
| PERF-EVENT-PREVIEW（24M 构建） | 488 | 60695 | 137685 | 165114 | p95 ≤400000, max ≤750000 | Pass |
| PERF-SCK | 93 | 34700 | 41510 | 219427 | p95 ≤150000 | Pass |
| PERF-STITCH | 219 | 430 | 489 | 783 | ≤100000 | Pass |
| PERF-STITCH（24M 构建） | 10231 | 510 | 608 | 3850 | ≤100000 | Pass |
| PERF-EXPORT-2M | 30 | 28578 | 30445 | 32075 | ≤2000000 | Pass |
| PERF-EXPORT-6M | 30 | 47138 | 48665 | 50993 | ≤5000000 | Pass |
| PERF-EXPORT-24M | 5 | 666956 | — | 712473 | 每次 ≤20000000 | Pass |
| PERF-CLIPBOARD | 60 | 323 | 376 | 545 | ≤500000 | Pass |
| RESOURCE-MAX | 1 次 24M 导出 | — | — | — | peak physical ≤2 GiB | Pass |

RESOURCE-MAX 实测：`peak_phys_footprint_kb=1016832`（0.97 GiB）、
`peak_footprint_kb=992256`、`peak_rss_kb=999568`；2575 次 RSS / 258 次 footprint 采样。
24M 导出达到 `height=16000`，PNG `1500×16000`。

> 5 次连续导出的资源采样另有一次失败：第三次构建只到 `height=9856`，夹具提前停止，
> 采样数值不作为证据。资源上界以一次完整 16000 行导出的峰值为准；5 次性能门槛由
> `perf-62-24m-perf.log` 单独给出。

## 新增/复跑的原生用例

| 用例 | 结果 | 证据 |
| --- | --- | --- |
| MAC-E2E-40（第三方粘贴 smoke） | Pass | ArcShot Region Finish 复制 `1000×600` PNG；Safari contenteditable 收到物理 `Cmd+V`，页面解码后标题为 `pasted 1000x600`；`mac_e2e_40=pass … decoded=true` |
| MAC-E2E-41（采集期目标关闭） | Pass | TextEdit 在 Collecting 中被强制关闭；产品以 `scroll_refused reason=Stitch Gap` 停止（`target_check=raced`），不输出；验证过的部分经 AX abandon 后覆盖层返回并取消；`mac_e2e_41=pass … output=none overlay_returned=true` |
| MAC-E2E-39（系统字体 CJK/emoji） | Pass | Debug/Release 矩阵均含 `mac_e2e_39`；见 `docs/validation/62-system-text.md` |
| MAC-E2E-26_esc 等物理输入 | Pass | 完整矩阵一次运行内通过；含 #39 之后的加固：瞬态系统 HUD 等待、TextEdit fresh 重开、滚动惯性容差、frontmost 轮询、丢失 keyup 的二次释放、global hotkey 重投 |

## 仍未验证（发布前必须解决或由用户确认）

1. **§4.4 用户项**：真实中文 IME 组合与候选窗、真实 TCC 授权/撤权往返、视觉与审美判断
   （状态项图标、标注排版、贴边布局、密度观感、三秒预览动画）、多显示器与物理输入设备差异。
2. **剩余原生故障类**：SCK 错误/超时、捕获 worker panic、内屏断开。目标窗口关闭已由
   MAC-E2E-41 覆盖；其余仍只有注入式策略证据，不能当作实机通过。
3. **真实指针动态**：选区/对象手势的语义路径已由 MAC-E2E-38 覆盖，真实按下—移动—释放的
   手感与命中容差仍属物理输入。
4. **系统升级后的重跑**：本记录绑定 macOS 27.2 (26B123)，升级后不能继承。

## 放行判定

自动化与代理可驱动项全部通过；#39 原有的阻塞项 1（指针手势证据）、4（第三方粘贴）、
5（Release 完整矩阵）已由 #63、MAC-E2E-40、Release 41/41 解决或覆盖，6（升级后重跑）
仍取决于系统状态。**剩余阻塞是 §4.4 用户项与 SCK/worker/内屏断开三类原生故障**；
需要用户完成或明确豁免后，才能把本候选标为 V1 放行。

## 隐私

- 运行期无网络、无遥测；`scripts/check-all.sh` 内的 `scripts/check-no-network.sh` 通过。
- 剪贴板 PNG 只在内存/临时文件中解码，临时文件随即删除；仓库树不新增截图。
- 未改动 TCC、登录项或系统设置；`MAC-E2E-15/16/22` 结束后偏好已还原。
