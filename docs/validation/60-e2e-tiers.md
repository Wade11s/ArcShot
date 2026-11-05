# #60 E2E 分档与耗时度量验证记录

本记录证明 `scripts/e2e-agent-desktop.py` 的分档、强制与计时机制，`--tier quiet` 两个单元与
`--tier ax` 单元（`notice_ax`）的实机通过，以及二审后的加固与复跑。它**不是**完整 37 用例的
验收证据：`physical`、`full` 两档没有运行（见 Not tested）。未保存任何截图或屏幕图像，未联网，
未改动系统权限或登录项。

## 实测来源与环境

- 实测实现/脚本 SHA：`3a1f767ea501066cb51dc79da08cef1e063471dc`（`feat(e2e): split the
  agent-desktop run into tiered units with timing`）；其后的文档提交只新增本报告。二审后的修正
  与复跑在 `33e2d7d71919b29f5de879026bdcc30f6ec966aa` 上做，环境块与结果见下文。
- 时间：2026-11-05T13:08:56Z（元数据采集）；`--tier quiet` 在其前数分钟运行。
- 构建配置：Debug。bundle 为 `dev.arcshot.app.debug` / `ArcShot Debug`。
- 工具：`agent-desktop 1.2.0`（envelope `2.6`）。脚本自动改调 npm 包旁的原生二进制并设置
  `AGENT_DESKTOP_MACOS_HELPER_PATH`。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建，1470×956 逻辑点，scale=2。
- 权限：启动 `agent-desktop` 的应用持有辅助功能与屏幕录制（`agent-desktop status`：
  `accessibility=granted`、`screen_recording=granted`、`automation=unknown`）。
- 夹具：`fixture=agent-desktop-e2e-v1`；`quiet` 档的两个单元不启动任何夹具窗口。
- 日志：`target/e2e-quiet-60-run.log`（脚本 stdout/stderr 原文；产品 stderr 在
  `target/e2e-agent-desktop-debug*.log`）。日志为本地构建产物，不入库。

## 自动记录的环境

```text
commit_sha=3a1f767ea501066cb51dc79da08cef1e063471dc
worktree_dirty=false
timestamp_utc=2026-11-05T13:08:56Z
build_configuration=debug
machine=Mac14,2
architecture=arm64
macos=27.2 (26B123)
rustc=rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo=cargo 1.99.1 (5f94df478 2026-08-27)
xcode=Xcode 27.0 (Build version 27A266a)
macos_sdk=27.0 (26A425)
swift=Apple Swift version 6.4 (swiftlang-6.4.0.34.1 clang-2100.3.34.1)
metal=Apple metal version 32023.921 (metalfe-32023.921.6)
log=target/e2e-quiet-60-run.log
```

## 执行

```sh
scripts/e2e-agent-desktop.py --tier quiet
```

结果：退出码 0，`mac_e2e=partial configuration=debug tier=quiet units=identity,sound_prefs
cases=7/37 acceptance=false`。运行前后 `lsappinfo front` 相同（前台应用未变）。

二审后的复现命令（TIER-* 三行证据；前两组为离线检查，不驱动产品）：

```sh
# TIER-UNITS-60-01：单元与用例归属、错误选择
scripts/e2e-agent-desktop.py --list                    # 20 个单元、37 条用例
scripts/e2e-agent-desktop.py --tier quiet --only 05    # 报“--only names units above --tier”
scripts/e2e-agent-desktop.py --tier quiet --skip identity,sound_prefs  # 报“the selection is empty”

# TIER-ENFORCE-60-01：档位拒统（离线调用 physical_violation 矩阵，不执行 agent-desktop）
python3 - <<'PY'
import importlib.util
spec = importlib.util.spec_from_file_location("e2e", "scripts/e2e-agent-desktop.py")
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
for args, refused in [
    (("press", "escape", "--headed"), True), (("press", "escape"), True),
    (("hover", "@x:e1"), True), (("drag", "@a", "@b"), True),
    (("mouse-click", "10", "20"), True), (("snapshot", "--app", "ArcShot"), False),
    (("click", "@x:e1"), False), (("list-apps", "--app", "ArcShot"), False),
    (("--session", "abc", "press", "escape"), True),
    (("--session", "abc", "snapshot", "--app", "ArcShot"), False),
    (("-w", "menuitem:\u9000\u51fa", "snapshot"), False),
    (("--wait-for", "button:OK", "press", "return"), True),
    (("batch", '[{"command":"press","args":["escape"]}]'), True),
    (("launch", "TextEdit", "--activate"), True),
    (("close-app", "TextEdit", "--force"), True),
    (("open-system-surface", "menu-bar-extras"), True),
    (("type", "@x:e1", "press"), False),
]:
    assert (m.physical_violation(args) is not None) == refused, args
print("tier guard matrix ok")
PY

# TIER-CLEAN-60-01：含截图的单元先查层面窗口
scripts/e2e-agent-desktop.py --only scroll_fixture --no-build   # physical 档，仅在机器空闲时跑
```

## 结果

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-09 | 启动后 `list-apps --app "ArcShot"` | 只出现被测 bundle | `dev.arcshot.app.debug` / `ArcShot Debug`，一个实例 | Pass |
| MAC-E2E-01 | `snapshot --surface menu-bar-extras` | 七个菜单项与 selector | `menu_items=7 refs=8` | Pass |
| MAC-E2E-17 | 读菜单与启动的登录项状态 | 状态一致、从不写 | `login_item_actual=Disabled`，无 `login_item_set` | Pass |
| MAC-E2E-02 | 驻留时 `list-windows --app` | 空 | `own_windows=0` | Pass |
| MAC-E2E-15 | AX 切换完成音效两次 | 每次写 `persisted=true` | `sound=true->false->true` | Pass |
| MAC-E2E-08 | 点击退出 | 进程 exit 0、状态项移除 | `exit=0 status_item=removed` | Pass |
| MAC-E2E-16 | 真实重启两次 | 偏好跨重启保持并还原 | `restart_restored=false`、`restart_restored=true` | Pass |
| TIER-ENFORCE-60-01 | 以 `ax` 档标记调用 `press`/`--headed`/`hover`/`drag`/`osascript` | 全部拒绝并报“单元档位标错” | 逐条拒绝 | Pass |
| TIER-UNITS-60-01 | `--list` 与解析 | 37 个用例各归属唯一单元；`--only` 超出 `--tier` 报错；空选择报错 | 满足 | Pass |
| TIER-CLEAN-60-01 | 干净度检查：实机读取与模拟横幅 | 实机无误报；模拟的第三方浮层被点名拒绝；屏外 Stage Manager 缩略图不计 | 满足（修复过一次误报） | Pass |

## 耗时数据与对剩余优化的判断

`--tier quiet` 共运行三次（前两次在提交前的同一份代码上，第三次在 SHA `3a1f767…` 上，含构建；
第一次在收尾处崩溃，见下节，用例数据仍有效）：

| 项 | 运行 1 | 运行 2（`--no-build`） | 运行 3（本 SHA） |
| --- | --- | --- | --- |
| `build`（缓存命中） | 1.6 s | 跳过 | 1.5 s |
| `identity` 单元（4 个用例） | 0.8 s | 1.0 s | 0.8 s |
| `sound_prefs` 单元（3 个用例，3 次启动/重启） | 4.4 s | 5.5 s | 4.2 s |
| 总计 / `agent-desktop` 调用次数 | 8.7 s / — | 7.3 s / 23 | 7.8 s / 23 |

由此得到的数字：一次应用启动或重启约 1.1 s；完整运行共 6 次启动，约 7 s；6 处等待
`completion_preview_expired` 受 3 s 固定停留限制，约 18 s；6 次夹具启动含 1 s 等待，约 8 s。合并重启、共用夹具与
去除等待在最好情况下合计约 30 s，且其中预览过期等待是验收断言。留存的历史运行里，同一票据在 43–129
分钟内反复运行 17–25 次，主要成本是失败后的重跑，不是单次编排。因此：

- **合并重启 / 共用夹具**：不执行。收益约 3–8 s，却要改动对时序敏感、且本机尚未重新跑通的 `physical` 用例。
- **去除预览过期等待**：不执行。它是 `TEST.md` 规定的验收断言，删除会削弱证据。
- 真正降低总时间的是减少失败重跑：分档、`--only … --no-build` 重跑单元与干净度检查已落地。

## 同轮发现并修复的缺陷

首次实机运行在 `quiet` 档全部用例通过后，于收尾处抛出 `UnboundLocalError`：`main()` 内的局部变量
`finish` 遮蔽了同名的模块级函数。已把函数改名为 `finish_run`，第二次运行通过。

## 二审（PR #61）后的加固与复跑

两轴审查（Standards：`kimi-coding/k3.1-pro`（high）；Spec：`xai/grok-4.7`（xhigh），两个模型独立分析）
针对 `3a1f767` 指出四条会落空的文档承诺，已在 `33e2d7d`（`fix(e2e): hold quiet/ax units to
their own tier and report failed runs`）修正：

1. 档位门槛只看 `args[0]` 与字面 `--headed`：`--session <id> press`、`batch` 里的物理命令都能穿过；
   `launch`/`close-app`/`open-system-surface` 不投递按键却改变前台与运行中的应用，未被拒。现在
   `command_of()` 跳过前置全局选项取子命令，`physical_violation()` 补上这三组拒绝。
2. 门槛是整轮上限，`quiet`/`ax` 单元在完整运行里仍可发真实输入。现在 `want()` 以单元自身档位为准。
3. 失败的运行没有汇总行，部分运行失败时看不到 `acceptance=false`。现在失败打印
   `mac_e2e=fail … acceptance=false`，单元内失败另加 `e2e_timing unit=… failed=true`。
4. `--list` 对唯一的 `ax` 单元 `notice_ax` 不给 blocker；CI `e2e` 作业可与 `native` 并行占同一台
   桌面（已加 `needs: native`）；`docs/agents/ci.md` 的表格被空行断开；`TEST.md` 的“单元以应用
   驻留结束”与 `pass` 行触发条件不准。

复跑环境（同一台机器，代码 `33e2d7d`，干净工作区）：

```text
commit_sha=33e2d7d71919b29f5de879026bdcc30f6ec966aa
worktree_dirty=false
timestamp_utc=2026-11-05T19:04:13Z
build_configuration=debug
machine=Mac14,2
architecture=arm64
macos=27.2 (26B123)
rustc=rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo=cargo 1.99.1 (5f94df478 2026-08-27)
xcode=Xcode 27.0 (Build version 27A266a)
macos_sdk=27.0 (26A425)
swift=Apple Swift version 6.4 (swiftlang-6.4.0.34.1 clang-2100.3.34.1)
metal=Apple metal version 32023.921 (metalfe-32023.921.6)
display=内建 1470×956 逻辑点, scale=2
permissions=accessibility=granted, screen_recording=granted, automation=unknown
fixture=agent-desktop-e2e-v1
log=target/e2e-quiet-61-run.log, target/e2e-ax-61-run.log
```

| ID | 步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| TIER-ENFORCE-61-01 | 离线跑 `physical_violation` 矩阵 17 组（含前置 `--session`/`-w`、`batch`、`launch`/`close-app`/`open-system-surface`，以及 `type "press"` 这种值不误报） | 只拒物理与桌面类 | 17/17 一致 | Pass |
| TIER-UNIT-61-01 | 离线：`_TIER="full"` 选 `sound_prefs` 后调 `agent("press", "escape")` | 仍拒绝（不再借用整轮上限） | 拒绝，`_ALLOW_PHYSICAL=False`；`density_prefs` 在 `full` 下仍为 True | Pass |
| TIER-FAIL-61-01 | 先手动启动一个实例，再 `--tier quiet --only identity --no-build` | 前置失败并打印失败汇总 | `mac_e2e=fail tier=quiet units=identity cases=4/37 acceptance=false`，退出码 1 | Pass |
| MAC-E2E-10 | `--tier ax --no-build` | 权限提示可读可关，且不发真实输入 | `mac_e2e_10=pass notice_texts=2 action=arcshot.notice.close readable=46`；`unit=notice_ax seconds=1.3` | Pass |
| MAC-E2E-09/01/17/02/15/08/16 | `--tier quiet`（含构建） | 与 `3a1f767` 时相同 | 7 条全 Pass；`e2e_timing total_s=6.9 agent_desktop_calls=23` | Pass |
| TIER-AX-61-01 | `--tier ax --no-build` | quiet + ax 共 8 条 | `mac_e2e=partial … tier=ax units=identity,notice_ax,sound_prefs cases=8/37 acceptance=false`，退出码 0 | Pass |

两次运行前后 `lsappinfo front` 相同（`ASN:0x0-0x2002000:`）。TIER-FAIL-61-01 为制造前置失败
而手动启动了一个 Debug 实例，并用状态菜单退出；期间误点“登录时启动”（产品日志
`login_item_set enabled=true outcome=Applied`），已立即点回（`login_item_set enabled=false
outcome=Applied`、`login_item_status state=Disabled source=verify`），随后的 quiet/ax 运行 MAC-E2E-17
又读回 `login_item_actual=Disabled`。最终无残留实例、无登录项变化。

## Not tested

- **`physical` 与 `full` 档的 17 + 1 个单元**：未运行。它们会冻结内置屏、出现自有窗口或发送真实键鼠，
  而使用者正在同一台机器上工作。拆分重构对这些单元的运行时行为（作用域、提升到 `main()` 外层的
  `drag_doc`/`activate_tool`、`density_prefs` 的前置退出）只经过静态检查（符号解析、逐行 `diff -w`），
  **没有实机证据**。完整运行 `mac_e2e=pass … cases=37` 之前，不得把本分支视为已回归。新增的
  `batch`/`launch`/`close-app` 拒绝与按单元限档只在 `quiet`/`ax` 单元上实机验证过；它们对
  `physical`/`full` 单元只应放宽不应收紧，但这一结论同属未运行。
- **CI `e2e` 作业**：未触发。`gh api repos/Wade11s/ArcShot/actions/runners` 当前没有任何已注册的
  runner，因此手动派发也只会排队；`needs: native` 的串行假设因此未在 CI 上验证。
- **产品侧 AX 接缝**（悬停选项与选区）：不在本票范围，未实现。
