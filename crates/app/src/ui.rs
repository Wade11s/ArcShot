//! GPUI presentation and input; semantic state stays in the capture authority.
use crate::a11y_gestures::{
    A11yAffordance, A11yAnchor, GestureContext, affordance_commands, annotation_corners,
    endpoint_name, format_corners, format_point, handle_name,
};
use crate::text_input::{TextInput, text_editor};
use arcshot_app::{
    CaptureKeyAction, CaptureKeyContext, CapturePressAction, KeyModifiers, PARTIAL_ABANDON_LABEL,
    PARTIAL_ACCEPT_LABEL, PermissionAction, PermissionNotice, ScrollPermissionAction,
    ScrollPermissionNotice, capture_key_action, capture_long_image_press_action,
    capture_press_action, capture_text_press_action,
};
use arcshot_core::{
    Annotation, AnnotationStyle, ArrowOption, ArrowStyle, BlurLevel, BlurOption, BlurStyle,
    CaptureTicket, Density, Endpoint, Hotkey, LineOption, LineStyle, PaletteColor, RectangleOption,
    RectangleStyle, ResizeHandle, ScreenGeometry, Selection, Shape, StrokeWidth, TextOption,
    TextStyle, Tool,
};
use gpui::{
    AccessibleAction, Animation, AnimationExt, Context, Entity, FocusHandle, Render, RenderImage,
    Role, Toggled, Window, WindowId, div, img, prelude::*, px, rgb, rgba, text,
};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

/// One transient pointer gesture; the committed document stays authoritative.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DragState {
    /// View-only rubber band for a new selection, committed on release.
    Band { start: [f64; 2], current: [f64; 2] },
    /// Move/resize owned by the document; live preview mirrors its selection.
    Core,
}

/// Slide-in duration of the completion thumbnail (I-11 §7).
pub(crate) const COMPLETION_PREVIEW_ENTER: Duration = Duration::from_millis(200);
/// How long the thumbnail stays before leaving; I-11 §7 fixes this at 3 seconds.
pub(crate) const COMPLETION_PREVIEW_HOLD: Duration = Duration::from_secs(3);
/// Slide-out and fade duration of the completion thumbnail.
pub(crate) const COMPLETION_PREVIEW_EXIT: Duration = Duration::from_millis(250);

/// Display transform for the whole-image annotation surface.
///
/// `screen = image * zoom + offset`; it never feeds back into the document crop
/// or annotation coordinates, so panning and zooming cannot change the output.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ViewTransform {
    pub(crate) zoom: f64,
    pub(crate) offset: [f64; 2],
}

impl ViewTransform {
    fn to_image(self, point: [f64; 2]) -> [f64; 2] {
        [
            (point[0] - self.offset[0]) / self.zoom,
            (point[1] - self.offset[1]) / self.zoom,
        ]
    }

    fn to_screen(self, point: [f64; 2]) -> [f64; 2] {
        [
            point[0] * self.zoom + self.offset[0],
            point[1] * self.zoom + self.offset[1],
        ]
    }

    fn rect(self, selection: Selection) -> [f64; 4] {
        [
            selection.x() * self.zoom + self.offset[0],
            selection.y() * self.zoom + self.offset[1],
            selection.width() * self.zoom,
            selection.height() * self.zoom,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
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
    /// One line option from the hover strip.
    LineOption(CaptureTicket, LineOption),
    /// One arrow option from the hover strip.
    ArrowOption(CaptureTicket, ArrowOption),
    /// One blur level from the hover strip.
    BlurOption(CaptureTicket, BlurOption),
    /// One text colour or size option from the hover strip.
    TextOption(CaptureTicket, TextOption),
    /// Start a text edit at a point (new object or re-edit).
    BeginText {
        ticket: CaptureTicket,
        point: [f64; 2],
    },
    /// The live editor content and its measured bounds changed.
    TextEdited {
        ticket: CaptureTicket,
        content: String,
        bounds: Selection,
    },
    /// Commit the in-progress text edit as one history step.
    TextCommit(CaptureTicket),
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
    /// The toolbar Scroll button: enter Scroll preparation from this selection.
    EnterScroll(CaptureTicket),
    /// One Scroll permission notice action.
    ScrollPermission(WindowId, ScrollPermissionAction),
    /// The Scroll control window requests cancel.
    ScrollCancel(CaptureTicket),
    /// The Scroll control window closed.
    ScrollControlClosed(WindowId, CaptureTicket),
    /// The completion preview was activated: enter the shared document.
    EnterCompletionAnnotation(CaptureTicket),
    /// The accessible non-drag path adds one Rectangle to the completion surface.
    AddDefaultRectangle(CaptureTicket),
    /// The user explicitly accepts the verified partial result.
    ScrollPartialAccept(CaptureTicket),
    /// The user explicitly abandons the verified partial result.
    ScrollPartialAbandon(CaptureTicket),
}
pub(crate) type UiQueue = Rc<RefCell<Vec<UiCommand>>>;

/// One composed selection preview produced by the shared image rasterizer.
pub(crate) struct OverlayPreview {
    pub(crate) image: Arc<RenderImage>,
    pub(crate) selection: Selection,
}

/// A mirror of the authoritative in-progress text edit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextEditMirror {
    pub(crate) anchor: [f64; 2],
    pub(crate) content: String,
    pub(crate) bounds: Selection,
    pub(crate) style: TextStyle,
    pub(crate) target: Option<usize>,
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
    /// Mirror of the session-scoped line tool style.
    pub(crate) line_style: LineStyle,
    /// Mirror of the session-scoped arrow tool style.
    pub(crate) arrow_style: ArrowStyle,
    /// Mirror of the session-scoped blur tool style.
    pub(crate) blur_style: BlurStyle,
    /// Mirror of the session-scoped text tool style.
    pub(crate) text_style: TextStyle,
    /// Mirror of the authoritative in-progress text edit.
    pub(crate) text_edit: Option<TextEditMirror>,
    /// The live IME-capable editor entity, present only while editing.
    pub(crate) text_input: Option<Entity<TextInput>>,
    /// Focus handle of the live text editor.
    pub(crate) text_focus: Option<FocusHandle>,
    /// Whether the size field is accepting direct keyboard input.
    pub(crate) size_editing: bool,
    /// The raw size draft while direct input is active.
    pub(crate) size_draft: String,
    /// Composed selection pixels; the overlay draws only interaction chrome on top.
    pub(crate) preview: Option<OverlayPreview>,
    /// Hovered tool row, including its option strip, for the hover-only options.
    pub(crate) hovered_tool: Option<Tool>,
    /// Mirror of the authoritative option density from the capture session.
    pub(crate) density: Density,
    /// Whether this surface shows the whole completed long image. Selection
    /// gestures are disabled in that mode; only annotations are editable.
    pub(crate) long_image: bool,
    /// Whole-image view zoom; ignored unless `long_image` is set.
    pub(crate) view_zoom: f64,
    /// Whole-image view offset in window points; ignored unless `long_image`.
    pub(crate) view_offset: [f64; 2],
    /// Window viewport in logical points, used to keep the view clamped.
    pub(crate) viewport: [f64; 2],
    pub(crate) focus: FocusHandle,
    pub(crate) queue: UiQueue,
}

/// I-09 layout numbers are a product choice, not a spec-mandated pixel value.
///
/// Both densities expose exactly the same controls: the same tool buttons, the
/// same option ids, tooltips, accessible names and selected state. Only icon
/// size, spacing and the optional group heading differ. The dock stays anchored
/// at a fixed screen position and the top-right finish/save entries do not
/// change with density, so neither shrinks out of reach at 1470×956 pt or at a
/// small selection.
#[derive(Clone, Copy)]
struct DockMetrics {
    tool_side: f32,
    tool_text: f32,
    option_side: f32,
    option_text: f32,
    dock_gap: f32,
    strip_gap: f32,
    strip_pad_x: f32,
    strip_pad_y: f32,
    heading_text: f32,
    show_heading: bool,
}

impl DockMetrics {
    const fn for_density(density: Density) -> Self {
        match density {
            Density::Compact => Self {
                tool_side: 32.0,
                tool_text: 16.0,
                option_side: 24.0,
                option_text: 12.0,
                dock_gap: 4.0,
                strip_gap: 2.0,
                strip_pad_x: 4.0,
                strip_pad_y: 2.0,
                heading_text: 10.0,
                show_heading: false,
            },
            Density::Standard => Self {
                tool_side: 44.0,
                tool_text: 22.0,
                option_side: 32.0,
                option_text: 15.0,
                dock_gap: 10.0,
                strip_gap: 6.0,
                strip_pad_x: 10.0,
                strip_pad_y: 8.0,
                heading_text: 14.0,
                show_heading: true,
            },
        }
    }
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

/// The blur tool's real tooltip: the effect is not a redaction guarantee.
const BLUR_TOOLTIP: &str =
    "模糊矩形：对选区内的画面做高斯模糊。模糊不保证敏感信息不可读，也不是可靠的信息遮盖方式。";

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
    /// The live mirrors an AX gesture is resolved against.
    ///
    /// Resolved per activation, never captured at render time, so a stale
    /// snapshot cannot make a gesture act on geometry the document no longer has.
    fn gesture_context(&self) -> GestureContext<'_> {
        GestureContext {
            busy: self.busy,
            long_image: self.long_image,
            tool: self.tool,
            selection: self.selection,
            annotations: &self.annotations,
            text_editing: self.text_edit.is_some(),
        }
    }

    /// The selection's two corners as the canonical AX value form.
    fn selection_value(&self) -> String {
        self.selection.map_or_else(
            || "none".to_owned(),
            |selection| {
                format_corners(
                    [selection.x(), selection.y()],
                    [
                        selection.x() + selection.width(),
                        selection.y() + selection.height(),
                    ],
                )
            },
        )
    }

    /// One annotation's two corners in document coordinates as AX value text.
    fn object_value(annotation: &Annotation) -> String {
        annotation_corners(annotation)
            .map_or_else(|| "none".to_owned(), |(a, b)| format_corners(a, b))
    }

    /// The window-space rect an annotation's AX node occupies.
    ///
    /// The node is invisible; it exists so assistive technology can address the
    /// object where it is drawn. A degenerate extent is widened to one point so
    /// the node stays addressable.
    fn object_screen_rect(&self, annotation: &Annotation) -> Option<[f32; 4]> {
        let (a, b) = annotation_corners(annotation)?;
        let a = self.screen_point(a);
        let b = self.screen_point(b);
        let x = a[0].min(b[0]) as f32;
        let y = a[1].min(b[1]) as f32;
        let width = (a[0] - b[0]).abs() as f32;
        let height = (a[1] - b[1]).abs() as f32;
        Some([x, y, width.max(1.0), height.max(1.0)])
    }

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

    /// The whole-image view transform, or `None` for the region surface.
    fn view_transform(&self) -> Option<ViewTransform> {
        self.long_image.then_some(ViewTransform {
            zoom: self.view_zoom,
            offset: self.view_offset,
        })
    }

    /// Maps a window point into document coordinates.
    ///
    /// Region mode keeps the existing screen clamp. Whole-image mode applies
    /// the inverse view transform and clamps to the image bounds, so panning
    /// and zooming never extend the editable area past the document.
    fn view_point(&self, raw: [f64; 2]) -> [f64; 2] {
        let Some(transform) = self.view_transform() else {
            return self.clamp(raw);
        };
        let [width, height] = self.logical_size();
        let image = transform.to_image(raw);
        [image[0].clamp(0.0, width), image[1].clamp(0.0, height)]
    }

    /// Maps one selection rectangle into window coordinates.
    fn screen_rect(&self, selection: Selection) -> [f64; 4] {
        match self.view_transform() {
            Some(transform) => transform.rect(selection),
            None => [
                selection.x(),
                selection.y(),
                selection.width(),
                selection.height(),
            ],
        }
    }

    /// Maps one document point into window coordinates.
    fn screen_point(&self, point: [f64; 2]) -> [f64; 2] {
        match self.view_transform() {
            Some(transform) => transform.to_screen(point),
            None => point,
        }
    }

    /// Pans the whole-image view, keeping at least one image edge reachable.
    fn pan_by(&mut self, delta: [f64; 2]) {
        if self.view_transform().is_none() {
            return;
        }
        self.view_offset[0] += delta[0];
        self.view_offset[1] += delta[1];
        self.clamp_view();
    }

    /// Zooms around one window anchor point.
    fn zoom_at(&mut self, factor: f64, anchor: [f64; 2]) {
        let Some(transform) = self.view_transform() else {
            return;
        };
        let [viewport_width, viewport_height] = self.viewport;
        let [image_width, image_height] = self.logical_size();
        let fit = (viewport_width / image_width)
            .min(viewport_height / image_height)
            .min(1.0);
        let zoom = (transform.zoom * factor).clamp(fit.max(f64::EPSILON), 8.0);
        if (zoom - transform.zoom).abs() < f64::EPSILON {
            return;
        }
        let image = transform.to_image(anchor);
        self.view_zoom = zoom;
        self.view_offset = [anchor[0] - image[0] * zoom, anchor[1] - image[1] * zoom];
        self.clamp_view();
    }

    /// Keeps the whole-image view inside the viewport without losing the image.
    fn clamp_view(&mut self) {
        let Some(transform) = self.view_transform() else {
            return;
        };
        let [viewport_width, viewport_height] = self.viewport;
        let [image_width, image_height] = self.logical_size();
        let width = image_width * transform.zoom;
        let height = image_height * transform.zoom;
        self.view_offset[0] = if width <= viewport_width {
            (viewport_width - width) / 2.0
        } else {
            self.view_offset[0].clamp(viewport_width - width, 0.0)
        };
        self.view_offset[1] = if height <= viewport_height {
            (viewport_height - height) / 2.0
        } else {
            self.view_offset[1].clamp(viewport_height - height, 0.0)
        };
    }

    /// Creates or drops the live editor to match the authoritative text edit.
    pub(crate) fn sync_text_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.text_edit.clone() else {
            let had_input = self.text_input.take().is_some();
            let had_focus = self.text_focus.take().is_some();
            if had_input || had_focus {
                self.size_editing = false;
                self.size_draft.clear();
                // The removed editor element would otherwise keep window
                // focus and overlay keys (undo/redo/Escape) would reach no
                // handler; return focus to the overlay root instead.
                let focus = self.focus.clone();
                window.focus(&focus, cx);
                cx.notify();
            }
            return;
        };
        if let Some(input) = self.text_input.as_ref() {
            let size = edit.style.size_points();
            input.update(cx, |input, cx| {
                if input.size_points != size {
                    input.size_points = size;
                    input.sync(window, cx);
                }
            });
            return;
        }
        let focus = cx.focus_handle();
        let input = cx.new(|_| {
            TextInput::new(
                edit.content,
                edit.style.size_points(),
                edit.anchor,
                focus.clone(),
                Rc::clone(&self.queue),
                self.ticket,
            )
        });
        self.text_input = Some(input);
        self.text_focus = Some(focus.clone());
        // The frozen overlay is a non-activating panel. While another app is
        // frontmost, macOS resolves the input source per active app, so the
        // system IME would see that app's English layout even though the
        // overlay has key focus. Activate ArcShot for text editing so Pinyin
        // (or any system IME) follows this input context.
        cx.activate(true);
        window.activate_window();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Confirms the IME composition, commits the draft as one history step and
    /// clears the live editor.
    pub(crate) fn commit_editor(&mut self, cx: &mut Context<Self>) {
        if let Some(input) = self.text_input.take() {
            input.update(cx, |input, cx| {
                input.marked_range = None;
                cx.notify();
            });
        }
        // `text_focus` stays set until `sync_text_editor` observes the edit
        // is gone: only that path has the window needed to return focus to
        // the overlay root.
        self.text_edit = None;
        self.size_editing = false;
        self.size_draft.clear();
        self.queue
            .borrow_mut()
            .push(UiCommand::TextCommit(self.ticket));
        cx.notify();
    }
}

impl CaptureView {
    /// The style the option strip reflects: a selected object of the same tool,
    /// else that tool's session memory.
    fn effective_style(&self, tool: Tool) -> AnnotationStyle {
        // While a text edit is active, the option strip reflects the draft style
        // (which the core updates on every option click) rather than the last
        // committed object.
        if tool == Tool::Text
            && let Some(edit) = self.text_edit.as_ref()
        {
            return AnnotationStyle::Text(edit.style);
        }
        let memory = match tool {
            Tool::Rectangle | Tool::Select => AnnotationStyle::Rectangle(self.rectangle_style),
            Tool::Line => AnnotationStyle::Line(self.line_style),
            Tool::Arrow => AnnotationStyle::Arrow(self.arrow_style),
            Tool::Blur => AnnotationStyle::Blur(self.blur_style),
            Tool::Text => AnnotationStyle::Text(self.text_style),
        };
        self.selected
            .and_then(|index| self.annotations.get(index))
            .map(|annotation| annotation.style())
            .filter(|style| style.tool() == tool)
            .unwrap_or(memory)
    }

    /// One dock row: the tool button plus, while hovered, its option strip.
    ///
    /// The row owns the hover listener so moving from the tool into the strip
    /// keeps it open; leaving the row closes it. Hover never switches tools.
    #[allow(clippy::too_many_arguments)]
    fn tool_row(
        &self,
        tool: Tool,
        row_id: &'static str,
        id: &'static str,
        label: &'static str,
        tooltip: &'static str,
        description: Option<&'static str>,
        glyph: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ticket = self.ticket;
        let queue = Rc::clone(&self.queue);
        let metrics = DockMetrics::for_density(self.density);
        let entity = cx.entity();
        let hover_entity = entity.clone();
        let mut row =
            div()
                .id(row_id)
                .flex()
                .flex_row()
                .items_center()
                .on_hover(move |hovered, _, cx| {
                    let hovered = *hovered;
                    hover_entity.update(cx, |view, cx| {
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
            tooltip,
            description,
            glyph,
            self.tool == tool,
            metrics,
            &queue,
        ));
        if self.hovered_tool == Some(tool) && tool.is_annotation() {
            row = row.child(self.option_strip(tool, metrics, entity));
        }
        row
    }

    /// The hover strip for one annotation tool. Width and palette are shared;
    /// rectangle adds fill, line adds dash and arrow adds the inward head.
    /// Both densities render the same options; only the metrics differ.
    fn option_strip(
        &self,
        tool: Tool,
        metrics: DockMetrics,
        entity: Entity<Self>,
    ) -> impl IntoElement {
        let ticket = self.ticket;
        let queue = Rc::clone(&self.queue);
        let style = self.effective_style(tool);
        let (strip_id, strip_label) = match tool {
            Tool::Line => ("line-options", "直线选项"),
            Tool::Arrow => ("arrow-options", "箭头选项"),
            Tool::Blur => ("blur-options", "模糊选项"),
            Tool::Text => ("text-options", "文字选项"),
            Tool::Rectangle | Tool::Select => ("rectangle-options", "矩形选项"),
        };
        let mut strip = div()
            .id(strip_id)
            .role(Role::Group)
            .aria_label(strip_label)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(metrics.strip_gap))
            .ml(px(8.0))
            .px(px(metrics.strip_pad_x))
            .py(px(metrics.strip_pad_y))
            .rounded_lg()
            .bg(rgba(0x20242bee));
        // Standard keeps a visible group heading; Compact drops it. The option
        // set below is identical in both densities.
        if metrics.show_heading {
            let (heading_id, heading_text) = match tool {
                Tool::Line => ("line-options-heading", "直线"),
                Tool::Arrow => ("arrow-options-heading", "箭头"),
                Tool::Blur => ("blur-options-heading", "模糊"),
                Tool::Text => ("text-options-heading", "文字"),
                Tool::Rectangle | Tool::Select => ("rectangle-options-heading", "矩形"),
            };
            strip = strip.child(
                div()
                    .id(heading_id)
                    .text_size(px(metrics.heading_text))
                    .text_color(rgb(0x9aa4b5))
                    .child(heading_text),
            );
        }
        if tool == Tool::Blur {
            // The blur strip has no width, palette or fill: only the three
            // levels, so the effect itself stays the single variable.
            for level in BlurLevel::ALL {
                strip = strip.child(option_button(
                    option_id(tool, level.label()),
                    level_label(level),
                    level_glyph(level),
                    style.blur().is_some_and(|blur| blur.level() == level),
                    None,
                    metrics,
                    UiCommand::BlurOption(ticket, BlurOption::Level(level)),
                    &queue,
                ));
            }
            return strip;
        }
        if tool == Tool::Text {
            // Text has no width, font, weight, background or alignment options:
            // one fixed palette and one whole-object size. The size applies to
            // the selected text object or the tool memory, quantized by the core.
            for color in PaletteColor::ALL {
                strip = strip.child(option_button(
                    option_id(tool, &format!("color.{}", color_name(color))),
                    color_label(color),
                    "",
                    style.text().is_some_and(|text| text.color() == color),
                    Some(color.rgb()),
                    metrics,
                    UiCommand::TextOption(ticket, TextOption::Stroke(color)),
                    &queue,
                ));
            }
            let size = style.text().map_or(16.0, |text| text.size_points());
            let display_label = if self.size_editing {
                self.size_draft.clone()
            } else {
                TextStyle::new(
                    style.text().map_or(PaletteColor::Red, |text| text.color()),
                    size,
                )
                .size_label()
            };
            let step_down = TextStyle::quantize_size(size - 0.5);
            let step_up = TextStyle::quantize_size(size + 0.5);
            strip = strip.child(option_button(
                option_id(tool, "size.down"),
                "字号减 0.5 点",
                "−",
                false,
                None,
                metrics,
                UiCommand::TextOption(ticket, TextOption::Size(step_down)),
                &queue,
            ));
            strip = strip.child(
                div()
                    .id("text-size-value")
                    .role(Role::Button)
                    .aria_label(format!("字号 {display_label} 点"))
                    .aria_value(display_label.clone())
                    .accessibility_id("arcshot.overlay.text.size")
                    .tooltip(move |_, cx| {
                        cx.new(|_| TooltipView {
                            label: "字号（点击后可直接输入）",
                        })
                        .into()
                    })
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(metrics.option_side + 8.0))
                    .h(px(metrics.option_side))
                    .rounded_md()
                    .bg(rgba(0x2a2f3add))
                    .text_color(rgb(0xffffff))
                    .text_size(px(metrics.option_text))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |view, cx| {
                            let current = view
                                .effective_style(Tool::Text)
                                .text()
                                .map_or(16.0, |text| text.size_points());
                            view.size_draft =
                                TextStyle::new(PaletteColor::Red, current).size_label();
                            view.size_editing = true;
                            cx.notify();
                        });
                    })
                    .child(display_label),
            );
            strip = strip.child(option_button(
                option_id(tool, "size.up"),
                "字号加 0.5 点",
                "+",
                false,
                None,
                metrics,
                UiCommand::TextOption(ticket, TextOption::Size(step_up)),
                &queue,
            ));
            return strip;
        }
        for width in StrokeWidth::ALL {
            let command = match tool {
                Tool::Line => UiCommand::LineOption(ticket, LineOption::Width(width)),
                Tool::Arrow => UiCommand::ArrowOption(ticket, ArrowOption::Width(width)),
                Tool::Rectangle | Tool::Select => {
                    UiCommand::RectangleOption(ticket, RectangleOption::Width(width))
                }
                // The text and blur strips return above; these arms only
                // satisfy exhaustiveness for the shared colour/width loops.
                Tool::Blur | Tool::Text => return strip,
            };
            strip = strip.child(option_button(
                option_id(tool, &format!("width.{}", width.label())),
                width_label(width),
                width.label(),
                style.width() == Some(width),
                None,
                metrics,
                command,
                &queue,
            ));
        }
        for color in PaletteColor::ALL {
            let command = match tool {
                Tool::Line => UiCommand::LineOption(ticket, LineOption::Stroke(color)),
                Tool::Arrow => UiCommand::ArrowOption(ticket, ArrowOption::Stroke(color)),
                Tool::Rectangle | Tool::Select => {
                    UiCommand::RectangleOption(ticket, RectangleOption::Stroke(color))
                }
                Tool::Blur | Tool::Text => return strip,
            };
            strip = strip.child(option_button(
                option_id(tool, &format!("color.{}", color_name(color))),
                color_label(color),
                "",
                style.stroke() == Some(color),
                Some(color.rgb()),
                metrics,
                command,
                &queue,
            ));
        }
        match tool {
            Tool::Line => {
                let dash = style.line().is_some_and(LineStyle::dash);
                strip.child(option_button(
                    option_id(tool, "dash"),
                    "虚线",
                    "╌",
                    dash,
                    None,
                    metrics,
                    UiCommand::LineOption(ticket, LineOption::Dash(!dash)),
                    &queue,
                ))
            }
            Tool::Arrow => {
                let inward = style.arrow().is_some_and(ArrowStyle::inward);
                strip.child(option_button(
                    option_id(tool, "inward"),
                    "内凹箭头",
                    "↯",
                    inward,
                    None,
                    metrics,
                    UiCommand::ArrowOption(ticket, ArrowOption::Inward(!inward)),
                    &queue,
                ))
            }
            Tool::Rectangle | Tool::Select => {
                let fill = style.rectangle().is_some_and(RectangleStyle::fill);
                strip.child(option_button(
                    option_id(tool, "fill"),
                    "20% 同色填充",
                    "▨",
                    fill,
                    None,
                    metrics,
                    UiCommand::RectangleOption(ticket, RectangleOption::Fill(!fill)),
                    &queue,
                ))
            }
            Tool::Blur | Tool::Text => strip,
        }
    }
}

/// Stable option identifier shared by AX and the pointer path.
fn option_id(tool: Tool, option: &str) -> String {
    let tool = match tool {
        Tool::Line => "line",
        Tool::Arrow => "arrow",
        Tool::Blur => "blur",
        Tool::Text => "text",
        Tool::Rectangle | Tool::Select => "rectangle",
    };
    format!("arcshot.overlay.option.{tool}.{option}")
}

const fn width_label(width: StrokeWidth) -> &'static str {
    match width {
        StrokeWidth::Two => "线宽 2 点",
        StrokeWidth::Three => "线宽 3 点",
        StrokeWidth::Five => "线宽 5 点",
    }
}

const fn color_name(color: PaletteColor) -> &'static str {
    match color {
        PaletteColor::Red => "red",
        PaletteColor::Orange => "orange",
        PaletteColor::Yellow => "yellow",
        PaletteColor::Green => "green",
        PaletteColor::Blue => "blue",
        PaletteColor::White => "white",
        PaletteColor::Black => "black",
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

const fn level_label(level: BlurLevel) -> &'static str {
    match level {
        BlurLevel::Weak => "弱模糊",
        BlurLevel::Medium => "中模糊",
        BlurLevel::Strong => "强模糊",
    }
}

const fn level_glyph(level: BlurLevel) -> &'static str {
    match level {
        BlurLevel::Weak => "░",
        BlurLevel::Medium => "▒",
        BlurLevel::Strong => "▓",
    }
}

fn rgb_u32([red, green, blue]: [u8; 3]) -> u32 {
    u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue)
}

#[expect(
    clippy::too_many_arguments,
    reason = "a GPUI option button needs its identity, label, state, metrics, command and queue"
)]
fn option_button(
    id: String,
    label: &'static str,
    glyph: &'static str,
    selected: bool,
    swatch: Option<[u8; 3]>,
    metrics: DockMetrics,
    command: UiCommand,
    queue: &UiQueue,
) -> impl IntoElement {
    let click_queue = Rc::clone(queue);
    let a11y_queue = Rc::clone(queue);
    let a11y_command = command.clone();
    let a11y_id = id.clone();
    let background = match swatch {
        Some(channels) => rgb(rgb_u32(channels)),
        None => rgba(0x2a2f3add),
    };
    div()
        .id(id.clone())
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
        .w(px(metrics.option_side))
        .h(px(metrics.option_side))
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
        .text_size(px(metrics.option_text))
        .child(glyph)
        .on_click(move |_, _, _| click_queue.borrow_mut().push(command.clone()))
        .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
            log::info!("a11y_action={a11y_id} action=Click");
            a11y_queue.borrow_mut().push(a11y_command.clone());
        })
}

#[expect(
    clippy::too_many_arguments,
    reason = "a GPUI tool button needs its identity, label, state, metrics, command and queue"
)]
fn tool_button(
    ticket: CaptureTicket,
    tool: Tool,
    id: &'static str,
    label: &'static str,
    tooltip: &'static str,
    description: Option<&'static str>,
    glyph: &'static str,
    active: bool,
    metrics: DockMetrics,
    queue: &UiQueue,
) -> impl IntoElement {
    let click_queue = Rc::clone(queue);
    let a11y_queue = Rc::clone(queue);
    let mut button = div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(active)
        .aria_toggled(if active {
            Toggled::True
        } else {
            Toggled::False
        })
        .accessibility_id(id);
    if let Some(description) = description {
        button = button.aria_description(description);
    }
    button
        .tooltip(move |_, cx| cx.new(|_| TooltipView { label: tooltip }).into())
        .w(px(metrics.tool_side))
        .h(px(metrics.tool_side))
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
        .text_size(px(metrics.tool_text))
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

/// One addressable gesture affordance on the overlay.
///
/// The node carries a stable `accessibility_id`, a role, a value that mirrors
/// the authoritative geometry, and the pointer-equivalent action set. It is
/// never a second editor: every activation is translated into the same
/// `UiCommand` sequence a pointer gesture at the same points would enqueue.
struct A11yGesture {
    id: String,
    label: String,
    value: String,
    /// What `SetValue` with this node's payload does.
    affordance: A11yAffordance,
    /// What `Click` does, when the node is clickable at all.
    click: Option<A11yAffordance>,
}

/// Builds one gesture node; the caller positions it with the returned div.
fn a11y_gesture(
    gesture: A11yGesture,
    ticket: CaptureTicket,
    view: Entity<CaptureView>,
    queue: &UiQueue,
) -> gpui::Stateful<gpui::Div> {
    let A11yGesture {
        id,
        label,
        value,
        affordance,
        click,
    } = gesture;
    let set_id = id.clone();
    let click_id = id.clone();
    let set_view = view.clone();
    let set_queue = Rc::clone(queue);
    // `textfield` is the only role that both earns a ref and accepts a semantic
    // write: `group` gets no ref and `button` advertises no `SetValue`, so
    // neither can carry a gesture payload (#63).
    let mut node = div()
        .id(gpui::ElementId::Name(id.clone().into()))
        .role(Role::TextInput)
        .aria_label(label)
        .aria_value(value)
        .accessibility_id(id)
        .on_a11y_action(AccessibleAction::SetValue, move |data, _, cx| {
            let payload = match data {
                Some(gpui::accesskit::ActionData::Value(text)) => text.to_string(),
                Some(gpui::accesskit::ActionData::NumericValue(value)) => value.to_string(),
                _ => String::new(),
            };
            log::info!("a11y_action={set_id} action=SetValue value={payload}");
            let commands = set_view.update(cx, |view, cx| {
                // A press while editing commits the draft first, exactly as the
                // pointer handler does; the same press starts nothing else.
                if view.text_edit.is_some() {
                    view.commit_editor(cx);
                    return Vec::new();
                }
                let context = view.gesture_context();
                affordance_commands(&context, ticket, affordance, &payload)
            });
            set_queue.borrow_mut().extend(commands);
        });
    if let Some(click_affordance) = click {
        let click_view = view;
        let click_queue = Rc::clone(queue);
        node = node.on_a11y_action(AccessibleAction::Click, move |_, _, cx| {
            log::info!("a11y_action={click_id} action=Click");
            let commands = click_view.update(cx, |view, cx| {
                // Same rule for a bare activation: commit, then do nothing else.
                if view.text_edit.is_some() {
                    view.commit_editor(cx);
                    return Vec::new();
                }
                let context = view.gesture_context();
                affordance_commands(&context, ticket, click_affordance, "")
            });
            click_queue.borrow_mut().extend(commands);
        });
    }
    node
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
        // The whole-image surface never reselects its extent, so the selection
        // chrome would be non-interactive there; object handles stay editable.
        let show_selection_handles = show_handles && !self.long_image;
        let error = self.error.clone();
        let [document_width, document_height] = self.logical_size();
        // The whole-image surface draws the complete document through the view
        // transform; the region surface still fills the window exactly.
        let image_rect = match self.view_transform() {
            Some(transform) => {
                let [x, y] = transform.to_screen([0.0, 0.0]);
                [
                    x,
                    y,
                    document_width * transform.zoom,
                    document_height * transform.zoom,
                ]
            }
            None => [0.0, 0.0, document_width, document_height],
        };
        let image = self.image.as_ref().map(|image| {
            img(Arc::clone(image))
                .absolute()
                .left(px(image_rect[0] as f32))
                .top(px(image_rect[1] as f32))
                .w(px(image_rect[2] as f32))
                .h(px(image_rect[3] as f32))
                .object_fit(gpui::ObjectFit::Fill)
        });

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
                    // Direct size input consumes every key until Enter or Esc.
                    if entity.read_with(cx, |view, _| view.size_editing) {
                        entity.update(cx, |view, cx| {
                            match event.keystroke.key.as_ref() {
                                "escape" => {
                                    view.size_editing = false;
                                    view.size_draft.clear();
                                }
                                "enter" => {
                                    if let Ok(size) = view.size_draft.trim().parse::<f64>() {
                                        let size = TextStyle::quantize_size(size);
                                        view.queue.borrow_mut().push(UiCommand::TextOption(
                                            ticket,
                                            TextOption::Size(size),
                                        ));
                                    }
                                    view.size_editing = false;
                                    view.size_draft.clear();
                                }
                                "backspace" => {
                                    view.size_draft.pop();
                                }
                                "delete" => {
                                    view.size_draft.clear();
                                }
                                _ => {
                                    if let Some(ch) = plain_key_char(&event.keystroke.key)
                                        && (ch.is_ascii_digit() || ch == '.')
                                        && view.size_draft.len() < 8
                                    {
                                        view.size_draft.push(ch);
                                    }
                                }
                            }
                            cx.notify();
                        });
                        cx.stop_propagation();
                        return;
                    }
                    // Read the live drag flag rather than the render-time snapshot,
                    // so a key right after release never sees a stale held state.
                    let context = entity.read_with(cx, |view, cx| CaptureKeyContext {
                        busy: view.busy,
                        dragging: view.drag.is_some(),
                        object_selected: view.selected.is_some(),
                        tool: view.tool,
                        text_editing: view.text_edit.is_some(),
                        ime_composing: view
                            .text_input
                            .as_ref()
                            .is_some_and(|input| input.read(cx).marked_range.is_some()),
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
                        Some(CaptureKeyAction::EndTextEdit) => {
                            // Esc layer zero: end editing and keep the content.
                            entity.update(cx, |view, cx| {
                                view.commit_editor(cx);
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
                    let double = event.click_count >= 2;
                    entity.update(cx, |view, cx| {
                        if !frozen || view.busy {
                            return;
                        }
                        // Map the window point into document space first: hit
                        // tests, creation bounds and drags share one frame.
                        let point = view.view_point(pointer(&event.position));
                        // A press while editing commits the draft; the same press
                        // does not also start a new object.
                        if view.text_edit.is_some() {
                            view.commit_editor(cx);
                            cx.notify();
                            return;
                        }
                        // Only the visible part of an object is interactive.
                        let object = view
                            .selection
                            .filter(|selection| selection.contains(point))
                            .and_then(|_| arcshot_core::annotation_at(&view.annotations, point));
                        let text_hit = object
                            .and_then(|index| view.annotations.get(index))
                            .and_then(|annotation| annotation.style().text())
                            .is_some();
                        if capture_text_press_action(double, view.tool, text_hit, point).is_some() {
                            view.queue
                                .borrow_mut()
                                .push(UiCommand::BeginText { ticket, point });
                            cx.notify();
                            return;
                        }
                        let action = if view.long_image {
                            capture_long_image_press_action(
                                double,
                                view.busy,
                                view.tool,
                                view.selection,
                                object.is_some(),
                                point,
                            )
                        } else {
                            capture_press_action(
                                double,
                                view.busy,
                                view.tool,
                                view.selection,
                                object.is_some(),
                                point,
                            )
                        };
                        match action {
                            // Core checks whether the point is actually inside
                            // the selection; the toolbar is not the background.
                            Some(CapturePressAction::FinishAt(point)) => {
                                view.queue.borrow_mut().push(UiCommand::FinishCapture {
                                    ticket,
                                    point: Some(point),
                                });
                            }
                            Some(CapturePressAction::Create) if view.tool == Tool::Text => {
                                // The text tool positions an editor instead of a drag.
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::BeginText { ticket, point });
                            }
                            Some(CapturePressAction::EditText(_)) => {}
                            Some(CapturePressAction::Create) => {
                                // A new object starts unselected; the draft is
                                // selected as soon as the first move lands.
                                view.selected = None;
                                view.drag = Some(DragState::Core);
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerPressed { ticket, point });
                            }
                            Some(CapturePressAction::ObjectDrag) => {
                                // Mirror the picked layer immediately so keys
                                // pressed before the next tick see the selection.
                                view.selected = object;
                                view.drag = Some(DragState::Core);
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerPressed { ticket, point });
                            }
                            Some(CapturePressAction::Drag) => {
                                view.selected = None;
                                view.drag = Some(DragState::Core);
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerPressed { ticket, point });
                            }
                            Some(CapturePressAction::Band) => {
                                view.selected = None;
                                view.drag = Some(DragState::Band {
                                    start: point,
                                    current: point,
                                });
                            }
                            None => {
                                // The whole-image surface never moves or
                                // reselects; empty space only clears the pick.
                                if view.long_image {
                                    view.selected = None;
                                }
                            }
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
                        let point = view.view_point(pointer(&event.position));
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
                    entity.update(cx, |view, cx| {
                        let Some(drag) = view.drag.take() else {
                            return;
                        };
                        let end = view.view_point(pointer(&event.position));
                        match drag {
                            DragState::Band { start, .. } => {
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
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerMoved { ticket, point: end });
                                view.queue
                                    .borrow_mut()
                                    .push(UiCommand::PointerReleased(ticket));
                            }
                        }
                        cx.notify();
                    });
                }
            })
            .on_scroll({
                let entity = cx.entity();
                move |event: &gpui::ScrollEvent, _, cx| {
                    entity.update(cx, |view, cx| {
                        if !view.long_image || view.busy {
                            return;
                        }
                        let delta = match event.delta {
                            gpui::ScrollDelta::Pixels(delta) => {
                                [f64::from(delta.x), f64::from(delta.y)]
                            }
                            gpui::ScrollDelta::Lines(delta) => {
                                [f64::from(delta.x) * 20.0, f64::from(delta.y) * 20.0]
                            }
                        };
                        view.pan_by(delta);
                        cx.notify();
                    });
                }
            })
            .on_pinch({
                let entity = cx.entity();
                move |event: &gpui::PinchEvent, _, cx| {
                    entity.update(cx, |view, cx| {
                        if !view.long_image || view.busy {
                            return;
                        }
                        let anchor = [f64::from(event.position.x), f64::from(event.position.y)];
                        view.zoom_at(1.0 + f64::from(event.delta), anchor);
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
            let [x, y, width, height] = self.screen_rect(selection);
            root = root.child(
                img(Arc::clone(&preview.image))
                    .absolute()
                    .left(px(x as f32))
                    .top(px(y as f32))
                    .w(px(width as f32))
                    .h(px(height as f32))
                    .object_fit(gpui::ObjectFit::Fill),
            );
        }

        if let Some((x, y, width, height)) = preview {
            // Region mode dims outside the selection and labels its physical
            // crop. The whole-image surface has no reselectable band and its
            // output extent is already the complete document.
            if self.view_transform().is_none() {
                let [full_width, full_height] = self.logical_size().map(|value| value as f32);
                root = root
                    .child(region(0.0, 0.0, full_width, y).bg(rgba(0x00000099)))
                    .child(region(0.0, y, x, height).bg(rgba(0x00000099)))
                    .child(
                        region(x + width, y, full_width - x - width, height).bg(rgba(0x00000099)),
                    )
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
            }

            // Eight control points are the visible move/resize affordances; the
            // same logical centers drive the core hit test. Each point is also an
            // addressable AX node whose SetValue drags it to a new position.
            if show_selection_handles && let Some(selection) = self.selection {
                for handle in RESIZE_HANDLES {
                    let center = selection.handle_center(handle);
                    let [x, y] = self.screen_point(center).map(|value| value as f32);
                    let node = a11y_gesture(
                        A11yGesture {
                            id: format!("arcshot.overlay.selection.handle.{}", handle_name(handle)),
                            label: format!("选区控制点 {}", handle_name(handle)),
                            value: format_point(center),
                            affordance: A11yAffordance::DragFrom(A11yAnchor::SelectionHandle(
                                handle,
                            )),
                            click: None,
                        },
                        ticket,
                        cx.entity(),
                        &self.queue,
                    );
                    root = root.child(
                        node.absolute()
                            .left(px(x - 4.0))
                            .top(px(y - 4.0))
                            .w(px(8.0))
                            .h(px(8.0))
                            .bg(rgb(0x007aff))
                            .border_1()
                            .border_color(rgb(0xffffff)),
                    );
                }
            }

            // The selected object's own eight control points use its stroke
            // colour, so object chrome is distinguishable from selection chrome.
            // Rectangle controls drag a corner/edge; line and arrow controls drag
            // one endpoint. Both are addressable AX nodes.
            if show_handles
                && let Some((index, annotation)) = self
                    .selected
                    .and_then(|index| self.annotations.get(index).map(|object| (index, object)))
            {
                let [red, green, blue] = match annotation.style().stroke() {
                    Some(color) => color.rgb(),
                    // A blur rectangle has no stroke; its handles stay visible
                    // with a neutral white instead of inventing a colour.
                    None => [0xFF, 0xFF, 0xFF],
                };
                let color = rgb(u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue));
                let controls: Vec<(&'static str, A11yAffordance, [f64; 2])> = match annotation
                    .shape()
                {
                    Shape::Rectangle(_) => RESIZE_HANDLES
                        .into_iter()
                        .filter_map(|handle| {
                            annotation.handle_center(handle).map(|center| {
                                (
                                    handle_name(handle),
                                    A11yAffordance::DragFrom(A11yAnchor::ObjectHandle {
                                        index,
                                        handle,
                                    }),
                                    center,
                                )
                            })
                        })
                        .collect(),
                    Shape::Line(segment) => [Endpoint::Start, Endpoint::End]
                        .into_iter()
                        .map(|endpoint| {
                            (
                                endpoint_name(endpoint),
                                A11yAffordance::DragFrom(A11yAnchor::Endpoint { index, endpoint }),
                                segment.endpoint_center(endpoint),
                            )
                        })
                        .collect(),
                    Shape::Text(_) => Vec::new(),
                };
                for (name, affordance, center) in controls {
                    let [x, y] = self.screen_point(center).map(|value| value as f32);
                    // Line and arrow controls are endpoints, not resize handles;
                    // the id says which so the E2E ids never churn.
                    let kind = match affordance {
                        A11yAffordance::DragFrom(A11yAnchor::Endpoint { .. }) => "endpoint",
                        _ => "handle",
                    };
                    let node = a11y_gesture(
                        A11yGesture {
                            id: format!("arcshot.overlay.object.{index}.{kind}.{name}"),
                            label: format!("对象 {index} 控制点 {name}"),
                            value: format_point(center),
                            affordance,
                            click: None,
                        },
                        ticket,
                        cx.entity(),
                        &self.queue,
                    );
                    root = root.child(
                        node.absolute()
                            .left(px(x - 4.0))
                            .top(px(y - 4.0))
                            .w(px(8.0))
                            .h(px(8.0))
                            .bg(color)
                            .border_1()
                            .border_color(rgb(0xffffff)),
                    );
                }
            }
        }

        // Selection and object gestures have no drag equivalent in assistive
        // technology, so the surface exposes them as addressable nodes whose
        // values mirror the authoritative geometry and whose SetValue replays
        // the pointer command sequence. The selection node exists even without
        // a selection so the first band can be created without a drag.
        if frozen && !self.long_image {
            let node = a11y_gesture(
                A11yGesture {
                    id: "arcshot.overlay.selection".to_owned(),
                    label: "选区".to_owned(),
                    value: self.selection_value(),
                    affordance: A11yAffordance::Band,
                    click: None,
                },
                ticket,
                cx.entity(),
                &self.queue,
            );
            root = root.child(node.absolute().left(px(0.0)).top(px(0.0)).size_full());
            if let Some(selection) = self.selection {
                // Moving the selection is a press on its body; the node only
                // exists where that gesture is legal (the select tool).
                if show_selection_handles && self.tool == Tool::Select {
                    let center = [
                        selection.x() + selection.width() / 2.0,
                        selection.y() + selection.height() / 2.0,
                    ];
                    let [x, y] = self.screen_point(center).map(|value| value as f32);
                    let node = a11y_gesture(
                        A11yGesture {
                            id: "arcshot.overlay.selection.body".to_owned(),
                            label: "移动选区".to_owned(),
                            value: format_point(center),
                            affordance: A11yAffordance::DragFrom(A11yAnchor::SelectionCenter),
                            click: None,
                        },
                        ticket,
                        cx.entity(),
                        &self.queue,
                    );
                    root = root.child(
                        node.absolute()
                            .left(px(x - 4.0))
                            .top(px(y - 4.0))
                            .w(px(8.0))
                            .h(px(8.0)),
                    );
                }
                // Creating an object with an annotation tool is a drag inside
                // the selection; the value reports the last object's extent so
                // the caller can see what the previous gesture produced.
                if show_handles && self.tool.is_annotation() {
                    let value = self
                        .annotations
                        .last()
                        .map_or_else(|| "none".to_owned(), Self::object_value);
                    let node = a11y_gesture(
                        A11yGesture {
                            id: "arcshot.overlay.object.new".to_owned(),
                            label: format!("新建{:?}标注", self.tool),
                            value,
                            affordance: A11yAffordance::CornersGesture,
                            // The text tool opens an editor instead of dragging,
                            // so its creation is a bare activation: `SetValue`
                            // would leave the reported value unchanged.
                            click: (self.tool == Tool::Text).then_some(A11yAffordance::BeginText),
                        },
                        ticket,
                        cx.entity(),
                        &self.queue,
                    );
                    root = root.child(
                        node.absolute()
                            .left(px(4.0))
                            .top(px(4.0))
                            .w(px(8.0))
                            .h(px(8.0)),
                    );
                }
            }
        }

        // Every annotation object is an addressable node at its drawn extent,
        // on the region surface and on the whole-image surface alike. A click
        // selects it; SetValue moves it by dragging its centre.
        if frozen && !self.busy {
            for (index, annotation) in self.annotations.iter().enumerate() {
                let Some([x, y, width, height]) = self.object_screen_rect(annotation) else {
                    continue;
                };
                let node = a11y_gesture(
                    A11yGesture {
                        id: format!("arcshot.overlay.object.{index}"),
                        label: format!("标注对象 {index} {:?}", annotation.style().tool()),
                        value: Self::object_value(annotation),
                        affordance: A11yAffordance::MoveObject(index),
                        click: Some(A11yAffordance::Click(A11yAnchor::ObjectCenter(index))),
                    },
                    ticket,
                    cx.entity(),
                    &self.queue,
                );
                root = root.child(
                    node.absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(width))
                        .h(px(height)),
                );
                // A text object also exposes the select-tool double-click, the
                // only pointer path into re-editing. Without it the AX surface
                // could move text but never edit it again (#63).
                if annotation.style().text().is_some() {
                    let node = a11y_gesture(
                        A11yGesture {
                            id: format!("arcshot.overlay.object.{index}.edit"),
                            label: format!("编辑文字对象 {index}"),
                            value: Self::object_value(annotation),
                            affordance: A11yAffordance::ReeditText(index),
                            click: None,
                        },
                        ticket,
                        cx.entity(),
                        &self.queue,
                    );
                    root = root.child(
                        node.absolute()
                            .left(px(x + width - 8.0))
                            .top(px(y + height - 8.0))
                            .w(px(8.0))
                            .h(px(8.0)),
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
                    .when(!self.long_image, |actions| {
                        actions.child({
                            // Scroll entry: enabled with a valid selection. Existing
                            // annotations are refused by the core with an explicit
                            // reason; the button never silently drops them.
                            let queue = Rc::clone(&self.queue);
                            let a11y_queue = Rc::clone(&self.queue);
                            let scroll_enabled = frozen && self.selection.is_some() && !busy;
                            let mut button = div()
                                .id("scroll-capture")
                                .role(Role::Button)
                                .aria_label("进入滚动截图")
                                .accessibility_id("arcshot.overlay.scroll")
                                .px_4()
                                .py_2()
                                .rounded_md()
                                .text_color(rgb(0xffffff))
                                .bg(if scroll_enabled {
                                    rgb(0x3b465b)
                                } else {
                                    rgb(0x2a2f3a)
                                })
                                .child("滚动");
                            if scroll_enabled {
                                button = button
                                    .role(Role::Button)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(0x52617b)))
                                    .on_click(move |_, _, _| {
                                        queue.borrow_mut().push(UiCommand::EnterScroll(ticket));
                                    })
                                    .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
                                        log::info!(
                                            "a11y_action=arcshot.overlay.scroll action=Click"
                                        );
                                        a11y_queue
                                            .borrow_mut()
                                            .push(UiCommand::EnterScroll(ticket));
                                    });
                            }
                            button
                        })
                    })
                    .child({
                        let queue = Rc::clone(&self.queue);
                        let a11y_queue = Rc::clone(&self.queue);
                        let entity = cx.entity();
                        let entity_for_a11y = entity.clone();
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
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| view.commit_editor(cx));
                                    queue.borrow_mut().push(UiCommand::SaveCapture(ticket));
                                })
                                .on_a11y_action(AccessibleAction::Click, move |_, _, cx| {
                                    log::info!("a11y_action=arcshot.overlay.save action=Click");
                                    entity_for_a11y.update(cx, |view, cx| view.commit_editor(cx));
                                    a11y_queue.borrow_mut().push(UiCommand::SaveCapture(ticket));
                                });
                        }
                        button
                    })
                    .child({
                        let queue = Rc::clone(&self.queue);
                        let a11y_queue = Rc::clone(&self.queue);
                        let entity = cx.entity();
                        let entity_for_a11y = entity.clone();
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
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| view.commit_editor(cx));
                                    queue.borrow_mut().push(UiCommand::FinishCapture {
                                        ticket,
                                        point: None,
                                    });
                                })
                                .on_a11y_action(AccessibleAction::Click, move |_, _, cx| {
                                    log::info!("a11y_action=arcshot.overlay.finish action=Click");
                                    entity_for_a11y.update(cx, |view, cx| view.commit_editor(cx));
                                    a11y_queue.borrow_mut().push(UiCommand::FinishCapture {
                                        ticket,
                                        point: None,
                                    });
                                });
                        }
                        button
                    })
                    .when(self.long_image, |actions| {
                        // Accessibility path for users who cannot perform a
                        // press-drag gesture: the same Rectangle object through
                        // the authoritative editor, never a second annotator.
                        actions.child({
                            let queue = Rc::clone(&self.queue);
                            let a11y_queue = Rc::clone(&self.queue);
                            div()
                                .id("completion-add-rectangle")
                                .role(Role::Button)
                                .aria_label("添加矩形标注")
                                .accessibility_id("arcshot.overlay.completion.add_rectangle")
                                .px_4()
                                .py_2()
                                .rounded_md()
                                .text_color(rgb(0xffffff))
                                .bg(rgb(0x3b465b))
                                .cursor_pointer()
                                .on_click(move |_, _, _| {
                                    queue
                                        .borrow_mut()
                                        .push(UiCommand::AddDefaultRectangle(ticket));
                                })
                                .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
                                    log::info!(
                                        "a11y_action=arcshot.overlay.completion.add_rectangle action=Click"
                                    );
                                    a11y_queue
                                        .borrow_mut()
                                        .push(UiCommand::AddDefaultRectangle(ticket));
                                })
                                .child("加矩形")
                        })
                    }),
            );
        }

        if frozen {
            // The left vertical dock lists only tools that exist: no fake or
            // disabled entries. Hover reveals that tool's options to its right.
            // Density changes only sizes/spacing/heading; the entries stay at the
            // same fixed screen anchor so they never shrink out of reach.
            let metrics = DockMetrics::for_density(self.density);
            let heading = match self.density {
                Density::Standard => "标注工具",
                Density::Compact => "工具",
            };
            let mut dock = div()
                .id("tool-dock")
                .role(Role::Group)
                .aria_label("标注工具")
                .absolute()
                .left_2()
                .top_16()
                .flex()
                .flex_col()
                .gap(px(metrics.dock_gap))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
            // AX-observable dock attribute for the real current density. The
            // node and its id exist in both densities so switching never removes
            // a control; Standard keeps a larger group heading, Compact a tiny
            // compressed title.
            dock = dock.child(
                div()
                    .id("density-state")
                    .role(Role::Label)
                    .aria_label(format!("选项密度 {:?}", self.density))
                    .aria_value(self.density.as_str())
                    .accessibility_id("arcshot.overlay.density")
                    .text_size(px(metrics.heading_text))
                    .text_color(rgb(0x9aa4b5))
                    .child(heading),
            );
            dock = dock.child(self.tool_row(
                Tool::Select,
                "select-tool-row",
                "arcshot.overlay.tool.select",
                "选择",
                "选择",
                None,
                "⌖",
                cx,
            ));
            dock = dock.child(self.tool_row(
                Tool::Rectangle,
                "rectangle-tool-row",
                "arcshot.overlay.tool.rectangle",
                "矩形",
                "矩形",
                None,
                "▭",
                cx,
            ));
            dock = dock.child(self.tool_row(
                Tool::Line,
                "line-tool-row",
                "arcshot.overlay.tool.line",
                "直线",
                "直线",
                None,
                "╱",
                cx,
            ));
            dock = dock.child(self.tool_row(
                Tool::Arrow,
                "arrow-tool-row",
                "arcshot.overlay.tool.arrow",
                "箭头",
                "箭头",
                None,
                "➤",
                cx,
            ));
            dock = dock.child(self.tool_row(
                Tool::Blur,
                "blur-tool-row",
                "arcshot.overlay.tool.blur",
                "模糊矩形",
                BLUR_TOOLTIP,
                Some(BLUR_TOOLTIP),
                "▨",
                cx,
            ));
            dock = dock.child(self.tool_row(
                Tool::Text,
                "text-tool-row",
                "arcshot.overlay.tool.text",
                "文字",
                "文字",
                None,
                "T",
                cx,
            ));
            if self.selected.is_some() && !busy {
                dock = dock.child(delete_button(ticket, &self.queue));
            }
            root = root.child(dock);
        }

        if let Some(input) = self.text_input.clone()
            && let Some(edit) = self.text_edit.as_ref()
            && let Some(focus) = self.text_focus.clone()
        {
            let metrics = crate::text_font::measure(&edit.content, edit.style.size_points());
            let width = (metrics.max_line_width as f32).max(1.0);
            let height = (metrics.line_count as f32 * metrics.line_height as f32).max(1.0);
            // The live editor shell follows the view transform; the measured
            // document bounds stay in image coordinates.
            let zoom = self.view_zoom;
            let [x, y] = self.screen_point(edit.anchor);
            root = root.child(text_editor(
                input,
                [x, y],
                width * zoom as f32,
                height * zoom as f32,
                focus,
            ));
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
    /// Title used by error notices; permission notices keep their own titles.
    pub(crate) error_heading: &'static str,
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
    let click_command = command.clone();
    let a11y_command = command;
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
        .on_click(move |_, _, _| click_queue.borrow_mut().push(click_command.clone()))
        // AXPress never reaches `on_click`, so the accessible path enqueues the
        // same command explicitly and reports itself for end-to-end checks.
        .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
            log::info!("a11y_action={a11y_id} action=Click");
            a11y_queue.borrow_mut().push(a11y_command.clone());
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
                self.error_heading,
                self.error.as_deref().unwrap_or("请取消后重试。"),
            ),
        };
        let window_id = window.window_handle().window_id();
        let cancel = UiCommand::Permission(window_id, PermissionAction::Cancel);
        let key_cancel = cancel.clone();
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
        // Capture and preference failures share this window but are not
        // permission notices, so their accessible name stays neutral.
        let window_label = if self.notice.is_some() {
            "ArcShot 权限提示"
        } else {
            "ArcShot 提示"
        };
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
            .aria_label(window_label)
            .track_focus(&self.focus)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    queue.borrow_mut().push(key_cancel.clone());
                    cx.stop_propagation();
                }
            })
            .child(div().text_xl().child(text!(id = "notice-title", title)))
            .child(div().flex_1().child(text!(id = "notice-body", body)))
            .child(buttons)
    }
}

/// A button with a custom element id and AX identifier for the Scroll surfaces.
fn scroll_button(
    element_id: String,
    a11y_id: String,
    label: &'static str,
    command: UiCommand,
    queue: &UiQueue,
) -> impl IntoElement {
    let click_queue = Rc::clone(queue);
    let a11y_queue = Rc::clone(queue);
    let click_command = command.clone();
    let a11y_command = command;
    div()
        .id(element_id)
        .role(Role::Button)
        .aria_label(label)
        .accessibility_id(a11y_id.clone())
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .bg(rgb(0x3b465b))
        .hover(|style| style.bg(rgb(0x52617b)))
        .child(label)
        .on_click(move |_, _, _| click_queue.borrow_mut().push(click_command.clone()))
        .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
            log::info!("a11y_action={a11y_id} action=Click");
            a11y_queue.borrow_mut().push(a11y_command.clone());
        })
}

/// The Scroll permission notice: per-permission purpose, matching settings pane,
/// recheck and cancel. A successful re-check shows `Ready`; it never auto-starts.
pub(crate) struct ScrollPermissionView {
    pub(crate) notice: ScrollPermissionNotice,
    pub(crate) focus: FocusHandle,
    pub(crate) queue: UiQueue,
}

impl Render for ScrollPermissionView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let window_id = window.window_handle().window_id();
        let (title, body, missing) = match &self.notice {
            ScrollPermissionNotice::Missing(missing) => (
                "滚动截图权限",
                "滚动截图需要以下权限。请点击对应设置项授权，返回后重新检查，然后再次点击“进入滚动截图”。",
                Some(missing.clone()),
            ),
            ScrollPermissionNotice::Ready => (
                "滚动截图权限已就绪",
                "三项权限均已可用。请重新点击“进入滚动截图”开始。",
                None,
            ),
        };
        let mut rows = div().flex().flex_col().gap_2();
        if let Some(missing) = missing {
            for permission in missing {
                let element_id = format!("scroll-permission-{}", permission.key());
                let a11y_id = format!("arcshot.scroll.permission.{}", permission.key());
                rows = rows.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(0xffffff))
                                        .child(permission.label()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0x9aa4b5))
                                        .child(permission.purpose()),
                                ),
                        )
                        .child(scroll_button(
                            element_id,
                            a11y_id,
                            "打开设置",
                            UiCommand::ScrollPermission(
                                window_id,
                                ScrollPermissionAction::OpenSettings(permission),
                            ),
                            &self.queue,
                        )),
                );
            }
        }
        let mut buttons = div().flex().gap_2();
        if matches!(self.notice, ScrollPermissionNotice::Missing(_)) {
            buttons = buttons
                .child(scroll_button(
                    "scroll-recheck".to_owned(),
                    "arcshot.scroll.recheck".to_owned(),
                    "重新检查",
                    UiCommand::ScrollPermission(window_id, ScrollPermissionAction::Recheck),
                    &self.queue,
                ))
                .child(scroll_button(
                    "scroll-cancel".to_owned(),
                    "arcshot.scroll.cancel".to_owned(),
                    "取消",
                    UiCommand::ScrollPermission(window_id, ScrollPermissionAction::Cancel),
                    &self.queue,
                ));
        } else {
            buttons = buttons.child(scroll_button(
                "scroll-close".to_owned(),
                "arcshot.scroll.close".to_owned(),
                "关闭",
                UiCommand::ScrollPermission(window_id, ScrollPermissionAction::Cancel),
                &self.queue,
            ));
        }
        div()
            .id("scroll-permission-root")
            .size_full()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .bg(rgb(0x20242b))
            .text_color(rgb(0xffffff))
            .text_size(px(15.0))
            .role(Role::Group)
            .aria_label("ArcShot 滚动截图权限")
            .track_focus(&self.focus)
            .on_key_down({
                let queue = Rc::clone(&self.queue);
                move |event, _, cx| {
                    if event.keystroke.key == "escape" {
                        queue.borrow_mut().push(UiCommand::ScrollPermission(
                            window_id,
                            ScrollPermissionAction::Cancel,
                        ));
                        cx.stop_propagation();
                    }
                }
            })
            .child(
                div()
                    .text_xl()
                    .child(text!(id = "scroll-permission-title", title)),
            )
            .child(
                div()
                    .flex_1()
                    .child(text!(id = "scroll-permission-body", body)),
            )
            .child(rows)
            .child(buttons)
    }
}

/// The minimal Scroll control UI that stays visible across preparation and
/// collection and is part of the fixed own-window exclusion set. During
/// collection it shows a concise status plus the operation hint — never a
/// percentage, ETA or pause control — and its cancel button stays operable
/// because the session tap passes events inside its rect through to ArcShot.
pub(crate) struct ScrollControlView {
    pub(crate) status: String,
    pub(crate) hint: Option<String>,
    /// Whether the two explicit partial-result choices are shown instead of
    /// the single cancel button (I-11 step 11).
    pub(crate) partial_choice: bool,
    pub(crate) ticket: CaptureTicket,
    pub(crate) focus: FocusHandle,
    pub(crate) queue: UiQueue,
}

impl Render for ScrollControlView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let ticket = self.ticket;
        let queue = Rc::clone(&self.queue);
        let hint = self.hint.clone();
        div()
            .id("scroll-control")
            .role(Role::Group)
            .aria_label("ArcShot 滚动截图控制")
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .px_5()
            .py_4()
            .rounded_lg()
            .bg(rgba(0x20242bee))
            .track_focus(&self.focus)
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    queue.borrow_mut().push(UiCommand::ScrollCancel(ticket));
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .id("scroll-status")
                    .role(Role::Label)
                    .aria_label(self.status.clone())
                    .accessibility_id("arcshot.scroll.status")
                    .text_color(rgb(0xffffff))
                    .text_sm()
                    .child(self.status.clone()),
            )
            .children(hint.map(|hint| {
                div()
                    .id("scroll-hint")
                    .role(Role::Label)
                    .aria_label(hint.clone())
                    .accessibility_id("arcshot.scroll.hint")
                    .text_color(rgb(0x9aa4b5))
                    .text_xs()
                    .child(hint)
            }))
            .child(if self.partial_choice {
                div()
                    .flex()
                    .gap_2()
                    .child(scroll_button(
                        "scroll-partial-use".to_owned(),
                        "arcshot.scroll.partial.use".to_owned(),
                        PARTIAL_ACCEPT_LABEL,
                        UiCommand::ScrollPartialAccept(ticket),
                        &self.queue,
                    ))
                    .child(scroll_button(
                        "scroll-partial-abandon".to_owned(),
                        "arcshot.scroll.partial.abandon".to_owned(),
                        PARTIAL_ABANDON_LABEL,
                        UiCommand::ScrollPartialAbandon(ticket),
                        &self.queue,
                    ))
            } else {
                div().child(scroll_button(
                    "scroll-control-cancel".to_owned(),
                    "arcshot.scroll.control.cancel".to_owned(),
                    "取消",
                    UiCommand::ScrollCancel(ticket),
                    &self.queue,
                ))
            })
    }
}

/// The fixed live verified preview (I-11 §5).
///
/// One window is opened during preparation and never rebuilt during
/// collection; only the image it presents changes. It ignores mouse events at
/// the native layer, so the pointer over an inner placement still scrolls the
/// target underneath.
pub(crate) struct ScrollPreviewView {
    pub(crate) image: Option<Arc<RenderImage>>,
}

impl ScrollPreviewView {
    /// Replaces the presented verified thumbnail, releasing the previous one.
    pub(crate) fn set_image(&mut self, image: Option<Arc<RenderImage>>, window: &mut Window) {
        if let Some(previous) = self.image.take() {
            let _ = window.drop_image(previous);
        }
        self.image = image;
    }
}

impl Render for ScrollPreviewView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("scroll-preview")
            .role(Role::Image)
            .aria_label("已验证滚动预览")
            .accessibility_id("arcshot.scroll.preview")
            .size_full()
            .overflow_hidden()
            .bg(rgba(0x10141aee))
            .children(self.image.clone().map(|image| {
                img(image)
                    .id("scroll-preview-image")
                    .object_fit(gpui::ObjectFit::Contain)
                    .size_full()
            }))
    }
}

/// The three-second completion thumbnail (I-11 §7).
///
/// It shows the complete long image proportionally, slides in from the right,
/// stays for exactly [`COMPLETION_PREVIEW_HOLD`] (hover never extends it), then
/// slides out and fades. Activating it enters the shared annotation surface.
pub(crate) struct CompletionPreviewView {
    pub(crate) image: Option<Arc<RenderImage>>,
    pub(crate) ticket: CaptureTicket,
    pub(crate) queue: UiQueue,
    /// Thumbnail window width, used as the slide travel distance.
    pub(crate) width: f32,
}

impl Render for CompletionPreviewView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let ticket = self.ticket;
        let queue = Rc::clone(&self.queue);
        let a11y_queue = Rc::clone(&self.queue);
        let travel = self.width;
        let mut root = div()
            .id("completion-preview")
            .role(Role::Button)
            .aria_label("完成预览，点击进入长图标注")
            .accessibility_id("arcshot.completion.preview")
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x20242b))
            .border_1()
            .border_color(rgb(0x007aff))
            .cursor_pointer()
            .on_click(move |_, _, _| {
                queue
                    .borrow_mut()
                    .push(UiCommand::EnterCompletionAnnotation(ticket));
            })
            .on_a11y_action(AccessibleAction::Click, move |_, _, _| {
                log::info!("a11y_action=arcshot.completion.preview action=Click");
                a11y_queue
                    .borrow_mut()
                    .push(UiCommand::EnterCompletionAnnotation(ticket));
            })
            .with_animations(
                "completion-preview-animation",
                vec![
                    Animation::new(COMPLETION_PREVIEW_ENTER).with_easing(gpui::ease_out_quint()),
                    Animation::new(COMPLETION_PREVIEW_HOLD),
                    Animation::new(COMPLETION_PREVIEW_EXIT),
                ],
                move |this, phase, delta| match phase {
                    0 => this.left(px(travel * (1.0 - delta))).opacity(1.0),
                    1 => this.left(px(0.0)).opacity(1.0),
                    _ => this.left(px(travel * delta)).opacity(1.0 - delta),
                },
            );
        if let Some(image) = self.image.clone() {
            root = root.child(
                img(image)
                    .id("completion-preview-image")
                    .size_full()
                    .object_fit(gpui::ObjectFit::Contain),
            );
        }
        root
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
                    let click_command = command.clone();
                    let a11y_command = command;
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
                                queue.borrow_mut().push(click_command.clone());
                            })
                            .on_a11y_action(AccessibleAction::Click, move |_, _, cx| {
                                log::info!("a11y_action=arcshot.hotkey.save action=Click");
                                a11y_entity.update(cx, |view, cx| {
                                    view.recording = false;
                                    cx.notify();
                                });
                                a11y_queue.borrow_mut().push(a11y_command.clone());
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

#[cfg(test)]
mod blur_tooltip_tests {
    use super::BLUR_TOOLTIP;

    #[test]
    fn the_blur_tooltip_states_the_redaction_limitation() {
        assert!(BLUR_TOOLTIP.contains("模糊不保证敏感信息不可读"));
        assert!(BLUR_TOOLTIP.contains("不是可靠的信息遮盖方式"));
    }
}
