use crate::backend::terminal::{EventSink, TerminalEvent, TerminalInfo};
use crate::state::app_state::AppState;
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_mini_icon_btn, BadgeTone, BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::{SandTheme, ThemeMode};
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Processor};
use alacritty_terminal::Term;
use base64::{engine::general_purpose::STANDARD, Engine};
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::{Arc, Mutex};

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

pub struct TerminalView {
    app: Entity<AppState>,
    focus_handle: FocusHandle,
    tabs: Vec<TermTab>,
    active_tab_id: Option<String>,
    tab_counter: usize,
    _poll_task: Task<()>,
}

impl TerminalView {
    pub fn new(app: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        cx.observe(&app, |_, _, cx| cx.notify()).detach();

        // Lightweight dirty-flag poller so PTY output from background threads repaints smoothly at ~30fps when active
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(33))
                    .await;
                let keep_going = this
                    .update(cx, |v, cx| {
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
            _poll_task: poll_task,
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
                        if !gen_copy.is_empty() {
                            terminals_ack.acknowledge(&id_ack, &gen_copy, len);
                        }
                        return Ok(());
                    }
                }
                TerminalEvent::Exit { code } => {
                    rt.exit_code = Some(code);
                    rt.dirty = true;
                }
                TerminalEvent::Error { message } => {
                    rt.error = Some(message);
                    rt.dirty = true;
                }
            }
            Ok(())
        });

        let (shell, actual_cwd, remote) =
            match terminals.start(id.clone(), cwd.clone(), cols, rows, ssh, sink) {
                Ok(TerminalInfo {
                    shell,
                    cwd,
                    remote,
                    ..
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

    fn clear_active_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = &self.active_tab_id {
            let _ = self.app.read(cx).terminals.write(id, b"\x0c");
        }
    }

    fn write_active(&self, data: &[u8], cx: &Context<Self>) {
        if let Some(id) = &self.active_tab_id {
            let _ = self.app.read(cx).terminals.write(id, data);
        }
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        let mods = &ks.modifiers;

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
        for r in 0..rows {
            let mut s = String::with_capacity(cols);
            for c in 0..cols {
                let cell = &grid[Line(r as i32)][Column(c)];
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
            let _ = self.app.read(cx).terminals.resize(&active_id, cols, rows);
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

        let (rendered_rows, meta_shell, meta_cwd, exit_code, error_msg) =
            if let Some(tab) = active_tab {
                let mut rows_out: Vec<Vec<(String, Hsla, Option<Hsla>, bool)>> = Vec::new();
                let mut exit_c = None;
                let mut err_m = None;
                if let Ok(rt) = tab.runtime.lock() {
                    exit_c = rt.exit_code;
                    err_m = rt.error.clone();
                    let grid = rt.term.grid();
                    let cursor_pt = grid.cursor.point;
                    let rows = tab.rows as usize;
                    let cols = tab.cols as usize;

                    for r in 0..rows {
                        let mut spans: Vec<(String, Hsla, Option<Hsla>, bool)> = Vec::new();
                        for c in 0..cols {
                            let cell = &grid[Line(r as i32)][Column(c)];
                            if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                                continue;
                            }
                            let is_cursor =
                                r as i32 == cursor_pt.line.0 && c == cursor_pt.column.0 && exit_c.is_none();
                            let inverse = cell.flags.contains(CellFlags::INVERSE);
                            let bold = cell.flags.contains(CellFlags::BOLD);
                            let mut fg = resolve_ansi_color(cell.fg, default_fg, term_bg, mode);
                            let mut bg = resolve_ansi_bg(cell.bg, default_fg, term_bg, mode);
                            if inverse {
                                let prev_fg = fg;
                                fg = bg.unwrap_or(term_bg);
                                bg = Some(prev_fg);
                            }
                            if is_cursor {
                                bg = Some(cursor_bg);
                                fg = rgb(0xffffff).into();
                            }
                            let ch = if cell.c == '\0' { ' ' } else { cell.c };
                            if let Some(last) = spans.last_mut() {
                                if last.1 == fg && last.2 == bg && last.3 == bold {
                                    last.0.push(ch);
                                    continue;
                                }
                            }
                            let mut s = String::new();
                            s.push(ch);
                            spans.push((s, fg, bg, bold));
                        }
                        rows_out.push(spans);
                    }
                }
                (
                    rows_out,
                    tab.shell.clone(),
                    tab.cwd.clone(),
                    exit_c,
                    err_m,
                )
            } else {
                (Vec::new(), "shell".into(), String::new(), None, None)
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
                    .px(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(theme.border_hairline)
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .overflow_hidden()
                    .children(self.tabs.iter().enumerate().map(|(idx, tab)| {
                        let active = self.active_tab_id.as_deref() == Some(tab.id.as_str());
                        let tid = tab.id.clone();
                        let tid_close = tab.id.clone();
                        let hover_bg = theme.bg_hover;
                        div()
                            .id(SharedString::from(format!("term-tab-{idx}")))
                            .h(px(26.0))
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
                                    SharedString::from(format!("term-close-{idx}")),
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
                                        v.clear_active_tab(cx);
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
                            .child(
                                gb_btn("term-cols-btn", &theme, BtnVariant::Ghost, true, false)
                                    .child("80×28")
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.resize_active_grid(80, 28, cx);
                                        cx.notify();
                                    })),
                            ),
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
                    .flex_1()
                    .w_full()
                    .bg(term_bg)
                    .p(px(10.0))
                    .overflow_hidden()
                    .font_family(FONT_MONO)
                    .text_size(px(12.0))
                    .line_height(px(16.5))
                    .border_1()
                    .border_color(if is_focused {
                        theme.agent_accent_border
                    } else {
                        SandTheme::transparent()
                    })
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
                            .children(rendered_rows.into_iter().map(|spans| {
                                div().w_full().flex().flex_row().whitespace_nowrap().children(
                                    spans.into_iter().map(|(text, fg, bg, bold)| {
                                        let mut cell_run = div().text_color(fg).child(text);
                                        if let Some(b) = bg {
                                            cell_run = cell_run.bg(b);
                                        }
                                        if bold {
                                            cell_run = cell_run.font_weight(FontWeight::BOLD);
                                        }
                                        cell_run
                                    }),
                                )
                            }))
                    }),
            )
            // Subtle keyboard hint footer
            .child(
                div()
                    .w_full()
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
