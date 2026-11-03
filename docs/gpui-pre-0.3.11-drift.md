# 0.2.2 → 0.3.11 API 漂移清单（#51 → #52 输入）

对象：`gpui` 0.2.2（官方包名的最新版）→ `gpui-pre` 0.3.11（ADR-0001 选定的基座，crates.io 上最新的快照，2026-11-02，`zed@f816e1f`）。这是 #51 的交付物之一，供 #52 直接照单迁移。

方法：在丢弃分支 `ticket/51-spike-gpui-pre`（worktree `.worktrees/51-spike-gpui-pre`，提交 `8a28d54`，0.3.11 版；同分支 `817eada` 是 0.3.8 版）里把 ArcShot 全量迁移一遍；凡“编译器接受原样代码”记为 CONFIRMED（编译级），凡读了 0.3.11 源码记为 CONFIRMED（源码级）；只有文档/推演支撑的记 SUSPECTED。源码指 `cargo fetch --locked` 后 `~/.cargo/registry/src/*/gpui-pre-0.3.11`、`gpui-pre-platform-0.3.11`、`gpui-pre-macos-0.3.11`、`gpui-pre-apple-0.3.11`、`accesskit-0.24.1`（本次另解包到 `/tmp/gpuisrc/` 便于对照 0.2.2）。实机取证与原始命令输出见 `/tmp/arcshot-spec-notes/51-proof.log`（外部工作记录，不随仓库提交）。

迁移后实际改动面：先做 0.2.2 → 0.3.8（12 个编译错误），再把整族升到 0.3.11（**0 个编译错误**）。`main.rs` 5 处、`coordinator.rs` 7 处、`ui.rs` 全是新增标注（无签名改动）——**0.2.2 的产品代码在 0.3.11 上只需 §0 那 12 处改动**。

## 0. 迁移总量

- 12 个编译错误全部集中在：`Timer`、`Application::new`、`Entity::update`/`AsyncApp::update` 的 `Result`、`Window::focus(cx)`、`DisplayId(u32→u64)`。
- 没有一处在 `div`/style/`img`/`RenderImage`/`on_key_down`/`on_click`/`stop_propagation`/`WindowOptions` 字段名上。
- 0.3.8 → 0.3.11 之间没有新增编译错误；只有源码级的小改（§10）。

## 1. `Application::new` / `Application::run`

| 项 | 0.2.2 | 0.3.11 | 级别 |
| --- | --- | --- | --- |
| 构造 | `Application::new()` | **删除**；只有 `Application::with_platform(Rc<dyn Platform>)` 与 `Application::new_inaccessible(platform)`（app.rs:242 / 259） | CONFIRMED（编译 + 源码） |
| 平台实现 | 编在 `gpui` 内（`src/platform/mac/*`） | 拆到 `gpui-pre-macos` → 再下探 `gpui-pre-apple`；由新 crate `gpui-pre-platform` 的 `gpui_platform::application()` / `headless()` / `current_platform(headless)` 选出 | CONFIRMED（源码 `gpui-platform/src/gpui_platform.rs`） |
| `run(F)` | `Application::run(FnOnce(&mut App))` 阻塞 | 同签名 | CONFIRMED（编译） |
| 新增 | — | `Application::run_embedded(F) -> ApplicationHandle`、`with_windowing(WindowingRequest)`、`with_quit_mode(QuitMode)`、`with_restart_arguments` | CONFIRMED（源码） |
| `Application::text_system()` | `Arc<TextSystem>` | `Arc<TextSystem>`（app.rs:393，未变） | CONFIRMED（编译 + 源码） |
| 激活策略 | `gpui` 不管，产品自己 `setActivationPolicy(Accessory)` | `gpui-pre-macos` 自己持有 `ActivationPolicy`（`application()` 默认 `Regular`，headless 为 `Accessory`，platform.rs:201/238），并有 `set_activation_policy`/`request_windowing`；产品随后设置的 Accessory 仍然生效（`startup_smoke=pass activation_policy=Accessory`） | CONFIRMED（源码 + 实测） |

对 #52：`main.rs` 的 `Application::new()`（两处：`check_text_backend` 用 `gpui_platform::headless()`，`run_app` 用 `gpui_platform::application()`），并把 `gpui-platform` 加进 `crates/app`。注意**可访问性可以被静默关掉**：`new_inaccessible()` 下 `role()`/`on_a11y_action` 全部 no-op，这就是为什么 ADR 明确要求用 `application()`。

SUSPECTED：0.3.11 让平台先设 `Regular` 再被产品改成 `Accessory`，若将来把 `open_window` 提到 `MenuBar::try_new` 之前，可能出现“短暂 Dock 图标”；当前顺序（MenuBar 先建）不受影响。

## 2. `App::open_window` 与 `WindowOptions`

| 项 | 0.2.2 | 0.3.11 | 级别 |
| --- | --- | --- | --- |
| `App::open_window(opts, build)` | `-> anyhow::Result<WindowHandle<V>>` | 同签名、同 `Result`（app.rs:1612） | CONFIRMED（编译 + 源码） |
| `window_bounds` / `WindowBounds::centered(size, cx)` | 有 | 未变 | CONFIRMED（编译） |
| `titlebar: Option<TitlebarOptions>` | 有；`None` = 无标题栏 | 未变；覆盖层仍无色带、仍不进 `list-windows` | CONFIRMED（编译 + 实测） |
| `kind: WindowKind` | 有 | 未变字段 | CONFIRMED（编译） |
| `display_id: Option<DisplayId>` | `DisplayId(u32)`，`From<u32>`/`Into<u32>` | `DisplayId(u64)`（platform.rs:777），`From<u64>`/`Into<u64>`；`u32::from(..)` 不再编译 | CONFIRMED（编译错误 + 源码） |
| `is_resizable` / `is_minimizable` / `is_movable` / `focus` / `show` | 有 | 同名同类型 | CONFIRMED（编译） |
| 新增字段（5 个） | — | `app_owns_titlebar_drag: bool`（macOS 自绘标题栏拖动）、`inactive_frame_interval: Option<Duration>`（默认 33_333µs）、`low_power_frame_interval: Option<Duration>`（默认 33_333µs）、`thermal_frame_interval: Option<Duration>`（默认 16_667µs）、`use_display_p3: bool`（默认 false） | CONFIRMED（源码 + `Default` impl，platform.rs:2792） |
| `Default` | 手写 `impl Default`（`titlebar: Some(..)`、`focus/show/is_movable/is_resizable/is_minimizable = true`） | 同为手写 `impl Default`，多出上面 5 个字段 | CONFIRMED（源码） |

对 #52：`..Default::default()` 的写法继续可用；`display_id` 的比较改成 `u64::from`。三个帧间隔默认值对“覆盖层失焦后动画/预览”是否需要显式设 `None` 未验证（SUSPECTED：覆盖层始终 focused，应先不设，等 PERF-FRAME-PREVIEW 实测）。

## 3. `WindowKind::PopUp`

- 0.2.2：`Normal | PopUp | Floating`。
- 0.3.11：`Normal | PopUp | AnchoredPopup(PopupOptions) | Floating | Dialog | LayerShell(..)`(Linux)（platform.rs:2839）。
- `PopUp` 语义（置顶、无父窗口锚定）未变；实测覆盖层仍是 `PopUp + titlebar: None`，仍然**不**出现在 `list-windows`，但 AX 子树可读可驱动。级别：CONFIRMED（源码 + 实测）。
- 新增 `AnchoredPopup`（父窗口锚定、原生菜单/提示型）与 `Dialog`；对 ArcShot 的覆盖层是否更合适：SUSPECTED（未试，`AnchoredPopup` 会把尺寸原点当锚点，不适合全屏冻结层）。

## 4. 元素 / 样式 / 焦点 / 事件

以下全部 **CONFIRMED（编译级）**——原代码一行未改即通过：

- `div()`、`.size_full()`/`.p_6()`/`.flex()`/`.gap_4()`/`.rounded_md()`/`.bg()`/`.text_color()`/`.text_size()`/`.text_xl()`/`.text_sm()`/`.absolute()`/`.top_4()`/`.left_4()`/`.relative()`/`.flex_1()`、`.hover(|s| …)`、`.cursor_pointer()`、`.children(Option<impl IntoElement>)`。
- `img(Arc<RenderImage>)`：`pub fn img(source: impl Into<ImageSource>) -> Img` 未变（0.2.2 img.rs:198 / 0.3.11 同名签名）。
- `.track_focus(&FocusHandle)` 未变。
- `.on_key_down(|event, window, cx|)`、`.on_click(|event, window, cx|)` 闭包形状未变；`cx.stop_propagation()` 未变。
- `gpui::Keystroke { key, modifiers }` 与 `modifiers.{platform,control,alt,shift}` 未变。
- `cx.new` / `cx.entity` / `cx.notify` / `cx.focus_handle` / `cx.set_global` / `cx.global` / `cx.global_mut` / `cx.quit` / `cx.activate(bool)` / `cx.drop_image(..)` / `window.window_handle().window_id()`：均未变（编译级；`drop_image`、`activate` 另做了源码对照）。

焦点相关（**有**签名变化，CONFIRMED 编译 + 源码）：

| API | 0.2.2 | 0.3.11 |
| --- | --- | --- |
| `Window::focus` | `focus(&mut self, handle: &FocusHandle)`（window.rs:1386） | `focus(&mut self, handle: &FocusHandle, cx: &mut App)`（window.rs:2618） |
| `FocusHandle::focus` | `focus(&self, window: &mut Window)`（window.rs:340） | `focus(&self, window: &mut Window, cx: &mut App)`（window.rs:610） |
| `window.focus_next/prev` | `(&mut self)`（window.rs:1413/1424） | `(&mut self, cx: &mut App)`（window.rs:2652/…） |

对 #52：`entity.update(cx, |view, _| window.focus(&view.focus))` 必须拆成“先取 handle，再 `window.focus(&handle, cx)`”——闭包里同时借 `cx` 会 E0501（实测两次报错）。

## 5. `RenderImage` / 图像帧

- `RenderImage::new(impl Into<SmallVec<[Frame; 1]>>)`（assets.rs:61）、`id`、`as_bytes(frame_index)`、`size`、`delay`、`frame_count` 全部同名同义。CONFIRMED（源码逐段对照）。
- 帧类型仍是 `image 0.25` 的 `image::Frame`（BGRA，`RgbaImage` 输入），产品 `Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))` 原样编译。CONFIRMED。
- 差异：`RenderImageParams` 由 `pub(crate)` 变 `pub`（对产品无影响）；`size()`/`delay()` 改为越界返回 `default`/`100/1ms` 而不是 panic（更安全，行为差异记在这里）。CONFIRMED（源码级）。

## 6. `cx.spawn` / `Timer` / 后台执行器

| 项 | 0.2.2 | 0.3.11 | 级别 |
| --- | --- | --- | --- |
| `gpui::Timer` | `pub use smol::Timer;`（gpui.rs:94），`Timer::after(d).await` 可用 | **移除**；`gpui` 不再导出 `smol::Timer` | CONFIRMED（编译错误 + 源码） |
| 定时 | `Timer::after` | `cx.background_executor().timer(d).await`（`BackgroundExecutor::timer` 两版都在，executor.rs:357 / 200） | CONFIRMED（编译通过） |
| `App::spawn(f)` | `AsyncFnOnce(&mut AsyncApp) -> R` → `Task<R>` | 同签名；`detach()` 仍在 | CONFIRMED（编译） |
| `AsyncApp::update` | `-> Result<R>` | **`-> R`**（无错误；应用已销毁时不再返回 Err） | CONFIRMED（编译错误 + 源码） |
| `Entity::update` | `-> C::Result<R>` | **`-> R`**（entity_map.rs:497）；实体已释放时**panic**（`update_entity_erased` 里 `unwrap()`，app.rs:3322） | CONFIRMED（源码） |
| `WeakEntity::update` | `-> Result<R>` | `-> Result<R>`（entity_map.rs:803，`upgrade()` 失败即 `Err("entity released")`） | CONFIRMED（源码） |
| `WindowHandle::update` | `-> Result<R>` | `-> Result<R>`（窗口已关仍可判错） | CONFIRMED（编译） |
| `cx.background_executor()` | `-> &BackgroundExecutor` | 同（app.rs:2388） | CONFIRMED（源码） |

对 #52（**极易踩**）：0.2.2 的主循环用 `coordinator.update(cx, ..).is_err()` 退出任务；0.3.11 里 `Entity::update` 不再返回 `Result`，若改成“持强引用 + 无退出条件”，coordinator 因为被闭包捕获而永远存活（spike 就这么做，16ms tick 正常工作）；若要保留“实体没了就退出”，必须显式 `downgrade()` 成 `WeakEntity` 并用它的 `Result`——但**不要**把强引用丢掉（spike 第一版就是强引用被 drop、弱引用 Err 立刻 break，结果整个任务队列停机、菜单命令无响应；这是实测到的真实故障，不是理论风险）。

## 7. `Window::focus` / `handle.update` / `remove_window` / `on_window_should_close`

- `Window::remove_window(&mut self)` → `()`：未变（window.rs:2596）。CONFIRMED。
- `Window::on_window_should_close(&self, cx: &App, Fn(&mut Window,&mut App)->bool)`：未变（0.2.2 window.rs:4329 / 0.3.11:7137）。CONFIRMED。
- `WindowHandle::update(cx, |view, window, cx|)`：仍是 `Result`，用于“窗口关了就不要继续”的判断（`refresh_hotkey_view` 的 `updated.is_err()` 无需改）。CONFIRMED（编译）。
- `Window::focus` 见 §4。CONFIRMED。

## 8. 文字系统

| 项 | 0.2.2 | 0.3.11 | 级别 |
| --- | --- | --- | --- |
| `TextSystem::resolve_font(&Font) -> FontId` | 有 | 同（text_system.rs:351） | CONFIRMED（编译 + 源码） |
| `TextSystem::advance(FontId, Pixels, char) -> Result<Size<Pixels>>` | 有 | 同（text_system.rs:403） | CONFIRMED（编译 + 源码） |
| `gpui::font(family) -> Font` | 有 | 同（text_system.rs:1160） | CONFIRMED（编译 + 源码） |
| `App::text_system()` | `Arc<TextSystem>`（app.rs:222） | `&Arc<TextSystem>`（app.rs:2499） | CONFIRMED（编译：产品只借用，无需改；若持有需 `.clone()`） |
| `Window::text_system()` | `&Arc<WindowTextSystem>` | `&Rc<WindowTextSystem>`（0.3.11 改成 Rc，window.rs:2674） | CONFIRMED（源码；产品未用） |
| macOS 文本后端 | `gpui` 内 `platform/mac/text_system`，`font-kit` feature | 移到 `gpui-pre-macos` → `gpui-pre-apple`；`font-kit` 由 `gpui-pre-platform/font-kit → gpui-pre-macos/font-kit → gpui-pre-apple/font-kit` 逐级转发到 `zed-font-kit`（0.3.11 里 `gpui-pre` 自身的 `font-kit` 已是空 feature） | CONFIRMED（源码/Cargo 元数据） |
| 真实度量 | Helvetica `i < W` | 实测 `i=3.5546875px`, `W=15.1015625px`，`text_backend=pass` | CONFIRMED（实测） |

对 #52：`scripts/check-text-backend.sh` 的断言与输出格式都不用改。真正的风险是 NoopTextSystem：ADR 要求显式打开 `font-kit`，且必须打开在 **`gpui-platform`** 上（0.3.11 的 `gpui-pre/font-kit` 是空 feature，只在旧版本有效）。

## 9. 可访问性 API 入口

新增（0.2.2 完全没有；0.2.2 源码中 `accesskit` 出现 0 次）——全部 CONFIRMED（源码 + 编译 + 实机）：

| 入口 | 说明 |
| --- | --- |
| `div().id("…")` | **必需**。AccessKit `NodeId` 由 `GlobalElementId` 派生；无 id 的元素不进 AX 树。role 只在有 id 时生效。 |
| `.role(accesskit::Role)` | `StatefulInteractiveElement::role`（div.rs:1357）；`Role::GenericContainer` 被 `debug_assert` 拦（会被过滤）。`gpui` 重导出 `Role`、`AccessibleAction`（= `accesskit::Action`）、`Toggled`、`Orientation`、`accesskit` 本体。 |
| `.aria_label(..)` / `.aria_description(..)` | 名称/描述（div.rs:1379 起）；macOS 上成为 AXTitle/AXDescription。 |
| `.accessibility_id(..)` | 新增（div.rs:1373）：映射成 macOS `AXIdentifier`（Windows UIA AutomationId / AT-SPI AccessibleId）。E2E 稳定外部 id 就该用它，实测出现在 `native_id.value`。 |
| `.on_a11y_action(AccessibleAction::Click, move \|action_data, window, cx\| …)` | **AX 动作唯一的处理入口**（div.rs:1550）。实测：只注册 `on_click` 时 `AXPress` 不触发任何产品代码；补上它之后同一引用立刻驱动产品队列。 |
| `.focusable()` / `.tab_stop(bool)` / `.aria_selected(bool)` / `aria_toggled` / 数值/level/set 相关 | 有；`role` 之后才有意义。 |
| `Element::a11y_role()` / `write_a11y_info()` / `a11y_synthetic_children(cb)` | 自定义元素与“一个元素产生多个 AX 节点”的入口（标注画布可能需要）。 |
| `Window::on_a11y_action(node_id, action, listener)` | 底层注册；通常用元素级 API。 |
| `Application::new_inaccessible(platform)` | 全局关闭 a11y（`role()`/`on_a11y_action` 静默 no-op）。 |

**不存在**的（容易被别处经验误导）：

- 没有 `aria_hidden`（0.3.11 源码中 `aria_hidden` 零命中）。想做“不上报”，只能不给 `id`，或用 `Text::new_inaccessible`，或把子树放在不带 role 的节点下。
- 文本：裸 `&str`/`String` 子节点没有 a11y role（实测 title/body 文本不出现在 AX 树里）；`text!(...)` 宏会**按源码位置**生成 id（同一宏调用点循环生成多份会撞 id，需要 `.with_id(i)` / `text(id = i, …)`）；`Text::new(id, styled)` 与 `Text::new_inaccessible(text)` 是显式控制手段。
- 没有“Button 自动响应 Click”的隐式行为（见上）。
- `img` 元素**没有**任何 a11y 支持（img.rs 无 `role`/`a11y` 字样）：冻结画面如果要被读屏描述，必须包一层带 `role` + `aria_label` 的 `div`。

实机观察（`agent-desktop` 1.1.0，macOS 27.2，0.3.11）：

- 树只在辅助技术客户端连接后构建：产品 stderr 出现 `level=INFO target=gpui::window Accessibility activated`。
- 窗口刚出现时首次 `snapshot --app` 可能是空树（`ref_count:0`，只有 `application/dialog` 骨架），隔 ~1.5s 的第二次才有节点 → E2E 必须重读。
- `role(Role::Application)` 的根节点在 macOS 树上被读成 `role:"group"`（外层是 `dialog`/`application`），不要按 role 名做死断言。
- 只有“可交互”的节点会拿到 `ref_id`（button 有；`role:statictext` 的 `capture-status` 没有）。断言“至少一个可寻址节点”时要选按钮/开关类对象。
- 覆盖层（`PopUp` + `titlebar: None`）的 AX 子树可读可驱动：`click @ref` → 产品 `capture_phase from=Frozen to=Resident action=CancelAttempt`。

## 10. 0.3.8 → 0.3.11 的小漂移

本票先在 0.3.8 上打通，再升到最新的 0.3.11。两者之间对产品**没有**编译级差异（同一份代码 0 改动通过），但源码级有四处值得记录：

| 项 | 0.3.8 | 0.3.11 | 级别 |
| --- | --- | --- | --- |
| `WindowOptions` | +`app_owns_titlebar_drag`、`inactive_frame_interval` | 再 +`low_power_frame_interval`、`thermal_frame_interval`、`use_display_p3` | CONFIRMED（源码） |
| `Window::text_system()` | `&Arc<WindowTextSystem>` | `&Rc<WindowTextSystem>` | CONFIRMED（源码） |
| 遗留 `cocoa 0.26` / `block 0.1.6` | 存在（future-incompat 告警） | **移除**；`cargo tree` 与 lock 中均无 | CONFIRMED（`cargo tree -i` + lock + 重新构建无告警） |
| 快照族与依赖规模 | 22 个 crate；596 个解析包、43 个重复包名 | 20 个 crate（去掉 `gpui-pre-perf`/`-util-macros`/`-zlog`，新增 `gpui-pre-ios`）；533 个包、29 个重复包名 | CONFIRMED（lock 审计） |
| `font-kit` 依赖位置 | `gpui-pre` 自身带 macOS 的 `zed-font-kit` 依赖（`gpui-pre` 与 `gpui-pre-macos` 两处定义） | 依赖下移到 `gpui-pre-apple/font-kit`，经 `gpui-pre-platform → gpui-pre-macos → gpui-pre-apple` 转发；`gpui-pre` 自身的 `font-kit` 变空 feature | CONFIRMED（Cargo 元数据） |

## 11. 其他必须一起改的仓库接缝

| 项 | 现象 | 级别 |
| --- | --- | --- |
| `scripts/check-architecture.sh` | 断言 app 依赖图包含字符串 `gpui v`；`gpui-pre` 的 `cargo tree` 名字是 `gpui-pre v0.3.11` → `architecture check failed: expected gpui v in app dependency graph` | CONFIRMED（实测） |
| `scripts/inspect-dependencies.py` | `cargo metadata --locked --offline`；lock 里新增了 macOS 不编译但会被解析的 `gpui-pre-linux/-windows/-web/-wgpu/-ios` 等 → 先 `cargo fetch --locked` 才通过（否则 `failed to download calloop … --offline was specified`） | CONFIRMED（实测） |
| 依赖规模 | 解析包 554 → 533；重复包名 55 → 29；仍存 `objc2 0.5.3`（`accesskit_macos 0.26.3`）与 `objc2 0.6.5`（arcshot-macos）并存 | CONFIRMED（lock + `cargo tree -i`） |
| 新 stderr 噪声 | `sandbox_extension_issue_file_to_process failed for …/ArcShot.app: 1 (Operation not permitted)`（gpui-pre-macos 打出；0.2.2 无） | CONFIRMED（实测 `check-startup.sh`） |
| 链接告警 | `ld: duplicate -rpath '/usr/lib/swift' ignored`（与既有 screencapturekit rpath 叠加） | CONFIRMED（构建输出） |
| 文档/注释 | `TEST.md` §4.2/§4.5 与 AGENTS.md 里“GPUI 0.2.2 不上报 accessibility”的表述在 #52 合入后需要改写（同一票的观察合同部分） | 待 #52 |

## 12. SUSPECTED（未验证，不要当结论）

- IME/组合文字与 AX 的交互：0.3.11 有 `Role::TextInput`/`AXValue` 等，但 ArcShot 目前没有文本控件；#52/#53 必须实机验证“AX 写入/读取不打断 IME”。
- 标注画布的多对象暴露策略（逐对象 `id`+`role` vs `a11y_synthetic_children`）未定，也不知道 synthetic children 在 macOS 上是否稳定。
- 焦点环/键盘遍历（`tab_stop`、`focus_next`）在覆盖层（无标题栏、无窗口焦点环）上的行为未测。
- 覆盖层是否仍被 `CGWindowList` 排除（`layer 101` 那条原型结论）未在 0.3.11 上复测；`list-windows` 为空只能说明“没有可枚举标题窗口”。
- 三个帧间隔默认值（inactive 33.3ms、low-power 33.3ms、thermal 16.7ms）对滚动预览/标注重绘的影响未测。
- 0.3.11 的 `WindowKind::AnchoredPopup` 是否更适合“预览浮在选区内”的浮层（#11/#55 相关）未评估。
- 0.3.9 / 0.3.10 未实测（最新 0.3.11 直接通过，未触发向下回退）；不得据本清单声称它们可用。
- 官方 `gpui` 0.2.2 与 `gpui-pre` 0.3.11 的**渲染**差异（Metal 后端、字体光栅化、阴影/圆角）未做像素对照，本票只关心 a11y 与可运行性。

## 13. 复现本次漂移证据的命令

```sh
git worktree add -b ticket/51-spike-gpui-pre .worktrees/51-spike-gpui-pre spec/gpui-a11y
cd .worktrees/51-spike-gpui-pre
# Cargo.toml: gpui -> { package = "gpui-pre", version = "=0.3.11", … } + gpui-platform =0.3.11
CARGO_TARGET_DIR=/tmp/arcshot-51-target cargo check --workspace
CARGO_TARGET_DIR=/tmp/arcshot-51-target cargo build -p arcshot-app
/tmp/arcshot-51-target/debug/arcshot --check-text-backend
# 取证脚本与完整原始输出见 /tmp/arcshot-spec-notes/51-proof.log
```
