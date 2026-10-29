//! GPUI presentation and input; semantic state stays in the capture authority.
use arcshot_app::{PermissionAction, PermissionNotice};
use gpui::{Context, FocusHandle, Render, RenderImage, Window, div, img, prelude::*, px, rgb};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub(crate) enum UiCommand {
    CancelCapture,
    Permission(PermissionAction),
}
pub(crate) type UiQueue = Rc<RefCell<Vec<UiCommand>>>;

pub(crate) struct CaptureView {
    pub(crate) image: Option<Arc<RenderImage>>,
    pub(crate) focus: FocusHandle,
    pub(crate) queue: UiQueue,
}
impl Render for CaptureView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let queue = Rc::clone(&self.queue);
        div()
            .size_full()
            .relative()
            .bg(rgb(0x15171b))
            .track_focus(&self.focus)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    queue.borrow_mut().push(UiCommand::CancelCapture);
                    cx.stop_propagation();
                }
            })
            .children(
                self.image
                    .as_ref()
                    .map(|image| img(Arc::clone(image)).size_full()),
            )
            .child(
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
                    .child(if self.image.is_some() {
                        "画面已冻结 · Esc 取消（本阶段不输出）"
                    } else {
                        "正在准备截图… · Esc 取消"
                    }),
            )
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
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
        let cancel = UiCommand::Permission(PermissionAction::Cancel);
        let queue = Rc::clone(&self.queue);
        let mut buttons = div().flex().gap_2();
        match self.notice {
            Some(PermissionNotice::FirstUse) => {
                buttons = buttons
                    .child(button(
                        "配置权限",
                        UiCommand::Permission(PermissionAction::Configure),
                        &self.queue,
                    ))
                    .child(button(
                        "稍后再说",
                        UiCommand::Permission(PermissionAction::Later),
                        &self.queue,
                    ));
            }
            Some(PermissionNotice::Unavailable) => {
                buttons = buttons
                    .child(button(
                        "打开系统设置",
                        UiCommand::Permission(PermissionAction::Configure),
                        &self.queue,
                    ))
                    .child(button(
                        "重新检查",
                        UiCommand::Permission(PermissionAction::Recheck),
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
