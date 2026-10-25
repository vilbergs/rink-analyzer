//! A text field, single- or multi-line, adapted from GPUI's input example.
//! GPUI 0.2 has no text input of its own.
//!
//! The field emits an `InputEvent` on every edit so its owner can apply the
//! text straight to the play. Enter (Cmd-Enter in a multi-line field) and
//! Esc hand focus back to the workspace; Esc first restores the text the
//! field had when it was focused.

use std::ops::Range;

use gpui::{
    actions, div, fill, point, prelude::*, px, relative, rgb, rgba, size, App, AvailableSpace, Bounds, ClipboardItem,
    Context, CursorStyle, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, GlobalElementId, Hsla, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    NoAction, PaintQuad, Pixels, Point, SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle,
    Window, WrappedLine,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Enter,
        Done,
        Cancel,
    ]
);

const CONTEXT: &str = "TextInput";

/// Keys the workspace binds without modifiers. Inside a field they must
/// type, so they're blocked here; a deeper binding wins.
const TYPED_KEYS: [&str; 24] = [
    "space",
    "a",
    "b",
    "d",
    "f",
    "g",
    "n",
    "p",
    "r",
    "shift-r",
    "shift-n",
    "0",
    "?",
    "shift-/",
    "]",
    "tab",
    "shift-tab",
    "alt-up",
    "alt-down",
    "cmd-z",
    "ctrl-z",
    "cmd-shift-z",
    "ctrl-shift-z",
    "ctrl-y",
];

pub fn bind_keys(cx: &mut App) {
    let ctx = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("alt-left", WordLeft, ctx),
        KeyBinding::new("alt-right", WordRight, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("shift-up", SelectUp, ctx),
        KeyBinding::new("shift-down", SelectDown, ctx),
        KeyBinding::new("cmd-a", SelectAll, ctx),
        KeyBinding::new("ctrl-a", SelectAll, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("cmd-left", Home, ctx),
        KeyBinding::new("cmd-right", End, ctx),
        KeyBinding::new("cmd-v", Paste, ctx),
        KeyBinding::new("ctrl-v", Paste, ctx),
        KeyBinding::new("cmd-x", Cut, ctx),
        KeyBinding::new("ctrl-x", Cut, ctx),
        KeyBinding::new("cmd-c", Copy, ctx),
        KeyBinding::new("ctrl-c", Copy, ctx),
        KeyBinding::new("enter", Enter, ctx),
        KeyBinding::new("cmd-enter", Done, ctx),
        KeyBinding::new("ctrl-enter", Done, ctx),
        KeyBinding::new("escape", Cancel, ctx),
    ]);
    cx.bind_keys(TYPED_KEYS.map(|k| KeyBinding::new(k, NoAction, ctx)));
}

pub struct InputEvent {
    /// The whole new text.
    pub text: String,
    /// The first change since the field was entered: owners record an undo
    /// step for it, so one visit to a field is one step.
    pub first: bool,
}

pub struct TextInput {
    focus_handle: FocusHandle,
    /// Where focus goes when editing ends.
    return_focus: FocusHandle,
    content: String,
    placeholder: SharedString,
    multiline: bool,
    /// The text before the first change of this visit to the field,
    /// restored by Esc. None until something changes.
    original: Option<String>,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    is_selecting: bool,
    layout: Option<TextLayout>,
}

/// The last painted text: each paragraph's wrapped line and where it starts.
#[derive(Clone)]
struct TextLayout {
    lines: Vec<(usize, WrappedLine)>,
    origin: Point<Pixels>,
    width: Pixels,
    line_height: Pixels,
}

impl TextLayout {
    /// Top-left of each paragraph, relative to the field's origin.
    fn tops(&self) -> impl Iterator<Item = (Pixels, usize, &WrappedLine)> {
        let mut y = px(0.);
        self.lines.iter().map(move |(start, line)| {
            let top = y;
            y += line.size(self.line_height).height;
            (top, *start, line)
        })
    }

    fn position_for(&self, offset: usize) -> Point<Pixels> {
        let mut found = point(px(0.), px(0.));
        for (top, start, line) in self.tops() {
            if offset < start {
                break;
            }
            let local = (offset - start).min(line.len());
            if let Some(p) = line.position_for_index(local, self.line_height) {
                found = point(p.x, top + p.y);
            }
        }
        found
    }

    fn index_for(&self, at: Point<Pixels>) -> usize {
        let mut last = 0;
        for (top, start, line) in self.tops() {
            let bottom = top + line.size(self.line_height).height;
            last = start + line.len();
            if at.y < bottom {
                let local = point(at.x.max(px(0.)), (at.y - top).max(px(0.)));
                let i = match line.closest_index_for_position(local, self.line_height) {
                    Ok(i) | Err(i) => i,
                };
                return start + i.min(line.len());
            }
        }
        last
    }
}

impl EventEmitter<InputEvent> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TextInput {
    pub fn new(
        placeholder: impl Into<SharedString>,
        multiline: bool,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_blur(&focus_handle, window, |this, _, cx| {
            this.original = None;
            this.selected_range = this.content.len()..this.content.len();
            cx.notify();
        })
        .detach();
        TextInput {
            focus_handle,
            return_focus,
            content: String::new(),
            placeholder: placeholder.into(),
            multiline,
            original: None,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            is_selecting: false,
            layout: None,
        }
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    /// Show `text`, unless the user is typing in the field.
    pub fn sync(&mut self, text: &str, window: &Window) {
        if !self.is_focused(window) && self.content != text {
            self.content = text.to_string();
            self.selected_range = self.content.len()..self.content.len();
            self.marked_range = None;
        }
    }

    fn set_content(&mut self, text: String, cx: &mut Context<Self>) {
        let first = self.original.is_none();
        if first {
            self.original = Some(std::mem::take(&mut self.content));
        }
        self.content = text;
        cx.emit(InputEvent { text: self.content.clone(), first });
        cx.notify();
    }

    fn finish(&mut self, window: &mut Window) {
        self.original = None;
        window.focus(&self.return_focus);
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
        } else {
            self.finish(window);
        }
    }

    fn done(&mut self, _: &Done, window: &mut Window, _: &mut Context<Self>) {
        self.finish(window);
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(original) = self.original.clone().filter(|o| *o != self.content) {
            self.set_content(original, cx);
        }
        self.finish(window);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical(-1.0), cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical(1.0), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical(-1.0), cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical(1.0), cx);
    }

    /// The offset one visual row up (dir < 0) or down from the cursor.
    fn vertical(&self, dir: f32) -> usize {
        let cursor = self.cursor_offset();
        let Some(layout) = &self.layout else { return cursor };
        if !self.multiline {
            return if dir < 0.0 { 0 } else { self.content.len() };
        }
        let p = layout.position_for(cursor);
        let y = p.y + layout.line_height * (0.5 + dir);
        if y < px(0.) {
            return 0;
        }
        layout.index_for(point(p.x, y)).min(self.content.len())
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let at = self.cursor_offset();
        let to = self.content.unicode_word_indices().rev().map(|(i, _)| i).find(|&i| i < at).unwrap_or(0);
        self.move_to(to, cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        let at = self.cursor_offset();
        let to = self
            .content
            .unicode_word_indices()
            .map(|(i, w)| i + w.len())
            .find(|&end| end > at)
            .unwrap_or(self.content.len());
        self.move_to(to, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    /// Start of the line (or the field, single-line).
    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let at = self.cursor_offset();
        let start = if self.multiline { self.content[..at].rfind('\n').map_or(0, |i| i + 1) } else { 0 };
        self.move_to(start, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let at = self.cursor_offset();
        let end = if self.multiline {
            self.content[at..].find('\n').map_or(self.content.len(), |i| at + i)
        } else {
            self.content.len()
        };
        self.move_to(end, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = if self.multiline { text } else { text.replace('\n', " ") };
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[self.selected_range.clone()].to_string()));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[self.selected_range.clone()].to_string()));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if !self.is_focused(window) {
            window.focus(&self.focus_handle);
        }
        self.is_selecting = true;
        let at = self.index_for_mouse(ev.position);
        if ev.click_count >= 2 {
            self.select_word_at(at, cx);
        } else if ev.modifiers.shift {
            self.select_to(at, cx);
        } else {
            self.move_to(at, cx)
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting && ev.pressed_button == Some(MouseButton::Left) {
            self.select_to(self.index_for_mouse(ev.position), cx);
        }
    }

    fn select_word_at(&mut self, at: usize, cx: &mut Context<Self>) {
        let word = self.content.unicode_word_indices().find(|(i, w)| *i <= at && at <= i + w.len());
        if let Some((start, end)) = word.map(|(i, w)| (i, i + w.len())) {
            self.move_to(start, cx);
            self.select_to(end, cx);
        }
    }

    fn index_for_mouse(&self, position: Point<Pixels>) -> usize {
        match &self.layout {
            Some(layout) if !self.content.is_empty() => layout.index_for(position - layout.origin),
            _ => 0,
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        let mut utf8 = 0;
        for ch in self.content.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content.grapheme_indices(true).rev().find_map(|(i, _)| (i < offset).then_some(i)).unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content.grapheme_indices(true).find_map(|(i, _)| (i > offset).then_some(i)).unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: self.range_to_utf16(&self.selected_range), reversed: self.selection_reversed })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let text = format!("{}{new_text}{}", &self.content[..range.start], &self.content[range.end..]);
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.set_content(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let text = format!("{}{new_text}{}", &self.content[..range.start], &self.content[range.end..]);
        self.marked_range = (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|r| r.start + range.start..r.end + range.start)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.set_content(text, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let start = layout.origin + layout.position_for(range.start);
        let end = layout.origin + layout.position_for(range.end);
        Some(Bounds::from_corners(start, point(end.x.max(start.x), start.y + layout.line_height)))
    }

    fn character_index_for_point(&mut self, at: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        Some(self.offset_to_utf16(layout.index_for(at - layout.origin)))
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.is_focused(window);
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::done))
            .on_action(cx.listener(Self::cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .w_full()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(if focused { theme::ACCENT } else { theme::BORDER }))
            .bg(rgb(if focused { theme::APP_BG } else { theme::RAISED }))
            .hover(|s| s.border_color(rgb(if focused { theme::ACCENT } else { theme::BORDER_STRONG })))
            .text_sm()
            .text_color(rgb(theme::TEXT))
            .child(TextElement { input: cx.entity() })
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct Prepaint {
    layout: TextLayout,
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The text to show and its runs: the content (with IME marking) or the
/// placeholder in a faint color.
fn display(input: &TextInput, window: &Window) -> (SharedString, Vec<TextRun>) {
    let style = window.text_style();
    let (text, color): (SharedString, Hsla) = if input.content.is_empty() {
        (input.placeholder.clone(), rgb(theme::TEXT_FAINT).into())
    } else {
        (input.content.clone().into(), style.color)
    };
    let run = TextRun {
        len: text.len(),
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let runs = match input.marked_range.as_ref().filter(|_| !input.content.is_empty()) {
        Some(marked) => vec![
            TextRun { len: marked.start, ..run.clone() },
            TextRun {
                len: marked.end - marked.start,
                underline: Some(UnderlineStyle { color: Some(run.color), thickness: px(1.0), wavy: false }),
                ..run.clone()
            },
            TextRun { len: text.len() - marked.end, ..run },
        ]
        .into_iter()
        .filter(|r| r.len > 0)
        .collect(),
        None => vec![run],
    };
    (text, runs)
}

fn shape(text: SharedString, runs: &[TextRun], width: Option<Pixels>, window: &Window) -> Vec<(usize, WrappedLine)> {
    let font_size = window.text_style().font_size.to_pixels(window.rem_size());
    let lines = window.text_system().shape_text(text, font_size, runs, width, None).unwrap_or_default();
    let mut start = 0;
    lines
        .into_iter()
        .map(|line| {
            let at = start;
            start += line.len() + 1;
            (at, line)
        })
        .collect()
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
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let input = self.input.read(cx);
        let line_height = window.line_height();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        if !input.multiline {
            style.size.height = line_height.into();
            return (window.request_layout(style, [], cx), ());
        }
        let (text, runs) = display(input, window);
        let layout = window.request_measured_layout(style, move |known, available, window, _| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            let height = shape(text.clone(), &runs, width, window)
                .iter()
                .map(|(_, l)| l.size(line_height).height)
                .fold(px(0.), |a, b| a + b);
            size(width.unwrap_or(px(200.)), height.max(line_height * 2.))
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let input = self.input.read(cx);
        let (text, runs) = display(input, window);
        let width = input.multiline.then_some(bounds.size.width);
        let layout = TextLayout {
            lines: shape(text, &runs, width, window),
            origin: bounds.origin,
            width: bounds.size.width,
            line_height: window.line_height(),
        };
        let range = input.selected_range.clone();
        let lh = layout.line_height;
        let (cursor, selection) = if range.is_empty() {
            let p = bounds.origin + layout.position_for(input.cursor_offset());
            (Some(fill(Bounds::new(p, size(px(1.5), lh)), rgb(theme::ACCENT))), Vec::new())
        } else {
            let (a, b) = (layout.position_for(range.start), layout.position_for(range.end));
            let tint = rgba((theme::ACCENT << 8) | 0x55);
            let rect = |x0: Pixels, y: Pixels, x1: Pixels| {
                fill(Bounds::from_corners(bounds.origin + point(x0, y), bounds.origin + point(x1, y + lh)), tint)
            };
            let mut quads = Vec::new();
            if a.y == b.y {
                quads.push(rect(a.x, a.y, b.x));
            } else {
                quads.push(rect(a.x, a.y, layout.width));
                let mut y = a.y + lh;
                while y < b.y {
                    quads.push(rect(px(0.), y, layout.width));
                    y += lh;
                }
                quads.push(rect(px(0.), b.y, b.x));
            }
            (None, quads)
        };
        Prepaint { layout, cursor, selection }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(&focus_handle, ElementInputHandler::new(bounds, self.input.clone()), cx);
        let focused = focus_handle.is_focused(window);
        if focused {
            for quad in prepaint.selection.drain(..) {
                window.paint_quad(quad);
            }
        }
        let layout = &prepaint.layout;
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for (top, _, line) in layout.tops() {
                line.paint(bounds.origin + point(px(0.), top), layout.line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
        });
        if focused {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        let layout = prepaint.layout.clone();
        self.input.update(cx, |input, _| input.layout = Some(layout));
    }
}
