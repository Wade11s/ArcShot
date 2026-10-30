//! GPUI presentation and input; semantic state stays in the capture authority.
use arcshot_app::{CaptureKeyAction, PermissionAction, PermissionNotice, capture_key_action};
use arcshot_core::{CaptureTicket, Hotkey, ScreenGeometry, Selection};
use gpui::{
    Context, FocusHandle, Render, RenderImage, Window, WindowId, div, img, prelude::*, px, rgb,
    rgba,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub(crate) enum UiCommand {
    CancelCapture(CaptureTicket),
    Permission(WindowId, PermissionAction),
    /// Save a recorded hotkey candidate from the settings window.
    HotkeySave(WindowId, Hotkey),
    /// Recording activated or ended in the settings window.
    HotkeyRecording(WindowId, bool),
    /// The hotkey settings window closed.
    HotkeySettingsClosed(WindowId),
    /// A completed pointer drag to commit as the document selection.
    SelectRegion {
        ticket: CaptureTicket,
        from: [f64; 2],
        to: [f64; 2],
    },
    /// Enter, the toolbar button, or a double-click point inside the selection.
    FinishCapture {
        ticket: CaptureTicket,
        point: Option<[f64; 2]>,
    },
}
pub(crate) type UiQueue = Rc<RefCell<Vec<UiCommand>>>;

pub(crate) struct CaptureView {
    pub(crate) ticket: CaptureTicket,
    pub(crate) image: Option<Arc<RenderImage>>,
    /// Authoritative selection mirrored from the capture document.
    pub(crate) selection: Option<Selection>,
    pub(crate) busy: bool,
    pub(crate) error: Option<String>,
    /// Frozen screen geometry; the single source for logical bounds and 2x output.
    pub(crate) screen: Option<ScreenGeometry>,
    /// View-only rubber band; core owns the committed selection.
    pub(crate) drag: Option<([f64; 2], [f64; 2])>,
    pub(crate) focus: FocusHandle,
    pub(crate) queue: UiQueue,
}

fn pointer(event: &gpui::Point<gpui::Pixels>) -> [f64; 2] {
    [f64::from(event.x), f64::from(event.y)]
}

/// Logical rubber-band bounds in points, rejecting a zero-size drag.
fn band(a: [f64; 2], b: [f64; 2]) -> Option<(f32, f32, f32, f32)> {
    let selection = Selection::from_drag(a, b).ok()?;
    Some((
        selection.x() as f32,
        selection.y() as f32,
        selection.width() as f32,
        selection.height() as f32,
    ))
}

fn region(x: f32, y: f32, width: f32, height: f32) -> gpui::Div {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(width))
        .h(px(height))
}

impl CaptureView {
    fn logical_size(&self) -> [f64; 2] {
        self.screen
            .map(|screen| {
                let [_, _, width, height] = screen.source_rect();
                [width, height]
            })
            .unwrap_or([0.0, 0.0])
    }

    fn clamp(&self, point: [f64; 2]) -> [f64; 2] {
        let [width, height] = self.logical_size();
        [point[0].clamp(0.0, width), point[1].clamp(0.0, height)]
    }
}

impl Render for CaptureView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ticket = self.ticket;
        let busy = self.busy;
        let dragging = self.drag.is_some();
        let frozen = self.image.is_some();
        let preview_points = self.drag.or_else(|| {
            self.selection.map(|selection| {
                (
                    [selection.x(), selection.y()],
                    [
                        selection.x() + selection.width(),
                        selection.y() + selection.height(),
                    ],
                )
            })
        });
        let preview = preview_points.and_then(|(start, end)| band(start, end));
        // Label the exact physical crop the shared output conversion would produce.
        let dimensions = preview_points
            .and_then(|(start, end)| Selection::from_drag(start, end).ok())
            .zip(self.screen)
            .and_then(|(selection, screen)| selection.pixel_rect(screen).ok())
            .map(|crop| format!("{} × {}", crop.width(), crop.height()));
        let finish_enabled = frozen && self.selection.is_some() && !busy;
        let error = self.error.clone();
        let image = self
            .image
            .as_ref()
            .map(|image| img(Arc::clone(image)).size_full());

        let mut root = div()
            .id("capture-overlay")
            .size_full()
            .relative()
            .bg(rgb(0x15171b))
            .track_focus(&self.focus)
            .on_key_down({
                let queue = Rc::clone(&self.queue);
                move |event, _, cx| {
                    let action = capture_key_action(&event.keystroke.key, busy, dragging);
                    match action {
                        Some(CaptureKeyAction::Cancel) => {
                            queue.borrow_mut().push(UiCommand::CancelCapture(ticket));
                            cx.stop_propagation();
                        }
                        Some(CaptureKeyAction::Finish) => {
                            queue.borrow_mut().push(UiCommand::FinishCapture {
                                ticket,
                                point: None,
                            });
                            cx.stop_propagation();
                        }
                        None => {}
                    }
                }
            })
            .on_mouse_down(gpui::MouseButton::Left, {
                let entity = cx.entity();
                move |event, _, cx| {
                    let point = pointer(&event.position);
                    let double = event.click_count >= 2;
                    entity.update(cx, |view, cx| {
                        if !frozen || view.busy {
                            return;
                        }
                        if double {
                            // Core checks whether the point is actually inside
                            // the selection; the toolbar is not the background.
                            view.queue.borrow_mut().push(UiCommand::FinishCapture {
                                ticket,
                                point: Some(point),
                            });
                            return;
                        }
                        let point = view.clamp(point);
                        view.drag = Some((point, point));
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            })
            .on_mouse_move({
                let entity = cx.entity();
                move |event, _, cx| {
                    entity.update(cx, |view, cx| {
                        let Some((start, _)) = view.drag else {
                            return;
                        };
                        let point = view.clamp(pointer(&event.position));
                        view.drag = Some((start, point));
                        cx.notify();
                    });
                }
            })
            .on_mouse_up(gpui::MouseButton::Left, {
                let entity = cx.entity();
                let queue = Rc::clone(&self.queue);
                move |event, _, cx| {
                    let end = pointer(&event.position);
                    entity.update(cx, |view, _| {
                        let Some((start, _)) = view.drag.take() else {
                            return;
                        };
                        let end = view.clamp(end);
                        if let Some(_band) = band(start, end) {
                            queue.borrow_mut().push(UiCommand::SelectRegion {
                                ticket,
                                from: start,
                                to: end,
                            });
                        }
                    });
                }
            });

        if let Some(image) = image {
            root = root.child(image);
        }

        if let Some((x, y, width, height)) = preview {
            let [full_width, full_height] = self.logical_size().map(|value| value as f32);
            // Dim only outside the selection; the region itself stays untouched.
            root = root
                .child(region(0.0, 0.0, full_width, y).bg(rgba(0x00000099)))
                .child(region(0.0, y, x, height).bg(rgba(0x00000099)))
                .child(region(x + width, y, full_width - x - width, height).bg(rgba(0x00000099)))
                .child(
                    region(0.0, y + height, full_width, full_height - y - height)
                        .bg(rgba(0x00000099)),
                )
                .child(
                    region(x, y, width, height)
                        .border_1()
                        .border_color(rgb(0xffffff)),
                )
                .children(dimensions.map(|dimensions| {
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px((y - 26.0).max(4.0)))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgba(0x000000cc))
                        .text_color(rgb(0xffffff))
                        .text_sm()
                        .child(dimensions)
                }));
        } else if !frozen {
            root = root.child(
                div()
                    .absolute()
                    .top_4()
                    .left_4()
                    .px_4()
                    .py_2()
                    .rounded_lg()
                    .bg(rgb(0x20242b))
                    .text_color(rgb(0xffffff))
                    .text_sm()
                    .child("正在准备截图… · Esc 取消"),
            );
        }

        if frozen {
            root = root.child(
                div()
                    .absolute()
                    .top_3()
                    .right_3()
                    .flex()
                    .gap_2()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child({
                        let queue = Rc::clone(&self.queue);
                        let mut button = div()
                            .id("finish-capture")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .text_color(rgb(0xffffff))
                            .bg(if finish_enabled {
                                rgb(0x007aff)
                            } else {
                                rgb(0x3b465b)
                            })
                            .child("完成");
                        if finish_enabled {
                            button = button
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(0x0062d6)))
                                .on_click(move |_, _, _| {
                                    queue.borrow_mut().push(UiCommand::FinishCapture {
                                        ticket,
                                        point: None,
                                    });
                                });
                        }
                        button
                    }),
            );
        }

        if let Some(error) = error {
            root = root.child(
                div()
                    .absolute()
                    .left_6()
                    .bottom_6()
                    .px_4()
                    .py_3()
                    .rounded_md()
                    .bg(rgba(0x7a1f1fcc))
                    .text_color(rgb(0xffffff))
                    .text_sm()
                    .child(error),
            );
        } else if frozen && self.selection.is_none() && self.drag.is_none() {
            root = root.child(
                div()
                    .absolute()
                    .left_6()
                    .bottom_6()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgba(0x000000aa))
                    .text_color(rgb(0xffffff))
                    .text_sm()
                    .child("拖动选择区域 · Enter 完成 · Esc 取消"),
            );
        }

        root
    }
}

pub(crate) struct NoticeView {
    pub(crate) notice: Option<PermissionNotice>,
    pub(crate) error: Option<String>,
    pub(crate) focus: FocusHandle,
    pub(crate) queue: UiQueue,
}
fn button(label: &'static str, command: UiCommand, queue: &UiQueue) -> impl IntoElement {
    let queue = Rc::clone(queue);
    div()
        .id(label)
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(0x3b465b))
        .hover(|style| style.bg(rgb(0x52617b)))
        .child(label)
        .on_click(move |_, _, _| queue.borrow_mut().push(command))
}
impl Render for NoticeView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (title, body) = match self.notice {
            Some(PermissionNotice::FirstUse) => (
                "允许 ArcShot 读取屏幕",
                "区域截图只需要屏幕录制权限，用于在内存中冻结内建屏幕。不会要求辅助功能或输入监控权限，也不会自动保存截图。",
            ),
            Some(PermissionNotice::Unavailable) => (
                "屏幕录制权限尚不可用",
                "请在系统设置中检查 ArcShot 的屏幕录制权限，返回后重新检查。若仍不可用，可能需要退出并重新打开 ArcShot；此处不推测具体原因。",
            ),
            Some(PermissionNotice::Available) => (
                "屏幕录制权限可用",
                "可以从菜单栏选择“开始截图”。截图只保存在内存中，Esc 取消并返回驻留。",
            ),
            None => (
                "无法继续截图",
                self.error.as_deref().unwrap_or("请取消后重试。"),
            ),
        };
        let window_id = window.window_handle().window_id();
        let cancel = UiCommand::Permission(window_id, PermissionAction::Cancel);
        let queue = Rc::clone(&self.queue);
        let mut buttons = div().flex().gap_2();
        match self.notice {
            Some(PermissionNotice::FirstUse) => {
                buttons = buttons
                    .child(button(
                        "配置权限",
                        UiCommand::Permission(window_id, PermissionAction::Configure),
                        &self.queue,
                    ))
                    .child(button(
                        "稍后再说",
                        UiCommand::Permission(window_id, PermissionAction::Later),
                        &self.queue,
                    ));
            }
            Some(PermissionNotice::Unavailable) => {
                buttons = buttons
                    .child(button(
                        "打开系统设置",
                        UiCommand::Permission(window_id, PermissionAction::Configure),
                        &self.queue,
                    ))
                    .child(button(
                        "重新检查",
                        UiCommand::Permission(window_id, PermissionAction::Recheck),
                        &self.queue,
                    ))
                    .child(button("取消", cancel, &self.queue));
            }
            _ => buttons = buttons.child(button("关闭", cancel, &self.queue)),
        }
        div()
            .size_full()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .bg(rgb(0x20242b))
            .text_color(rgb(0xffffff))
            .text_size(px(15.0))
            .track_focus(&self.focus)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    queue.borrow_mut().push(cancel);
                    cx.stop_propagation();
                }
            })
            .child(div().text_xl().child(title.to_owned()))
            .child(div().flex_1().child(body.to_owned()))
            .child(buttons)
    }
}

/// The plain key character a keystroke names, or `None` for special keys
/// (enter, function keys, bare modifiers) that cannot form a hotkey.
fn plain_key_char(key: &str) -> Option<char> {
    if key == "space" {
        return Some(' ');
    }
    let mut chars = key.chars();
    let (first, single) = (chars.next()?, chars.next().is_none());
    single.then(|| first.to_lowercase().next().unwrap_or(first))
}

pub(crate) struct HotkeyView {
    pub(crate) current: Hotkey,
    pub(crate) active: bool,
    pub(crate) candidate: Option<Hotkey>,
    pub(crate) recording: bool,
    pub(crate) error: Option<String>,
    pub(crate) focus: FocusHandle,
    pub(crate) queue: UiQueue,
}

impl HotkeyView {
    /// Records one keystroke while the recording area is activated; returns
    /// whether the key was consumed.
    fn record(
        &mut self,
        keystroke: &gpui::Keystroke,
        window_id: WindowId,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.recording {
            return false;
        }
        if keystroke.key == "escape" {
            self.recording = false;
            self.candidate = None;
            self.queue
                .borrow_mut()
                .push(UiCommand::HotkeyRecording(window_id, false));
            cx.notify();
            return true;
        }
        // Modifier-only presses have no plain key; keep waiting for one.
        let Some(key) = plain_key_char(&keystroke.key) else {
            return false;
        };
        let modifiers = &keystroke.modifiers;
        let candidate = Hotkey::new(
            key,
            modifiers.platform,
            modifiers.control,
            modifiers.alt,
            modifiers.shift,
        );
        log::info!(
            "hotkey_recorded keystroke_key={:?} candidate={candidate}",
            keystroke.key
        );
        self.candidate = Some(candidate);
        cx.notify();
        true
    }
}

impl Render for HotkeyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_id = window.window_handle().window_id();
        let entity = cx.entity();
        let current = self.current;
        let active = self.active;
        let candidate = self.candidate;
        let recording = self.recording;
        let error = self.error.clone();
        let save_command = candidate
            .filter(|candidate| candidate.is_valid())
            .map(|candidate| UiCommand::HotkeySave(window_id, candidate));
        div()
            .size_full()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .bg(rgb(0x20242b))
            .text_color(rgb(0xffffff))
            .text_size(px(15.0))
            .track_focus(&self.focus)
            .on_key_down(move |event, _, cx| {
                let consumed = entity.update(cx, |view, cx| {
                    view.record(&event.keystroke, window_id, cx)
                });
                if consumed {
                    cx.stop_propagation();
                }
            })
            .child(div().text_xl().child("截图快捷键"))
            .child(div().child(format!(
                "当前：{current}{}",
                if active { "" } else { "（未生效）" }
            )))
            .child(
                div()
                    .id("hotkey-recorder")
                    .rounded_md()
                    .px_4()
                    .py_4()
                    .cursor_pointer()
                    .bg(if recording {
                        rgb(0x52617b)
                    } else {
                        rgb(0x2a2f3a)
                    })
                    .hover(|style| style.bg(rgb(0x3b465b)))
                    .on_click({
                        let entity = cx.entity();
                        let queue = Rc::clone(&self.queue);
                        move |_, _, cx| {
                            entity.update(cx, |view, cx| {
                                view.recording = true;
                                view.error = None;
                                view.candidate = None;
                                cx.notify();
                            });
                            // The active global hotkey must be suspended
                            // while the recording area receives keys.
                            queue
                                .borrow_mut()
                                .push(UiCommand::HotkeyRecording(window_id, true));
                        }
                    })
                    .child(match (recording, candidate) {
                        (true, Some(candidate)) => format!(
                            "{candidate}{}",
                            if candidate.is_valid() {
                                " · Esc 取消录入"
                            } else {
                                "（不合法）· Esc 取消录入"
                            }
                        ),
                        (true, None) => "按下新组合 · Esc 取消录入".to_owned(),
                        (false, Some(candidate)) => format!(
                            "{candidate}{}",
                            if candidate.is_valid() { "" } else { "（不合法）" }
                        ),
                        (false, None) => "点击录入新组合".to_owned(),
                    }),
            )
            .child(
                div()
                    .text_color(rgb(0x9aa4b5))
                    .child("合法组合需包含 Command / Control / Option 至少一个修饰键和一个普通键，可加 Shift；仅 Shift 加普通键不合法。"),
            )
            .child({
                let mut buttons = div().flex().gap_2();
                if let Some(command) = save_command {
                    let queue = Rc::clone(&self.queue);
                    let entity = cx.entity();
                    buttons = buttons.child(
                        div()
                            .id("hotkey-save")
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(rgb(0x3b465b))
                            .hover(|style| style.bg(rgb(0x52617b)))
                            .child("保存")
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |view, cx| {
                                    view.recording = false;
                                    cx.notify();
                                });
                                log::info!("hotkey_save_pushed window_id={window_id:?}");
                                queue.borrow_mut().push(command);
                            }),
                    );
                }
                buttons.child(button(
                    "关闭",
                    UiCommand::HotkeySettingsClosed(window_id),
                    &self.queue,
                ))
            })
            .children(error.map(|error| {
                div()
                    .id("hotkey-error")
                    .text_color(rgb(0xff6b6b))
                    .child(error)
            }))
    }
}
