use crate::backend::terminal::{EventSink, TerminalEvent, TerminalInfo};
use crate::state::app_state::{AppState, RightPaneTabId};
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_mini_icon_btn, BadgeTone, BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::{SandTheme, ThemeMode};
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::{Config as TermConfig, TermMode};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Processor};
use alacritty_terminal::Term;
use base64::{engine::general_purpose::STANDARD, Engine};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::scroll::{ScrollableElement, ScrollbarHandle};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, Notify};

#[derive(Clone, Copy)]
struct TermDims {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermDims {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

struct TermSessionRuntime {
    term: Term<VoidListener>,
    parser: Processor,
    generation: String,
    exit_code: Option<u32>,
    error: Option<String>,
    dirty: bool,
}

struct TermTab {
    id: String,
    title: String,
    cwd: String,
    shell: String,
    remote: bool,
    cols: u16,
    rows: u16,
    runtime: Arc<Mutex<TermSessionRuntime>>,
}

const TERMINAL_LINE_HEIGHT: f32 = 16.5;
const TERMINAL_FONT_SIZE: f32 = 12.0;

#[derive(Clone)]
struct TerminalScrollHandle {
    bounds: Rc<Cell<Bounds<Pixels>>>,
    runtime: Arc<Mutex<TermSessionRuntime>>,
}

impl ScrollbarHandle for TerminalScrollHandle {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.bounds.get()
    }

    fn offset(&self) -> Point<Pixels> {
        let y = self
            .runtime
            .lock()
            .map(|runtime| {
                let grid = runtime.term.grid();
                -px((grid.history_size() - grid.display_offset()) as f32 * TERMINAL_LINE_HEIGHT)
            })
            .unwrap_or_default();
        point(px(0.0), y)
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        if let Ok(mut runtime) = self.runtime.lock() {
            let grid = runtime.term.grid();
            let history = grid.history_size() as i32;
            let from_top = (-f32::from(offset.y) / TERMINAL_LINE_HEIGHT).round() as i32;
            let delta = (history - from_top).clamp(0, history) - grid.display_offset() as i32;
            runtime.term.scroll_display(Scroll::Delta(delta));
        }
    }

    fn content_size(&self) -> Size<Pixels> {
        let viewport = self.bounds.get().size;
        let history = self
            .runtime
            .lock()
            .map(|runtime| runtime.term.grid().history_size())
            .unwrap_or(0);
        size(
            viewport.width,
            viewport.height + px(history as f32 * TERMINAL_LINE_HEIGHT),
        )
    }
}

pub struct TerminalView {
    app: Entity<AppState>,
    focus_handle: FocusHandle,
    tabs: Vec<TermTab>,
    active_tab_id: Option<String>,
    tab_counter: usize,
    wake_notify: Arc<Notify>,
    _wake_task: Task<()>,
    write_queue: mpsc::Sender<(String, Vec<u8>)>,
    _write_task: Task<()>,
    surface_bounds: Rc<Cell<Bounds<Pixels>>>,
    wheel_pixels: f32,
}

impl TerminalView {
    fn is_terminal_visible(app: &AppState) -> bool {
        !app.setup_open
            && !app.initializing
            && app.right_pane_open
            && app.right_pane_tab == RightPaneTabId::Terminal
    }

    pub fn new(app: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let initial_state = {
            let s = app.read(cx);
            (Self::is_terminal_visible(s), s.theme_mode())
        };
        let mut last_state = initial_state;
        cx.observe(&app, move |_, app, cx| {
            let next = {
                let s = app.read(cx);
                (Self::is_terminal_visible(s), s.theme_mode())
            };
            if next != last_state {
                last_state = next;
                if next.0 {
                    cx.notify();
                }
            }
        })
        .detach();

        let (write_queue, mut writes) = mpsc::channel::<(String, Vec<u8>)>(128);
        let terminals = app.read(cx).terminals.clone();
        let rt = app.read(cx).tokio_rt.clone();
        let write_task = cx.spawn(async move |this, cx| {
            while let Some((id, bytes)) = writes.recv().await {
                let terminals = terminals.clone();
                let result = rt
                    .spawn_blocking(move || terminals.write(&id, &bytes))
                    .await
                    .unwrap_or_else(|error| Err(error.to_string()));
                if let Err(error) = result {
                    let _ = this.update(cx, |view, cx| {
                        view.app.update(cx, |app, cx| app.show_toast(error, cx));
                    });
                }
            }
        });

        // Event-driven 120fps wake task: sleeps at 0% CPU when idle, wakes immediately on PTY
        // output and coalesces rapid bursts at ~8ms (120Hz) frame pacing just like Zed's terminal!
        let wake_notify = Arc::new(Notify::new());
        let wake_rx = wake_notify.clone();
        let wake_task = cx.spawn(async move |this, cx| {
            loop {
                wake_rx.notified().await;
                // Coalesce bursty PTY reads over one 120Hz frame window (8ms)
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(8))
                    .await;
                let keep_going = this
                    .update(cx, |v, cx| {
                        if !Self::is_terminal_visible(v.app.read(cx)) {
                            return;
                        }
                        let mut any_dirty = false;
                        for tab in &v.tabs {
                            if let Ok(mut rt) = tab.runtime.lock() {
                                if rt.dirty {
                                    rt.dirty = false;
                                    any_dirty = true;
                                }
                            }
                        }
                        if any_dirty {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !keep_going {
                    break;
                }
            }
        });

        let mut view = Self {
            app,
            focus_handle,
            tabs: Vec::new(),
            active_tab_id: None,
            tab_counter: 0,
            wake_notify,
            _wake_task: wake_task,
            write_queue,
            _write_task: write_task,
            surface_bounds: Rc::new(Cell::new(Bounds::default())),
            wheel_pixels: 0.0,
        };
        view.open_new_tab(cx);
        view
    }

    fn open_new_tab(&mut self, cx: &mut Context<Self>) {
        self.tab_counter += 1;
        let id = format!("term-{}", uuid::Uuid::new_v4().simple());
        let (cwd, ssh, terminals) = {
            let s = self.app.read(cx);
            let cwd = s.inspector_cwd.clone();
            let ssh = if s.connection.is_local() {
                None
            } else {
                s.connection.ssh.clone()
            };
            (cwd, ssh, s.terminals.clone())
        };

        let cols: u16 = 80;
        let rows: u16 = 28;
        let dims = TermDims {
            cols: cols as usize,
            rows: rows as usize,
        };
        let term = Term::new(TermConfig::default(), &dims, VoidListener);
        let runtime = Arc::new(Mutex::new(TermSessionRuntime {
            term,
            parser: Processor::new(),
            generation: String::new(),
            exit_code: None,
            error: None,
            dirty: true,
        }));

        let rt_sink = runtime.clone();
        let terminals_ack = terminals.clone();
        let id_ack = id.clone();
        let wake = self.wake_notify.clone();
        let sink = EventSink::new(move |event| {
            let Ok(mut rt) = rt_sink.lock() else {
                return Err(());
            };
            match event {
                TerminalEvent::Started { generation, .. } => {
                    rt.generation = generation;
                    rt.exit_code = None;
                    rt.error = None;
                    rt.dirty = true;
                    wake.notify_one();
                }
                TerminalEvent::Output { data } => {
                    if let Ok(bytes) = STANDARD.decode(&data) {
                        let len = bytes.len();
                        let gen_copy = rt.generation.clone();
                        let TermSessionRuntime {
                            ref mut term,
                            ref mut parser,
                            ..
                        } = *rt;
                        parser.advance(term, &bytes);
                        rt.dirty = true;
                        drop(rt);
                        wake.notify_one();
                        if !gen_copy.is_empty() {
                            terminals_ack.acknowledge(&id_ack, &gen_copy, len);
                        }
                        return Ok(());
                    }
                }
                TerminalEvent::Exit { code } => {
                    rt.exit_code = Some(code);
                    rt.dirty = true;
                    wake.notify_one();
                }
                TerminalEvent::Error { message } => {
                    rt.error = Some(message);
                    rt.dirty = true;
                    wake.notify_one();
                }
            }
            Ok(())
        });

        let (shell, actual_cwd, remote) =
            match terminals.start(id.clone(), cwd.clone(), cols, rows, ssh, sink) {
                Ok(TerminalInfo {
                    shell, cwd, remote, ..
                }) => (shell, cwd, remote),
                Err(err) => {
                    if let Ok(mut rt) = runtime.lock() {
                        rt.error = Some(err);
                    }
                    ("shell".to_string(), cwd, false)
                }
            };

        let title = if remote {
            format!("SSH {}", self.tab_counter)
        } else {
            format!("{shell} {}", self.tab_counter)
        };

        self.tabs.push(TermTab {
            id: id.clone(),
            title,
            cwd: actual_cwd,
            shell,
            remote,
            cols,
            rows,
            runtime,
        });
        self.active_tab_id = Some(id);
        cx.notify();
    }

    fn close_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        self.app.read(cx).terminals.close(id);
        self.tabs.retain(|t| t.id != id);
        if self.active_tab_id.as_deref() == Some(id) {
            self.active_tab_id = self.tabs.last().map(|t| t.id.clone());
        }
        cx.notify();
    }

    fn restart_active_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.active_tab_id.clone() {
            self.close_tab(&id, cx);
        }
        self.open_new_tab(cx);
    }

    fn write_active(&self, data: &[u8], cx: &mut Context<Self>) {
        if let Some(id) = &self.active_tab_id {
            let error = if data.len() > 1024 * 1024 {
                Some("Paste is too large (1 MB maximum)")
            } else if self
                .write_queue
                .try_send((id.clone(), data.to_vec()))
                .is_err()
            {
                Some("Terminal input is busy. Try again.")
            } else {
                None
            };
            if let Some(error) = error {
                self.app.update(cx, |app, cx| app.show_toast(error, cx));
            }
        }
    }

    fn scroll_history(&self, scroll: Scroll, cx: &mut Context<Self>) -> bool {
        let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == self.active_tab_id.as_ref())
        else {
            return false;
        };
        let Ok(mut runtime) = tab.runtime.lock() else {
            return false;
        };
        let before = runtime.term.grid().display_offset();
        runtime.term.scroll_display(scroll);
        if runtime.term.grid().display_offset() != before {
            cx.notify();
            true
        } else {
            false
        }
    }

    fn scroll_terminal(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let delta = f32::from(event.delta.pixel_delta(px(TERMINAL_LINE_HEIGHT)).y);
        if delta == 0.0 {
            return;
        }
        if self.wheel_pixels.signum() != delta.signum() {
            self.wheel_pixels = 0.0;
        }
        self.wheel_pixels += delta;
        let rows = (self.wheel_pixels / TERMINAL_LINE_HEIGHT).trunc() as i32;
        if rows == 0 {
            return;
        }
        self.wheel_pixels -= rows as f32 * TERMINAL_LINE_HEIGHT;
        let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| Some(&tab.id) == self.active_tab_id.as_ref())
        else {
            return;
        };
        let (mode, cols, height) = match tab.runtime.lock() {
            Ok(runtime) => (*runtime.term.mode(), tab.cols, tab.rows),
            Err(_) => return,
        };
        if !event.modifiers.shift && mode.intersects(TermMode::MOUSE_MODE) {
            let bounds = self.surface_bounds.get();
            let width = f32::from((bounds.size.width - px(22.0)).max(px(1.0)));
            let relative = event.position - bounds.origin - point(px(11.0), px(11.0));
            let x = (f32::from(relative.x) / (width / f32::from(cols))).floor() as i32 + 1;
            let y = (f32::from(relative.y) / TERMINAL_LINE_HEIGHT).floor() as i32 + 1;
            let x = x.clamp(1, i32::from(cols));
            let y = y.clamp(1, i32::from(height));
            let button = if rows > 0 { 64 } else { 65 };
            let report = if mode.contains(TermMode::SGR_MOUSE) {
                format!("\x1b[<{button};{x};{y}M").into_bytes()
            } else {
                vec![
                    0x1b,
                    b'[',
                    b'M',
                    button as u8 + 32,
                    x.min(223) as u8 + 32,
                    y.min(223) as u8 + 32,
                ]
            };
            self.write_active(&report.repeat(rows.unsigned_abs().min(20) as usize), cx);
            cx.stop_propagation();
        } else if self.scroll_history(Scroll::Delta(rows), cx) {
            cx.stop_propagation();
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        let mods = &ks.modifiers;

        if mods.shift && !mods.alt && !mods.control && !mods.platform {
            let scroll = match ks.key.as_str() {
                "pageup" => Some(Scroll::PageUp),
                "pagedown" => Some(Scroll::PageDown),
                _ => None,
            };
            if let Some(scroll) = scroll {
                self.scroll_history(scroll, cx);
                cx.stop_propagation();
                return;
            }
        }

        // Copy / Paste shortcuts
        if (mods.control && mods.shift) || mods.platform {
            if ks.key.eq_ignore_ascii_case("v") {
                if let Some(item) = cx.read_from_clipboard() {
                    if let Some(text) = item.text() {
                        self.write_active(text.as_bytes(), cx);
                    }
                }
                cx.stop_propagation();
                return;
            }
            if ks.key.eq_ignore_ascii_case("c") {
                if let Some(text) = self.extract_visible_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    self.app
                        .update(cx, |s, cx| s.show_toast("Copied terminal screen", cx));
                }
                cx.stop_propagation();
                return;
            }
        }

        // Ctrl+<letter> control sequences
        if mods.control && !mods.alt && !mods.platform && ks.key.len() == 1 {
            let b = ks.key.as_bytes()[0].to_ascii_lowercase();
            if b.is_ascii_lowercase() {
                let ctrl_byte = b - b'a' + 1;
                self.write_active(&[ctrl_byte], cx);
                cx.stop_propagation();
                return;
            }
        }

        let seq: Option<&[u8]> = match ks.key.as_str() {
            "enter" => Some(b"\r"),
            "backspace" => Some(b"\x7f"),
            "tab" => Some(if mods.shift { b"\x1b[Z" } else { b"\t" }),
            "escape" => Some(b"\x1b"),
            "up" => Some(b"\x1b[A"),
            "down" => Some(b"\x1b[B"),
            "right" => Some(b"\x1b[C"),
            "left" => Some(b"\x1b[D"),
            "home" => Some(b"\x1b[H"),
            "end" => Some(b"\x1b[F"),
            "pageup" => Some(b"\x1b[5~"),
            "pagedown" => Some(b"\x1b[6~"),
            "delete" => Some(b"\x1b[3~"),
            "space" => Some(b" "),
            _ => None,
        };

        if let Some(bytes) = seq {
            self.write_active(bytes, cx);
            cx.stop_propagation();
            return;
        }

        if let Some(key_char) = &ks.key_char {
            if !key_char.is_empty() && !mods.control && !mods.platform {
                if mods.alt {
                    let mut buf = vec![0x1b];
                    buf.extend_from_slice(key_char.as_bytes());
                    self.write_active(&buf, cx);
                } else {
                    self.write_active(key_char.as_bytes(), cx);
                }
                cx.stop_propagation();
            }
        }
    }

    fn extract_visible_text(&self) -> Option<String> {
        let active_id = self.active_tab_id.as_deref()?;
        let tab = self.tabs.iter().find(|t| t.id == active_id)?;
        let rt = tab.runtime.lock().ok()?;
        let grid = rt.term.grid();
        let rows = tab.rows as usize;
        let cols = tab.cols as usize;
        let mut lines = Vec::with_capacity(rows);
        let display_offset = grid.display_offset() as i32;
        for r in 0..rows {
            let mut s = String::with_capacity(cols);
            for c in 0..cols {
                let cell = &grid[Line(r as i32 - display_offset)][Column(c)];
                s.push(cell.c);
            }
            lines.push(s.trim_end().to_string());
        }
        Some(lines.join("\n").trim_end().to_string())
    }

    fn resize_active_grid(&mut self, cols: u16, rows: u16, cx: &Context<Self>) {
        let Some(active_id) = self.active_tab_id.clone() else {
            return;
        };
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == active_id) {
            if tab.cols == cols && tab.rows == rows {
                return;
            }
            tab.cols = cols;
            tab.rows = rows;
            if let Ok(mut rt) = tab.runtime.lock() {
                rt.term.resize(TermDims {
                    cols: cols as usize,
                    rows: rows as usize,
                });
            }
            let app = self.app.read(cx);
            let terminals = app.terminals.clone();
            app.tokio_rt.spawn_blocking(move || {
                let _ = terminals.resize(&active_id, cols, rows);
            });
        }
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (theme, mode) = {
            let s = self.app.read(cx);
            (s.sand_theme(), s.theme_mode())
        };

        let term_bg = match mode {
            ThemeMode::Light => hsla(40.0 / 360.0, 0.20, 0.97, 1.0),
            ThemeMode::Dark => rgb(0x131312).into(),
        };
        let default_fg: Hsla = match mode {
            ThemeMode::Light => rgb(0x1d1d1c).into(),
            ThemeMode::Dark => rgb(0xebe9e6).into(),
        };
        let cursor_bg: Hsla = theme.agent_accent;
        let is_focused = self.focus_handle.is_focused(window);

        let active_tab = self
            .active_tab_id
            .as_deref()
            .and_then(|id| self.tabs.iter().find(|t| t.id == id));

        let scroll_handle = active_tab.map(|tab| TerminalScrollHandle {
            bounds: self.surface_bounds.clone(),
            runtime: tab.runtime.clone(),
        });
        let active_dimensions = active_tab.map(|tab| (tab.cols, tab.rows));
        let active_id = self.active_tab_id.clone();
        let bounds = self.surface_bounds.clone();
        let terminal = cx.weak_entity();
        let text_system = window.text_system();
        let font = text_system.resolve_font(&font(FONT_MONO));
        let char_width = text_system.advance(font, px(TERMINAL_FONT_SIZE), 'M')
            .map(|size| f32::from(size.width)).unwrap_or(7.2).max(1.0);

        let (rendered_rows, meta_shell, meta_cwd, meta_remote, exit_code, error_msg) =
            if let Some(tab) = active_tab {
                let mut rows_out: Vec<StyledText> = Vec::with_capacity(tab.rows as usize);
                let mut exit_c = None;
                let mut err_m = None;
                if let Ok(rt) = tab.runtime.lock() {
                    exit_c = rt.exit_code;
                    err_m = rt.error.clone();
                    let grid = rt.term.grid();
                    let cursor_pt = grid.cursor.point;
                    let display_offset = grid.display_offset() as i32;
                    let rows = tab.rows as usize;
                    let cols = tab.cols as usize;
                    let screen: Vec<_> = (0..rows)
                        .flat_map(|row| {
                            (0..cols).map(move |col| {
                                let cell = &grid[Line(row as i32 - display_offset)][Column(col)];
                                (cell.c, cell.flags, cell.fg, cell.bg)
                            })
                        })
                        .collect();
                    drop(rt);

                    for (r, row) in screen.chunks(cols).enumerate() {
                        let mut line_str = String::with_capacity(cols);
                        let mut highlights: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
                        let mut cur_run: Option<(usize, Hsla, Option<Hsla>, bool)> = None;

                        for (c, &(character, flags, fg, bg)) in row.iter().enumerate() {
                            if flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                                continue;
                            }
                            let is_cursor = display_offset == 0 && r as i32 == cursor_pt.line.0
                                && c == cursor_pt.column.0
                                && exit_c.is_none();
                            let inverse = flags.contains(CellFlags::INVERSE);
                            let bold = flags.contains(CellFlags::BOLD);
                            let mut fg = resolve_ansi_color(fg, default_fg, term_bg, mode);
                            let mut bg = resolve_ansi_bg(bg, default_fg, term_bg, mode);
                            if inverse {
                                let prev_fg = fg;
                                fg = bg.unwrap_or(term_bg);
                                bg = Some(prev_fg);
                            }
                            if is_cursor {
                                bg = Some(cursor_bg);
                                fg = rgb(0xffffff).into();
                            }
                            let ch = if character == '\0' { ' ' } else { character };
                            let start = line_str.len();
                            line_str.push(ch);

                            let same = matches!(
                                cur_run,
                                Some((_, r_fg, r_bg, r_bold))
                                    if r_fg == fg && r_bg == bg && r_bold == bold
                            );
                            if !same {
                                if let Some((r_start, r_fg, r_bg, r_bold)) = cur_run.take() {
                                    if r_start < start
                                        && (r_fg != default_fg || r_bg.is_some() || r_bold)
                                    {
                                        highlights.push((
                                            r_start..start,
                                            HighlightStyle {
                                                color: Some(r_fg),
                                                background_color: r_bg,
                                                font_weight: r_bold.then_some(FontWeight::BOLD),
                                                ..Default::default()
                                            },
                                        ));
                                    }
                                }
                                cur_run = Some((start, fg, bg, bold));
                            }
                        }

                        if let Some((r_start, r_fg, r_bg, r_bold)) = cur_run.take() {
                            let end = line_str.len();
                            if r_start < end && (r_fg != default_fg || r_bg.is_some() || r_bold) {
                                highlights.push((
                                    r_start..end,
                                    HighlightStyle {
                                        color: Some(r_fg),
                                        background_color: r_bg,
                                        font_weight: r_bold.then_some(FontWeight::BOLD),
                                        ..Default::default()
                                    },
                                ));
                            }
                        }

                        let st = if highlights.is_empty() {
                            StyledText::new(line_str)
                        } else {
                            StyledText::new(line_str).with_highlights(highlights)
                        };
                        rows_out.push(st);
                    }
                }
                (
                    rows_out,
                    tab.shell.clone(),
                    tab.cwd.clone(),
                    tab.remote,
                    exit_c,
                    err_m,
                )
            } else {
                (Vec::new(), "shell".into(), String::new(), false, None, None)
            };

        div()
            .size_full()
            .flex()
            .flex_col()
            .font_family(FONT_SANS)
            // Tab Bar
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .px(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(theme.border_hairline)
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .id("terminal-tabs-scroll")
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .children(self.tabs.iter().map(|tab| {
                        let active = self.active_tab_id.as_deref() == Some(tab.id.as_str());
                        let tid = tab.id.clone();
                        let tid_close = tab.id.clone();
                        let hover_bg = theme.bg_hover;
                        div()
                            .id(SharedString::from(format!("term-tab-{}", tab.id)))
                            .h(px(26.0))
                            .flex_shrink_0()
                            .px(px(8.0))
                            .rounded(px(7.0))
                            .bg(if active {
                                theme.bg_elevated
                            } else {
                                SandTheme::transparent()
                            })
                            .border_1()
                            .border_color(if active {
                                theme.border_default
                            } else {
                                SandTheme::transparent()
                            })
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover_bg))
                            .on_click(cx.listener(move |v, _, window, cx| {
                                v.active_tab_id = Some(tid.clone());
                                window.focus(&v.focus_handle, cx);
                                cx.notify();
                            }))
                            .child(cursor_icon(
                                "terminal",
                                12.0,
                                if active {
                                    theme.text_primary
                                } else {
                                    theme.text_tertiary
                                },
                            ))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if active {
                                        theme.text_primary
                                    } else {
                                        theme.text_secondary
                                    })
                                    .child(tab.title.clone()),
                            )
                            .child(
                                gb_mini_icon_btn(
                                    SharedString::from(format!("term-close-{}", tab.id)),
                                    &theme,
                                    true,
                                )
                                .child(cursor_icon("x", 10.5, theme.text_tertiary))
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    cx.stop_propagation();
                                    v.close_tab(&tid_close, cx);
                                })),
                            )
                    }))
                            .overflow_x_scrollbar(),
                    )
                    .child(
                        gb_mini_icon_btn("term-add-tab", &theme, false)
                            .child(cursor_icon("plus", 13.0, theme.text_secondary))
                            .on_click(cx.listener(|v, _, window, cx| {
                                v.open_new_tab(cx);
                                window.focus(&v.focus_handle, cx);
                            })),
                    ),
            )
            // Status / Action strip
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .px(px(10.0))
                    .py(px(5.0))
                    .border_b_1()
                    .border_color(theme.border_hairline)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .min_w_0()
                            .child(gb_badge(
                                &theme,
                                if exit_code.is_some() {
                                    BadgeTone::Warning
                                } else if meta_remote {
                                    BadgeTone::Info
                                } else {
                                    BadgeTone::Success
                                },
                                match exit_code {
                                    Some(c) => format!("exit {c}"),
                                    None => meta_shell,
                                },
                            ))
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(10.5))
                                    .text_color(theme.text_tertiary)
                                    .truncate()
                                    .child(meta_cwd),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                gb_btn("term-clear-btn", &theme, BtnVariant::Ghost, true, false)
                                    .child("Clear")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.write_active(b"\x0c", cx);
                                        window.focus(&v.focus_handle, cx);
                                    })),
                            )
                            .child(
                                gb_btn("term-restart-btn", &theme, BtnVariant::Ghost, true, false)
                                    .child("Restart")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.restart_active_tab(cx);
                                        window.focus(&v.focus_handle, cx);
                                    })),
                            )

                    ),
            )
            // Error banner if PTY failed
            .when_some(error_msg, |d, err| {
                d.child(
                    div()
                        .mx(px(10.0))
                        .mt(px(8.0))
                        .p(px(10.0))
                        .rounded(px(10.0))
                        .bg(theme.danger_soft)
                        .text_size(px(12.0))
                        .text_color(theme.danger_text)
                        .child(err),
                )
            })
            // Native GPU Terminal Canvas
            .child(
                div()
                    .id("native-gpu-terminal-surface")
                    .track_focus(&self.focus_handle)
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .bg(term_bg)
                    .p(px(10.0))
                    .overflow_hidden()
                    .font_family(FONT_MONO)
                    .text_size(px(TERMINAL_FONT_SIZE))
                    .line_height(px(TERMINAL_LINE_HEIGHT))
                    .border_1()
                    .border_color(if is_focused {
                        theme.agent_accent_border
                    } else {
                        SandTheme::transparent()
                    })
                    .on_scroll(cx.listener(|view, event, _, cx| view.scroll_terminal(event, cx)))
                    .child(canvas(
                        move |measured, window, cx| {
                            bounds.set(measured);
                            let (width, height) = (f32::from(measured.size.width), f32::from(measured.size.height));
                            let cols = ((width - 29.0) / char_width).floor().clamp(2.0, 1000.0) as u16;
                            let rows = ((height - 22.0) / TERMINAL_LINE_HEIGHT).floor().clamp(2.0, 1000.0) as u16;
                            if active_dimensions == Some((cols, rows)) {
                                return;
                            }
                            if let Some(id) = active_id.as_deref() {
                                let id = id.to_string();
                                let terminal = terminal.clone();
                                window.defer(cx, move |_, cx| {
                                    let _ = terminal.update(cx, |view, cx| {
                                        if view.active_tab_id.as_deref() == Some(id.as_str())
                                            && view.tabs.iter().any(|tab| tab.id == id && (tab.cols, tab.rows) != (cols, rows))
                                        {
                                            view.resize_active_grid(cols, rows, cx);
                                            cx.notify();
                                        }
                                    });
                                });
                            }
                        },
                        |_, _, _, _| {},
                    ).absolute().inset_0())
                    .when_some(scroll_handle, |surface, handle| surface.vertical_scrollbar(&handle))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|v, _, window, cx| {
                            window.focus(&v.focus_handle, cx);
                            cx.notify();
                        }),
                    )
                    .on_key_down(cx.listener(|v, ev, window, cx| {
                        v.handle_key_down(ev, window, cx);
                    }))
                    .child(if self.tabs.is_empty() {
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme.text_secondary)
                                    .child("No active terminal tabs."),
                            )
                            .child(
                                gb_btn("term-open-first", &theme, BtnVariant::Primary, true, false)
                                    .child("New Terminal")
                                    .on_click(cx.listener(|v, _, window, cx| {
                                        v.open_new_tab(cx);
                                        window.focus(&v.focus_handle, cx);
                                    })),
                            )
                    } else {
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .text_color(default_fg)
                            .children(rendered_rows.into_iter().map(|styled_line| {
                                div().w_full().h(px(TERMINAL_LINE_HEIGHT)).flex_shrink_0().whitespace_nowrap().child(styled_line)
                            }))
                    }),
            )
            // Subtle keyboard hint footer
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .px(px(10.0))
                    .py(px(5.0))
                    .border_t_1()
                    .border_color(theme.border_hairline)
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(10.5))
                    .text_color(theme.text_tertiary)
                    .child(if is_focused {
                        "Terminal focused · Type directly"
                    } else {
                        "Click terminal to focus"
                    })
                    .child("Ctrl+Shift+C copy · Ctrl+Shift+V paste"),
            )
    }
}

fn resolve_ansi_bg(color: AnsiColor, fg: Hsla, bg: Hsla, mode: ThemeMode) -> Option<Hsla> {
    match color {
        AnsiColor::Named(NamedColor::Background) => None,
        other => Some(resolve_ansi_color(other, fg, bg, mode)),
    }
}

fn resolve_ansi_color(color: AnsiColor, default_fg: Hsla, default_bg: Hsla, mode: ThemeMode) -> Hsla {
    match color {
        AnsiColor::Spec(c) => rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32)).into(),
        AnsiColor::Named(named) => named_color(named, default_fg, default_bg, mode),
        AnsiColor::Indexed(idx) => indexed_color(idx, default_fg, default_bg, mode),
    }
}

fn named_color(named: NamedColor, default_fg: Hsla, default_bg: Hsla, mode: ThemeMode) -> Hsla {
    let dark = mode == ThemeMode::Dark;
    match named {
        NamedColor::Foreground | NamedColor::BrightForeground => default_fg,
        NamedColor::DimForeground => default_fg.opacity(0.65),
        NamedColor::Background => default_bg,
        NamedColor::Black => rgb(if dark { 0x262624 } else { 0x1d1d1c }).into(),
        NamedColor::Red => rgb(if dark { 0xf46c65 } else { 0xc93b34 }).into(),
        NamedColor::Green => rgb(if dark { 0x4cc38a } else { 0x208857 }).into(),
        NamedColor::Yellow => rgb(if dark { 0xe5b567 } else { 0xa66f12 }).into(),
        NamedColor::Blue => rgb(if dark { 0x62a8ff } else { 0x1068d4 }).into(),
        NamedColor::Magenta => rgb(if dark { 0xc084fc } else { 0x7c3aed }).into(),
        NamedColor::Cyan => rgb(if dark { 0x4fd1c5 } else { 0x0f8a7d }).into(),
        NamedColor::White => rgb(if dark { 0xe5e3df } else { 0x5f5d59 }).into(),
        NamedColor::BrightBlack => rgb(if dark { 0x6f6e69 } else { 0x8a8782 }).into(),
        NamedColor::BrightRed => rgb(0xff7b72).into(),
        NamedColor::BrightGreen => rgb(0x56d364).into(),
        NamedColor::BrightYellow => rgb(0xf2cc60).into(),
        NamedColor::BrightBlue => rgb(0x79c0ff).into(),
        NamedColor::BrightMagenta => rgb(0xd2a8ff).into(),
        NamedColor::BrightCyan => rgb(0x76e4f7).into(),
        NamedColor::BrightWhite => rgb(if dark { 0xffffff } else { 0x111110 }).into(),
        _ => default_fg,
    }
}

fn indexed_color(idx: u8, default_fg: Hsla, default_bg: Hsla, mode: ThemeMode) -> Hsla {
    match idx {
        0 => named_color(NamedColor::Black, default_fg, default_bg, mode),
        1 => named_color(NamedColor::Red, default_fg, default_bg, mode),
        2 => named_color(NamedColor::Green, default_fg, default_bg, mode),
        3 => named_color(NamedColor::Yellow, default_fg, default_bg, mode),
        4 => named_color(NamedColor::Blue, default_fg, default_bg, mode),
        5 => named_color(NamedColor::Magenta, default_fg, default_bg, mode),
        6 => named_color(NamedColor::Cyan, default_fg, default_bg, mode),
        7 => named_color(NamedColor::White, default_fg, default_bg, mode),
        8 => named_color(NamedColor::BrightBlack, default_fg, default_bg, mode),
        9 => named_color(NamedColor::BrightRed, default_fg, default_bg, mode),
        10 => named_color(NamedColor::BrightGreen, default_fg, default_bg, mode),
        11 => named_color(NamedColor::BrightYellow, default_fg, default_bg, mode),
        12 => named_color(NamedColor::BrightBlue, default_fg, default_bg, mode),
        13 => named_color(NamedColor::BrightMagenta, default_fg, default_bg, mode),
        14 => named_color(NamedColor::BrightCyan, default_fg, default_bg, mode),
        15 => named_color(NamedColor::BrightWhite, default_fg, default_bg, mode),
        16..=231 => {
            let n = idx - 16;
            let r = n / 36;
            let g = (n % 36) / 6;
            let b = n % 6;
            let to_u8 = |v: u8| if v == 0 { 0u32 } else { 55 + (v as u32) * 40 };
            rgb((to_u8(r) << 16) | (to_u8(g) << 8) | to_u8(b)).into()
        }
        232..=255 => {
            let v = (8 + (idx - 232) as u32 * 10).min(255);
            rgb((v << 16) | (v << 8) | v).into()
        }
    }
}
