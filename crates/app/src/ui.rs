//! GPUI presentation and input; semantic state stays in the capture authority.
use arcshot_app::{
    CaptureKeyAction, CaptureKeyContext, CapturePressAction, KeyModifiers, PermissionAction,
    PermissionNotice, capture_key_action, capture_press_action,
};
use arcshot_core::{
    Annotation, CaptureTicket, Hotkey, PaletteColor, RectangleOption, RectangleStyle, ResizeHandle,
    ScreenGeometry, Selection, StrokeWidth, Tool,
};
use gpui::{
    AccessibleAction, Context, FocusHandle, Render, RenderImage, Role, Toggled, Window, WindowId,
    div, img, prelude::*, px, rgb, rgba, text,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// One transient pointer gesture; the committed document stays authoritative.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DragState {
    /// View-only rubber band for a new selection, committed on release.
    Band { start: [f64; 2], current: [f64; 2] },
    /// Move/resize owned by the document; live preview mirrors its selection.
    Core,
}

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
    /// A press the core resolves to a selection or object gesture.
    PointerPressed {
        ticket: CaptureTicket,
        point: [f64; 2],
    },
    /// Live pointer movement for that core drag.
    PointerMoved {
        ticket: CaptureTicket,
        point: [f64; 2],
    },
    /// Release commits the drag as one history step.
    PointerReleased(CaptureTicket),
    /// The tool dock selected a tool; interaction state, never history.
    SelectTool(CaptureTicket, Tool),
    /// One rectangle option from the hover strip.
    RectangleOption(CaptureTicket, RectangleOption),
    /// Delete the single selected object.
    DeleteObject(CaptureTicket),
    /// Esc layer one: clear the selected object.
    DeselectSelected(CaptureTicket),
    /// Command+Z.
    Undo(CaptureTicket),
    /// Command+Shift+Z.
    Redo(CaptureTicket),
    /// Enter, the toolbar button, or a double-click point inside the selection.
    FinishCapture {
        ticket: CaptureTicket,
        point: Option<[f64; 2]>,
    },
    /// The toolbar Save button: explicit PNG file output for this attempt.
    SaveCapture(CaptureTicket),
}
pub(crate) type UiQueue = Rc<RefCell<Vec<UiCommand>>>;

/// One composed selection preview produced by the shared image rasterizer.
pub(crate) struct OverlayPreview {
    pub(crate) image: Arc<RenderImage>,
    pub(crate) selection: Selection,
}

pub(crate) struct CaptureView {
    pub(crate) ticket: CaptureTicket,
    pub(crate) image: Option<Arc<RenderImage>>,
    /// Authoritative selection mirrored from the capture document.
    pub(crate) selection: Option<Selection>,
    pub(crate) busy: bool,
    pub(crate) error: Option<String>,
    /// Frozen screen geometry; the single source for logical bounds and 2x output.
    pub(crate) screen: Option<ScreenGeometry>,
    /// Transient gesture state; core owns the committed selection and history.
    pub(crate) drag: Option<DragState>,
    /// Mirror of the authoritative active tool.
    pub(crate) tool: Tool,
    /// Mirror of the single selected object index.
    pub(crate) selected: Option<usize>,
    /// Mirror of the layer-ordered objects for hit testing and chrome.
    pub(crate) annotations: Vec<Annotation>,
    /// Mirror of the session-scoped rectangle tool style.
    pub(crate) rectangle_style: RectangleStyle,
    /// Composed selection pixels; the overlay draws only interaction chrome on top.
    pub(crate) preview: Option<OverlayPreview>,
    /// Hovered tool row, including its option strip, for the hover-only options.
    pub(crate) hovered_tool: Option<Tool>,
    pub(crate) focus: FocusHandle,
    pub(crate) queue: UiQueue,
}

const RESIZE_HANDLES: [ResizeHandle; 8] = [
    ResizeHandle::TopLeft,
    ResizeHandle::Top,
    ResizeHandle::TopRight,
    ResizeHandle::Right,
    ResizeHandle::BottomRight,
    ResizeHandle::Bottom,
    ResizeHandle::BottomLeft,
    ResizeHandle::Left,
];

fn key_modifiers(modifiers: &gpui::Modifiers) -> KeyModifiers {
    KeyModifiers {
        command: modifiers.platform,
        shift: modifiers.shift,
        control: modifiers.control,
        option: modifiers.alt,
    }
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

impl CaptureView {
    /// The style the option strip reflects: the selected rectangle, else the tool memory.
    fn effective_style(&self) -> RectangleStyle {
        self.selected
            .and_then(|index| self.annotations.get(index))
            .map_or(self.rectangle_style, |annotation| annotation.style())
    }

    /// One dock row: the tool button plus, while hovered, its option strip.
    ///
    /// The row owns the hover listener so moving from the tool into the strip
    /// keeps it open; leaving the row closes it. Hover never switches tools.
    fn tool_row(
        &self,
        tool: Tool,
        row_id: &'static str,
        id: &'static str,
        label: &'static str,
        glyph: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ticket = self.ticket;
        let queue = Rc::clone(&self.queue);
        let entity = cx.entity();
        let mut row =
            div()
                .id(row_id)
                .flex()
                .flex_row()
                .items_center()
                .on_hover(move |hovered, _, cx| {
                    let hovered = *hovered;
                    entity.update(cx, |view, cx| {
                        if hovered {
                            view.hovered_tool = Some(tool);
                        } else if view.hovered_tool == Some(tool) {
                            view.hovered_tool = None;
                        }
                        cx.notify();
                    });
                });
        row = row.child(tool_button(
            ticket,
            tool,
            id,
            label,
            glyph,
            self.tool == tool,
            &queue,
        ));
        if self.hovered_tool == Some(tool) && tool == Tool::Rectangle {
            row = row.child(self.option_strip(cx));
        }
        row
    }

    fn option_strip(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let ticket = self.ticket;
        let queue = Rc::clone(&self.queue);
        let style = self.effective_style();
        let mut strip = div()
            .id("rectangle-options")
            .role(Role::Group)
            .aria_label("矩形选项")
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .ml_2()
            .px_2()
            .py_1()
            .rounded_lg()
            .bg(rgba(0x20242bee));
        for width in StrokeWidth::ALL {
            let id = match width {
                StrokeWidth::Two => "arcshot.overlay.option.rectangle.width.2",
                StrokeWidth::Three => "arcshot.overlay.option.rectangle.width.3",
                StrokeWidth::Five => "arcshot.overlay.option.rectangle.width.5",
            };
            let label = match width {
                StrokeWidth::Two => "线宽 2 点",
                StrokeWidth::Three => "线宽 3 点",
                StrokeWidth::Five => "线宽 5 点",
            };
            strip = strip.child(option_button(
                id,
                label,
                width.label(),
                style.width() == width,
                None,
                UiCommand::RectangleOption(ticket, RectangleOption::Width(width)),
                &queue,
            ));
        }
        for color in PaletteColor::ALL {
            strip = strip.child(option_button(
                color_id(color),
                color_label(color),
                "",
                style.stroke() == color,
                Some(color.rgb()),
                UiCommand::RectangleOption(ticket, RectangleOption::Stroke(color)),
                &queue,
            ));
        }
        strip.child(option_button(
            "arcshot.overlay.option.rectangle.fill",
            "20% 同色填充",
            "▨",
            style.fill(),
            None,
            UiCommand::RectangleOption(ticket, RectangleOption::Fill(!style.fill())),
            &queue,
        ))
    }
}

const fn color_id(color: PaletteColor) -> &'static str {
    match color {
        PaletteColor::Red => "arcshot.overlay.option.rectangle.color.red",
        PaletteColor::Orange => "arcshot.overlay.option.rectangle.color.orange",
        PaletteColor::Yellow => "arcshot.overlay.option.rectangle.color.yellow",
        PaletteColor::Green => "arcshot.overlay.option.rectangle.color.green",
        PaletteColor::Blue => "arcshot.overlay.option.rectangle.color.blue",
        PaletteColor::White => "arcshot.overlay.option.rectangle.color.white",
        PaletteColor::Black => "arcshot.overlay.option.rectangle.color.black",
    }
}

const fn color_label(color: PaletteColor) -> &'static str {
    match color {
        PaletteColor::Red => "红色 #FF3B30",
        PaletteColor::Orange => "橙色 #FF9500",
        PaletteColor::Yellow => "黄色 #FFCC00",
        PaletteColor::Green => "绿色 #34C759",
        PaletteColor::Blue => "蓝色 #007AFF",
        PaletteColor::White => "白色 #FFFFFF",
        PaletteColor::Black => "黑色 #000000",
    }
}

fn rgb_u32([red, green, blue]: [u8; 3]) -> u32 {
    u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue)
}

#[allow(clippy::too_many_arguments)]
fn option_button(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    selected: bool,
    swatch: Option<[u8; 3]>,
    command: UiCommand,
    queue: &UiQueue,
) -> impl IntoElement {
    let click_queue = Rc::clone(queue);
    let a11y_queue = Rc::clone(queue);
    let background = match swatch {
        Some(channels) => rgb(rgb_u32(channels)),
        None => rgba(0x2a2f3add),
    };
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(selected)
        .aria_toggled(if selected {
            Toggled::True
        } else {
            Toggled::False
        })
        .accessibility_id(id)
        .tooltip(move |_, cx| cx.new(|_| TooltipView { label }).into())
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .border_2()
        .border_color(if selected {
            rgb(0xffffff)
        } else {
            rgba(0xffffff33)
        })
        .bg(background)
        .text_color(rgb(0xffffff))
        .text_sm()
        .child(glyph)
        .on_click(move |_, _, _| click_queue.borrow_mut().push(command))
        .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
            log::info!("a11y_action={id} action=Click");
            a11y_queue.borrow_mut().push(command);
        })
}

fn tool_button(
    ticket: CaptureTicket,
    tool: Tool,
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    active: bool,
    queue: &UiQueue,
) -> impl IntoElement {
    let click_queue = Rc::clone(queue);
    let a11y_queue = Rc::clone(queue);
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(active)
        .aria_toggled(if active {
            Toggled::True
        } else {
            Toggled::False
        })
        .accessibility_id(id)
        .tooltip(move |_, cx| cx.new(|_| TooltipView { label }).into())
        .w(px(36.0))
        .h(px(36.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .bg(if active {
            rgb(0x007aff)
        } else {
            rgba(0x20242bdd)
        })
        .text_color(rgb(0xffffff))
        .text_xl()
        .hover(move |style| style.bg(if active { rgb(0x0062d6) } else { rgb(0x2a2f3a) }))
        .child(glyph)
        .on_click(move |_, _, _| {
            click_queue
                .borrow_mut()
                .push(UiCommand::SelectTool(ticket, tool));
        })
        .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
            log::info!("a11y_action={id} action=Click");
            a11y_queue
                .borrow_mut()
                .push(UiCommand::SelectTool(ticket, tool));
        })
}

fn delete_button(ticket: CaptureTicket, queue: &UiQueue) -> impl IntoElement {
    let click_queue = Rc::clone(queue);
    let a11y_queue = Rc::clone(queue);
    div()
        .id("delete-object")
        .role(Role::Button)
        .aria_label("删除选中的标注对象")
        .accessibility_id("arcshot.overlay.delete")
        .tooltip(|_, cx| cx.new(|_| TooltipView { label: "删除" }).into())
        .px_2()
        .py_1()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(0x7a1f1f))
        .text_color(rgb(0xffffff))
        .text_sm()
        .hover(|style| style.bg(rgb(0x9a2a2a)))
        .child("删除")
        .on_click(move |_, _, _| {
            click_queue
                .borrow_mut()
                .push(UiCommand::DeleteObject(ticket));
        })
        .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
            log::info!("a11y_action=arcshot.overlay.delete action=Click");
            a11y_queue
                .borrow_mut()
                .push(UiCommand::DeleteObject(ticket));
        })
}

struct TooltipView {
    label: &'static str,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgba(0x000000ee))
            .text_color(rgb(0xffffff))
            .text_sm()
            .child(self.label)
    }
}

impl Render for CaptureView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ticket = self.ticket;
        let busy = self.busy;
        let frozen = self.image.is_some();
        let preview_points = match self.drag {
            Some(DragState::Band { start, current }) => Some((start, current)),
            _ => self.selection.map(|selection| {
                (
                    [selection.x(), selection.y()],
                    [
                        selection.x() + selection.width(),
                        selection.y() + selection.height(),
                    ],
                )
            }),
        };
        let preview = preview_points.and_then(|(start, end)| band(start, end));
        // Label the exact physical crop the shared output conversion would produce.
        let dimensions = preview_points
            .and_then(|(start, end)| Selection::from_drag(start, end).ok())
            .zip(self.screen)
            .and_then(|(selection, screen)| selection.pixel_rect(screen).ok())
            .map(|crop| format!("{} × {}", crop.width(), crop.height()));
        let finish_enabled = frozen && self.selection.is_some() && !busy;
        let save_enabled = finish_enabled;
        let show_handles = frozen
            && !busy
            && self.selection.is_some()
            && !matches!(self.drag, Some(DragState::Band { .. }));
        let error = self.error.clone();
        let image = self
            .image
            .as_ref()
            .map(|image| img(Arc::clone(image)).size_full());

        let mut root = div()
            .id("capture-overlay")
            .role(Role::Group)
            .aria_label("ArcShot 截图覆盖层")
            .size_full()
            .relative()
            .bg(rgb(0x15171b))
            .track_focus(&self.focus)
            .on_key_down({
                let queue = Rc::clone(&self.queue);
                let entity = cx.entity();
                move |event, _, cx| {
                    // Read the live drag flag rather than the render-time snapshot,
                    // so a key right after release never sees a stale held state.
                    let context = entity.read_with(cx, |view, _| CaptureKeyContext {
                        busy: view.busy,
                        dragging: view.drag.is_some(),
                        object_selected: view.selected.is_some(),
                        tool: view.tool,
                    });
                    let action = capture_key_action(
                        &event.keystroke.key,
                        key_modifiers(&event.keystroke.modifiers),
                        context,
                    );
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
                        Some(CaptureKeyAction::Undo) => {
                            queue.borrow_mut().push(UiCommand::Undo(ticket));
                            cx.stop_propagation();
                        }
                        Some(CaptureKeyAction::Redo) => {
                            queue.borrow_mut().push(UiCommand::Redo(ticket));
                            cx.stop_propagation();
                        }
                        Some(CaptureKeyAction::Deselect) => {
                            queue.borrow_mut().push(UiCommand::DeselectSelected(ticket));
                            cx.stop_propagation();
                        }
                        Some(CaptureKeyAction::SelectTool) => {
                            queue
                                .borrow_mut()
                                .push(UiCommand::SelectTool(ticket, Tool::Select));
                            cx.stop_propagation();
                        }
                        Some(CaptureKeyAction::DeleteObject) => {
                            queue.borrow_mut().push(UiCommand::DeleteObject(ticket));
                            cx.stop_propagation();
                        }
                        None => {}
                    }
                }
            })
            .on_mouse_down(gpui::MouseButton::Left, {
                let entity = cx.entity();
                move |event, _, cx| {
                    let raw = pointer(&event.position);
                    let double = event.click_count >= 2;
                    entity.update(cx, |view, cx| {
                        if !frozen || view.busy {
                            return;
                        }
                        // Only the visible part of an object is interactive.
                        let object = view
                            .selection
                            .filter(|selection| selection.contains(raw))
                            .and_then(|_| arcshot_core::annotation_at(&view.annotations, raw));
                        match capture_press_action(
                            double,
                            view.busy,
                            view.tool,
                            view.selection,
                            object.is_some(),
                            raw,
                        ) {
                            // Core checks whether the point is actually inside
                            // the selection; the toolbar is not the background.
                            Some(CapturePressAction::FinishAt(point)) => {
                                view.queue.borrow_mut().push(UiCommand::FinishCapture {
                                    ticket,
                                    point: Some(point),
                                });
                            }
                            Some(CapturePressAction::Create) => {
                                // A new object starts unselected; the draft is
                                // selected as soon as the first move lands.
                                view.selected = None;
                                let point = view.clamp(raw);
                                view.drag = Some(DragState::Core);
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerPressed { ticket, point });
                            }
                            Some(CapturePressAction::ObjectDrag) => {
                                // Mirror the picked layer immediately so keys
                                // pressed before the next tick see the selection.
                                view.selected = object;
                                let point = view.clamp(raw);
                                view.drag = Some(DragState::Core);
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerPressed { ticket, point });
                            }
                            Some(CapturePressAction::Drag) => {
                                view.selected = None;
                                let point = view.clamp(raw);
                                view.drag = Some(DragState::Core);
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerPressed { ticket, point });
                            }
                            Some(CapturePressAction::Band) => {
                                view.selected = None;
                                let point = view.clamp(raw);
                                view.drag = Some(DragState::Band {
                                    start: point,
                                    current: point,
                                });
                            }
                            None => {}
                        }
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            })
            .on_mouse_move({
                let entity = cx.entity();
                move |event, _, cx| {
                    entity.update(cx, |view, cx| {
                        let point = view.clamp(pointer(&event.position));
                        match &mut view.drag {
                            Some(DragState::Band { current, .. }) => {
                                *current = point;
                                // Everything visual reads the view's band; the
                                // authoritative document changes only on release.
                                cx.notify();
                            }
                            Some(DragState::Core) => {
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerMoved { ticket, point });
                            }
                            None => {}
                        }
                    });
                }
            })
            .on_mouse_up(gpui::MouseButton::Left, {
                let entity = cx.entity();
                move |event, _, cx| {
                    let end = pointer(&event.position);
                    entity.update(cx, |view, cx| {
                        let Some(drag) = view.drag.take() else {
                            return;
                        };
                        match drag {
                            DragState::Band { start, .. } => {
                                let end = view.clamp(end);
                                if band(start, end).is_some() {
                                    view.queue.borrow_mut().push(UiCommand::SelectRegion {
                                        ticket,
                                        from: start,
                                        to: end,
                                    });
                                }
                            }
                            DragState::Core => {
                                // Apply the release position before committing, so a
                                // final mouse-up without a move event is not lost.
                                let point = view.clamp(end);
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerMoved { ticket, point });
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerReleased(ticket));
                            }
                        }
                        cx.notify();
                    });
                }
            });

        if let Some(image) = image {
            root = root.child(image);
        }

        // Annotation pixels come only from the shared image rasterizer; the
        // composed crop is shown in place of the base inside the selection.
        // A new-selection band still shows the old document, so hide it then.
        if let (Some(preview), Some(selection)) = (&self.preview, self.selection)
            && preview.selection == selection
            && !matches!(self.drag, Some(DragState::Band { .. }))
        {
            root = root.child(
                img(Arc::clone(&preview.image))
                    .absolute()
                    .left(px(selection.x() as f32))
                    .top(px(selection.y() as f32))
                    .w(px(selection.width() as f32))
                    .h(px(selection.height() as f32))
                    .object_fit(gpui::ObjectFit::Fill),
            );
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

            // Eight control points are the visible move/resize affordances; the
            // same logical centers drive the core hit test.
            if show_handles && let Some(selection) = self.selection {
                for handle in RESIZE_HANDLES {
                    let [x, y] = selection.handle_center(handle).map(|value| value as f32);
                    root = root.child(
                        region(x - 4.0, y - 4.0, 8.0, 8.0)
                            .bg(rgb(0x007aff))
                            .border_1()
                            .border_color(rgb(0xffffff)),
                    );
                }
            }

            // The selected object's own eight control points use its stroke
            // colour, so object chrome is distinguishable from selection chrome.
            if show_handles
                && let Some(annotation) = self.selected.and_then(|i| self.annotations.get(i))
            {
                let [red, green, blue] = annotation.style().stroke().rgb();
                let color = rgb(u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue));
                for handle in RESIZE_HANDLES {
                    let [x, y] = annotation.handle_center(handle).map(|value| value as f32);
                    root = root.child(
                        region(x - 4.0, y - 4.0, 8.0, 8.0)
                            .bg(color)
                            .border_1()
                            .border_color(rgb(0xffffff)),
                    );
                }
            }
        }

        // The hint doubles as the accessible cancel affordance: it is the only
        // action a Frozen session offers before a selection exists.
        root = root.child({
            let a11y_queue = Rc::clone(&self.queue);
            let click_queue = Rc::clone(&self.queue);
            let status = if frozen {
                "画面已冻结 · Esc 取消"
            } else {
                "正在准备截图… · Esc 取消"
            };
            div()
                .id("capture-badge")
                .role(Role::Button)
                .aria_label(status)
                .accessibility_id("arcshot.overlay.cancel")
                .absolute()
                .top_4()
                .left_4()
                .px_4()
                .py_2()
                .rounded_lg()
                .bg(rgb(0x20242b))
                .text_color(rgb(0xffffff))
                .text_sm()
                .hover(|style| style.bg(rgb(0x2a2f3a)))
                .child(status)
                .on_click(move |_, _, _| {
                    click_queue
                        .borrow_mut()
                        .push(UiCommand::CancelCapture(ticket));
                })
                // AXPress never reaches `on_click`, so the accessible path
                // enqueues the same command and reports itself for E2E checks.
                .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
                    log::info!("a11y_action=arcshot.overlay.cancel action=Click");
                    a11y_queue
                        .borrow_mut()
                        .push(UiCommand::CancelCapture(ticket));
                })
        });

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
                        let a11y_queue = Rc::clone(&self.queue);
                        let mut button = div()
                            .id("save-capture")
                            .aria_label("保存 PNG")
                            .accessibility_id("arcshot.overlay.save")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .text_color(rgb(0xffffff))
                            .bg(if save_enabled {
                                rgb(0x3b465b)
                            } else {
                                rgb(0x2a2f3a)
                            })
                            .child("保存");
                        if save_enabled {
                            button = button
                                .role(Role::Button)
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(0x52617b)))
                                .on_click(move |_, _, _| {
                                    queue.borrow_mut().push(UiCommand::SaveCapture(ticket));
                                })
                                .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
                                    log::info!("a11y_action=arcshot.overlay.save action=Click");
                                    a11y_queue.borrow_mut().push(UiCommand::SaveCapture(ticket));
                                });
                        }
                        button
                    })
                    .child({
                        let queue = Rc::clone(&self.queue);
                        let a11y_queue = Rc::clone(&self.queue);
                        let mut button = div()
                            .id("finish-capture")
                            .aria_label("完成")
                            .accessibility_id("arcshot.overlay.finish")
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
                                .role(Role::Button)
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(0x0062d6)))
                                .on_click(move |_, _, _| {
                                    queue.borrow_mut().push(UiCommand::FinishCapture {
                                        ticket,
                                        point: None,
                                    });
                                })
                                .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
                                    log::info!("a11y_action=arcshot.overlay.finish action=Click");
                                    a11y_queue.borrow_mut().push(UiCommand::FinishCapture {
                                        ticket,
                                        point: None,
                                    });
                                });
                        }
                        button
                    }),
            );
        }

        if frozen {
            // The left vertical dock lists only tools that exist: no fake or
            // disabled entries. Hover reveals that tool's options to its right.
            let mut dock = div()
                .id("tool-dock")
                .role(Role::Group)
                .aria_label("标注工具")
                .absolute()
                .left_2()
                .top_16()
                .flex()
                .flex_col()
                .gap_2()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
            dock = dock.child(self.tool_row(
                Tool::Select,
                "select-tool-row",
                "arcshot.overlay.tool.select",
                "选择",
                "⌖",
                cx,
            ));
            dock = dock.child(self.tool_row(
                Tool::Rectangle,
                "rectangle-tool-row",
                "arcshot.overlay.tool.rectangle",
                "矩形",
                "▭",
                cx,
            ));
            if self.selected.is_some() && !busy {
                dock = dock.child(delete_button(ticket, &self.queue));
            }
            root = root.child(dock);
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
fn button(
    a11y_id: &'static str,
    label: &'static str,
    command: UiCommand,
    queue: &UiQueue,
) -> impl IntoElement {
    let click_queue = Rc::clone(queue);
    let a11y_queue = Rc::clone(queue);
    div()
        .id(label)
        .role(Role::Button)
        .aria_label(label)
        .accessibility_id(a11y_id)
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(0x3b465b))
        .hover(|style| style.bg(rgb(0x52617b)))
        .child(label)
        .on_click(move |_, _, _| click_queue.borrow_mut().push(command))
        // AXPress never reaches `on_click`, so the accessible path enqueues the
        // same command explicitly and reports itself for end-to-end checks.
        .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
            log::info!("a11y_action={a11y_id} action=Click");
            a11y_queue.borrow_mut().push(command);
        })
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
                        "arcshot.notice.configure",
                        "配置权限",
                        UiCommand::Permission(window_id, PermissionAction::Configure),
                        &self.queue,
                    ))
                    .child(button(
                        "arcshot.notice.later",
                        "稍后再说",
                        UiCommand::Permission(window_id, PermissionAction::Later),
                        &self.queue,
                    ));
            }
            Some(PermissionNotice::Unavailable) => {
                buttons = buttons
                    .child(button(
                        "arcshot.notice.settings",
                        "打开系统设置",
                        UiCommand::Permission(window_id, PermissionAction::Configure),
                        &self.queue,
                    ))
                    .child(button(
                        "arcshot.notice.recheck",
                        "重新检查",
                        UiCommand::Permission(window_id, PermissionAction::Recheck),
                        &self.queue,
                    ))
                    .child(button("arcshot.notice.cancel", "取消", cancel, &self.queue));
            }
            _ => {
                buttons = buttons.child(button("arcshot.notice.close", "关闭", cancel, &self.queue))
            }
        }
        div()
            .id("notice-root")
            .size_full()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .bg(rgb(0x20242b))
            .text_color(rgb(0xffffff))
            .text_size(px(15.0))
            .role(Role::Group)
            .aria_label("ArcShot 权限提示")
            .track_focus(&self.focus)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    queue.borrow_mut().push(cancel);
                    cx.stop_propagation();
                }
            })
            .child(div().text_xl().child(text!(id = "notice-title", title)))
            .child(div().flex_1().child(text!(id = "notice-body", body)))
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
        let key_entity = entity.clone();
        let current = self.current;
        let active = self.active;
        let candidate = self.candidate;
        let recording = self.recording;
        let error = self.error.clone();
        let save_command = candidate
            .filter(|candidate| candidate.is_valid())
            .map(|candidate| UiCommand::HotkeySave(window_id, candidate));
        // One source for the visible text and the accessible name, so assistive
        // technology reads the same recording state a pointer user sees.
        let recorder_label = match (recording, candidate) {
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
                if candidate.is_valid() {
                    ""
                } else {
                    "（不合法）"
                }
            ),
            (false, None) => "点击录入新组合".to_owned(),
        };
        div()
            .id("hotkey-root")
            .size_full()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .bg(rgb(0x20242b))
            .text_color(rgb(0xffffff))
            .text_size(px(15.0))
            .role(Role::Group)
            .aria_label("ArcShot 截图快捷键")
            .track_focus(&self.focus)
            .on_key_down(move |event, _, cx| {
                let consumed = key_entity.update(cx, |view, cx| {
                    view.record(&event.keystroke, window_id, cx)
                });
                if consumed {
                    cx.stop_propagation();
                }
            })
            .child(div().text_xl().child(text!(id = "hotkey-title", "截图快捷键")))
            .child(div().child(text!(
                id = "hotkey-current",
                format!("当前：{current}{}", if active { "" } else { "（未生效）" })
            )))
            .child(
                div()
                    .id("hotkey-recorder")
                    .role(Role::Button)
                    .aria_label(recorder_label.clone())
                    .accessibility_id("arcshot.hotkey.recorder")
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
                    // AX activation must reach the same state change and queue
                    // push as a pointer click; `AXPress` never calls `on_click`.
                    .on_a11y_action(AccessibleAction::Click, {
                        let entity = entity.clone();
                        let queue = Rc::clone(&self.queue);
                        move |_, _, cx| {
                            log::info!("a11y_action=arcshot.hotkey.recorder action=Click");
                            entity.update(cx, |view, cx| {
                                view.recording = true;
                                view.error = None;
                                view.candidate = None;
                                cx.notify();
                            });
                            queue
                                .borrow_mut()
                                .push(UiCommand::HotkeyRecording(window_id, true));
                        }
                    })
                    .child(recorder_label),
            )
            .child(
                div().text_color(rgb(0x9aa4b5)).child(text!(
                    id = "hotkey-rule",
                    "合法组合需包含 Command / Control / Option 至少一个修饰键和一个普通键，可加 Shift；仅 Shift 加普通键不合法。"
                )),
            )
            .child({
                let mut buttons = div().flex().gap_2();
                if let Some(command) = save_command {
                    let queue = Rc::clone(&self.queue);
                    let entity = cx.entity();
                    let a11y_entity = entity.clone();
                    let a11y_queue = Rc::clone(&self.queue);
                    buttons = buttons.child(
                        div()
                            .id("hotkey-save")
                            .role(Role::Button)
                            .aria_label("保存")
                            .accessibility_id("arcshot.hotkey.save")
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
                            })
                            .on_a11y_action(AccessibleAction::Click, move |_, _, cx| {
                                log::info!("a11y_action=arcshot.hotkey.save action=Click");
                                a11y_entity.update(cx, |view, cx| {
                                    view.recording = false;
                                    cx.notify();
                                });
                                a11y_queue.borrow_mut().push(command);
                            }),
                    );
                }
                buttons.child(button(
                    "arcshot.hotkey.close",
                    "关闭",
                    UiCommand::HotkeySettingsClosed(window_id),
                    &self.queue,
                ))
            })
            .children(error.map(|error| {
                div()
                    .id("hotkey-error")
                    .text_color(rgb(0xff6b6b))
                    .child(text!(id = "hotkey-error-text", error))
            }))
    }
}
