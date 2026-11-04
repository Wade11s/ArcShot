//! IME-capable multi-line text input used by the overlay text editor.
//!
//! The entity owns the live draft content, the selection and the marked IME
//! range. Every change is pushed to the authoritative document as a
//! `TextEdited` command, so Finish and Save always see the latest composed
//! text. Rendering uses the shared GPUI text system; the committed rasterizer
//! is the image compositor's font input.
use crate::ui::{UiCommand, UiQueue};
use arcshot_core::{CaptureTicket, Selection};
use gpui::{
    App, Bounds, Context, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, Pixels, Point, Role, ShapedLine, SharedString,
    TextRun, UTF16Selection, Window, div, fill, point, prelude::*, px, size,
};
use std::ops::Range;

/// One live text draft with its selection and IME marked range.
pub(crate) struct TextInput {
    pub(crate) content: String,
    pub(crate) selected_range: Range<usize>,
    pub(crate) marked_range: Option<Range<usize>>,
    pub(crate) size_points: f64,
    anchor: [f64; 2],
    focus_handle: FocusHandle,
    queue: UiQueue,
    ticket: CaptureTicket,
}

impl TextInput {
    pub(crate) fn new(
        content: impl Into<String>,
        size_points: f64,
        anchor: [f64; 2],
        focus_handle: FocusHandle,
        queue: UiQueue,
        ticket: CaptureTicket,
    ) -> Self {
        let content = content.into();
        Self {
            selected_range: content.len()..content.len(),
            content,
            marked_range: None,
            size_points,
            anchor,
            focus_handle,
            queue,
            ticket,
        }
    }

    pub(crate) fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let metrics = arcshot_image::measure_text(&self.content, self.size_points);
        let width = metrics.max_line_width;
        let height = metrics.line_count as f64 * metrics.line_height;
        let bounds = Selection::try_new(
            self.anchor[0],
            self.anchor[1],
            width.max(1.0),
            height.max(1.0),
        )
        .unwrap_or_else(|_| {
            Selection::try_new(self.anchor[0], self.anchor[1], 1.0, 1.0)
                .expect("a unit box at a finite anchor is valid")
        });
        self.queue.borrow_mut().push(UiCommand::TextEdited {
            ticket: self.ticket,
            content: self.content.clone(),
            bounds,
        });
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace(
        &mut self,
        range: Range<usize>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());
        self.content.replace_range(start..end, text);
        let cursor = start + text.len();
        self.selected_range = cursor..cursor;
        self.marked_range = None;
        self.sync(window, cx);
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.content[..offset.min(self.content.len())]
            .encode_utf16()
            .count()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (byte, character) in self.content.char_indices() {
            if utf16 >= offset {
                return byte;
            }
            utf16 += character.len_utf16();
        }
        self.content.len()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: false,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_range = self.range_from_utf16(&range_utf16);
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.content.encode_utf16().count())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        self.replace(range, new_text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());
        self.content.replace_range(start..end, new_text);
        self.marked_range = if new_text.is_empty() {
            None
        } else {
            Some(start..start + new_text.len())
        };
        self.selected_range = new_selected_range_utf16
            .map(|selected| self.range_from_utf16(&selected))
            .unwrap_or(start + new_text.len()..start + new_text.len());
        self.sync(window, cx);
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(element_bounds)
    }

    fn selection_bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        vec![element_bounds]
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.selected_range.start)
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// A minimal multi-line, non-wrapping text element that attaches the input
/// handler and paints the live draft with a cursor.
pub(crate) struct TextElement {
    pub(crate) input: Entity<TextInput>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

pub(crate) struct Prepaint {
    lines: Vec<ShapedLine>,
    cursor_line: usize,
    cursor_x: Pixels,
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let metrics = arcshot_image::measure_text(&input.content, input.size_points);
        let width = (metrics.max_line_width as f32).max(1.0);
        let height = (metrics.line_count as f32 * metrics.line_height as f32).max(1.0);
        let mut style = gpui::Style::default();
        style.size.width = px(width).into();
        style.size.height = px(height).into();
        let layout = window.request_layout(style, [], cx);
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let font_size = px(input.size_points as f32);
        let cursor_byte = input.selected_range.start;
        let mut lines = Vec::new();
        let mut cursor_line = 0;
        let mut cursor_line_start = 0;
        let mut offset = 0;
        for (index, line) in input.content.split('\n').enumerate() {
            let line_end = offset + line.len();
            if cursor_byte <= line_end {
                cursor_line = index;
                cursor_line_start = offset;
                break;
            }
            offset = line_end + 1;
            cursor_line = index + 1;
            cursor_line_start = offset;
        }
        for line in input.content.split('\n') {
            let run = TextRun {
                len: line.len(),
                font: window.text_style().font(),
                color: window.text_style().color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(line.to_owned()),
                font_size,
                &[run],
                None,
            );
            lines.push(shaped);
        }
        let cursor_byte_in_line = cursor_byte.saturating_sub(cursor_line_start).min(
            input
                .content
                .split('\n')
                .nth(cursor_line)
                .map_or(0, |line| line.len()),
        );
        let cursor_x = lines
            .get(cursor_line)
            .map_or(px(0.), |line| line.x_for_index(cursor_byte_in_line));
        Prepaint {
            lines,
            cursor_line,
            cursor_x,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let line_height = window.line_height();
        for (index, line) in prepaint.lines.iter().enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * index as f32);
            let _ = line.paint(origin, line_height, gpui::TextAlign::Left, None, window, cx);
        }
        if focus.is_focused(window) {
            let cursor_bounds = Bounds::new(
                point(
                    bounds.left() + prepaint.cursor_x,
                    bounds.top() + line_height * prepaint.cursor_line as f32,
                ),
                size(px(2.), line_height),
            );
            window.paint_quad(fill(cursor_bounds, gpui::rgb(0xff3b30)));
        }
    }
}

/// A focusable editor shell used by the overlay.
pub(crate) fn text_editor(
    input: Entity<TextInput>,
    anchor: [f64; 2],
    bounds: arcshot_core::Selection,
) -> impl IntoElement {
    div()
        .id("arcshot.overlay.text.input")
        .role(Role::Button)
        .aria_label("文字输入")
        .accessibility_id("arcshot.overlay.text.input")
        .absolute()
        .left(px(anchor[0] as f32))
        .top(px(anchor[1] as f32))
        .w(px(bounds.width() as f32))
        .h(px(bounds.height() as f32))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
            // Clicks inside the editor must not commit it as "clicked elsewhere".
            cx.stop_propagation();
        })
        .child(TextElement { input })
}
