# #41 代理 guardrails 验证记录

- 实测实现/脚本 SHA：`a12e0ede91c611cad0df95330a34882c03fc3ab6`。
- 范围：CI、证据工具、source map 与依赖检查；无 ArcShot 产品行为变化。
- 内建显示器：`display=1470x956 scale=2.0`；本票未使用显示捕获。
- 权限：`permissions=未请求新增权限；未注册 runner，未修改 branch protection`。
- 夹具：`fixture=validation-fixtures-v1`，仅文本 metadata 与临时日志，无图像或网络 fixture。

## 自动记录的环境

以下内容由 `scripts/record-validation.sh` 在上述 SHA 生成，工作区当时无未提交差异：

```text
commit_sha=a12e0ede91c611cad0df95330a34882c03fc3ab6
worktree_dirty=false
timestamp_utc=2026-10-29T14:28:43Z
build_configuration=release
machine=Mac14,2
architecture=arm64
macos=27.2 (26B123)
rustc=rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo=cargo 1.99.1 (5f94df478 2026-08-27)
xcode=Xcode 27.0 (Build version 27A266a)
macos_sdk=27.0 (26A425)
swift=Apple Swift version 6.4 (swiftlang-6.4.0.34.1 clang-2100.3.34.1)
metal=Apple metal version 32023.921 (metalfe-32023.921.6)
log=target/startup-smoke-debug.log
log=target/startup-smoke-release.log
log=target/resident-smoke.log
```

## 稳定用例

| ID | 前置与步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CHECK-ALL-01 | 在 macOS 27.2 执行最终 `scripts/check-all.sh` | Rust、证据、依赖、文字和 Debug/Release 原生检查全部成功 | 全部 exit 0；包括 `validation_tools=pass`、`dependency_inspection=pass`、`startup_smoke=pass` | Pass |
| EVIDENCE-01 | 运行记录/报告工具及负面对照 | 完整报告通过；缺/空字段、非法配置、占位、工具/Git 失败均拒绝 | `validation_tools=pass`；既有 #19 报告兼容通过 | Pass |
| DEPENDENCY-01 | 运行 metadata fixtures 和实际依赖检查 | normal/build/dev、重复版本、异常来源与畸形输入均被正确处理 | `dependency_inspection=pass`；实际来源仅 path/crates.io | Pass |
| CI-SOURCE-01 | PR 触发 hosted `source` job | macos-26 上仅源码级检查成功，不执行产品 | Run `40890618289` / job `123506091085`，conclusion `SUCCESS`，2026-10-29T14:22:01Z | Pass |
| CI-NATIVE-01 | PR 检查 native job；之后由用户配置 runner | 未注册 runner 时不执行、不假报通过；仅手动 dispatch 可在 macOS 27 运行 | PR 中 conclusion `SKIPPED`；self-hosted 完整检查未执行 | Not tested |

## 审查与限制

- 两轮 Standards / Spec 独立审查发现的问题均已修复并重新完整验证：工具管道假成功、报告缺/空字段、Git status 假 clean、build/dev 依赖遗漏及依赖失败测试缺口。
- Source CI 通过不代表 Rust 编译、测试、文档或 macOS 27 原生 CI 通过；这些由本地 CHECK-ALL-01 和后续 self-hosted native job 分别证明。
- 本地构建仍出现 duplicate `-rpath /usr/lib/swift` linker warning；未影响本票检查结果，未在本环境票据中改产品链接配置。
- 未保存截图或取证图像；临时 metadata 和日志仅文本。
