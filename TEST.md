# ArcShot 测试

本文件从 #19 起建立。自动化只声明实际覆盖；macOS 原生冒烟不替代完整发布矩阵。

## 固定接缝与命令

| 稳定 ID | 接缝与命令 | 断言 | 未覆盖 |
| --- | --- | --- | --- |
| ARCH-01 | `scripts/check-architecture.sh` | 四 crate 方向、GPUI 仅在 app | crate 内模块质量 |
| TEXT-01 | `scripts/check-text-backend.sh` | GPUI font-kit 返回真实 Helvetica `i < W` advance | 中文 IME、emoji、实际标注排版 |
| MAC-START-01 | `scripts/check-startup.sh` | 从 bundle 启动，Accessory 无 Dock 策略，真实 NSStatusItem/图像/菜单可见，`terminate:` action 排队后 exit 0；5 秒看门狗 | 人工 Finder 点击、菜单视觉布局、多系统版本 |
| PRIVACY-01 | `scripts/check-no-network.sh` | 正常驻留至少 0.5 秒且 `lsof -i` 无 socket | 所有系统/后续功能的永久网络证明 |
| BUILD-RELEASE-01 | `scripts/check-startup.sh --release` | Release bundle 同样完成原生驻留/退出 | 签名、公证、性能与资源发布门槛 |

每次完整验证运行：

```sh
scripts/check-all.sh
```

该入口还运行 `cargo fmt --check`、Clippy warnings-as-errors、工作区测试和
`cargo doc --no-deps`。原生检查使用正式产品二进制和 AppKit 对象，不是 mock
窗口；没有截图捕获、取证图像或网络 fixture。

## 证据

- #19 的环境、步骤、实际结果与限制：`docs/validation/19-startup.md`。
- 未授予权限的自动 UI 操作、Finder 人工视觉检查及尚未实现的命令标为
  **Not tested**，不冒称 Pass。
- 后续切片沿用父规格 T-02/T-03 的 CORE、IMAGE、APP 与 MAC 接缝，不为私有实现
  另造测试接口。
