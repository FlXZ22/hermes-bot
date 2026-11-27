use crate::face::face_frame_stats;
use crate::harness::events::{SubagentStatus, TrajectoryEventKind};
use crate::harness::metrics::format_elapsed_ms;
use crate::harness::ptc::{is_ptc_tool, resolve_recorded_language};
use crate::state::app_state::{AppState, AvatarEditorTarget, RightPaneTabId};
use crate::theme::bot_face::{bot_face_with_options, BotFaceOptions};
use crate::theme::components::{
    cursor_icon, format_tokens, gb_badge, gb_btn, gb_mini_icon_btn, gb_row, gb_section_card,
    gb_section_card_head, gb_segmented_item, gb_segmented_shell, BadgeTone, BtnVariant, FONT_MONO,
    FONT_SANS,
};
use crate::ui::inspector::terminal_view::TerminalView;
use crate::ui::markdown::render_highlighted_code_block;
use crate::ui::motion::{gpu_spinner, view_enter, EntranceKey, SpringValue};
use crate::ui::smooth_scroll::{
    smooth_scroll_layer, smooth_scroll_layer_xy, NestedScrollRegions, SmoothScroller,
};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement;
use std::collections::HashSet;
use std::path::Path;

pub struct RightSidePaneView {
    app: Entity<AppState>,
    terminal: Option<Entity<TerminalView>>,
    cwd_input: Entity<InputState>,
    soul_input: Entity<TextareaState>,
    synced_cwd: String,
    synced_soul: String,
    target_width_px: f32,
    width: SpringValue,
    open_progress: f32,
    tab_entrance: EntranceKey<RightPaneTabId>,
    files_scroll: ScrollHandle,
    files_smooth: SmoothScroller,
    preview_scroll: ScrollHandle,
    preview_smooth: SmoothScroller,
    preview_source: Option<String>,
    git_scroll: ScrollHandle,
    git_smooth: SmoothScroller,
    shown_diff_lines: usize,
    git_cwd: String,
    expanded_git_files: HashSet<String>,
    agents_scroll: ScrollHandle,
    agents_smooth: SmoothScroller,
    trajectory_scroll: ScrollHandle,
    trajectory_smooth: SmoothScroller,
    shown_trajectory_events: usize,
    expanded_trajectory: HashSet<String>,
    trajectory_json_mode: HashSet<String>,
    bot_scroll: ScrollHandle,
    bot_smooth: SmoothScroller,
    bot_nested_scroll: NestedScrollRegions,
}

impl RightSidePaneView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (cwd, soul) = {
            let s = app.read(cx);
            (s.inspector_cwd.clone(), s.soul_md.clone())
        };
        let cwd_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("/path/to/workspace");
            st.set_value(cwd.clone(), window, cx);
            st
        });
        let soul_input = cx.new(|cx| {
            let mut st = TextareaState::new(window, cx)
                .placeholder("Define bot personality in SOUL.md...")
                .rows(10);
            st.set_value(soul.clone(), window, cx);
            st
        });

        cx.subscribe(&cwd_input, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                let next = this.cwd_input.read(cx).value().trim().to_string();
                if !next.is_empty() {
                    this.app.update(cx, |s, cx| {
                        s.inspector_cwd = next;
                        s.inspector_preview_path = None;
                        s.refresh_inspector_files(cx);
                        s.refresh_inspector_git(cx);
                    });
                }
            }
        })
        .detach();

        cx.observe(&app, |_, app_e, cx| {
            let app = app_e.read(cx);
            if !app.setup_open && !app.initializing && app.right_pane_open {
                cx.notify();
            }
        })
        .detach();
        let files_scroll = ScrollHandle::new();
        let files_smooth = SmoothScroller::for_handle(files_scroll.clone());
        let preview_scroll = ScrollHandle::new();
        let preview_smooth = SmoothScroller::for_handle(preview_scroll.clone());
        let git_scroll = ScrollHandle::new();
        let git_smooth = SmoothScroller::for_handle(git_scroll.clone());
        let agents_scroll = ScrollHandle::new();
        let agents_smooth = SmoothScroller::for_handle(agents_scroll.clone());
        let trajectory_scroll = ScrollHandle::new();
        let trajectory_smooth = SmoothScroller::for_handle(trajectory_scroll.clone());
        let bot_scroll = ScrollHandle::new();
        let bot_smooth = SmoothScroller::for_handle(bot_scroll.clone());
        Self {
            app,
            terminal: None,
            cwd_input,
            soul_input,
            synced_cwd: cwd,
            synced_soul: soul,
            target_width_px: 380.0,
            width: SpringValue::new(380.0).with_precision(0.1),
            open_progress: 0.0,
            tab_entrance: EntranceKey::new(),
            files_scroll,
            files_smooth,
            preview_scroll,
            preview_smooth,
            preview_source: None,
            git_scroll,
            git_smooth,
            shown_diff_lines: 250,
            git_cwd: String::new(),
            expanded_git_files: HashSet::new(),
            agents_scroll,
            agents_smooth,
            trajectory_scroll,
            trajectory_smooth,
            shown_trajectory_events: 80,
            expanded_trajectory: HashSet::new(),
            trajectory_json_mode: HashSet::new(),
            bot_scroll,
            bot_smooth,
            bot_nested_scroll: NestedScrollRegions::default(),
        }
    }

    fn go_parent_dir(&mut self, cx: &mut Context<Self>) {
        let cwd = self.app.read(cx).inspector_cwd.clone();
        if let Some(parent) = Path::new(&cwd).parent() {
            let p = parent.to_string_lossy().to_string();
            if !p.is_empty() {
                self.app.update(cx, |s, cx| {
                    s.inspector_cwd = p;
                    s.inspector_preview_path = None;
                    s.refresh_inspector_files(cx);
                    s.refresh_inspector_git(cx);
                });
            }
        }
    }

    pub fn open_progress(&self) -> f32 {
        self.open_progress
    }

    pub fn set_open_progress(&mut self, progress: f32, cx: &mut Context<Self>) {
        self.open_progress = progress;
        cx.notify();
    }

    fn cycle_width(&mut self, cx: &mut Context<Self>) {
        self.target_width_px = if self.target_width_px < 420.0 {
            480.0
        } else if self.target_width_px < 560.0 {
            600.0
        } else {
            360.0
        };
        self.width.set_target(self.target_width_px);
        cx.notify();
    }
}

impl Render for RightSidePaneView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (
            theme,
            _mode,
            tab,
            cwd,
            files,
            file_err,
            preview_path,
            preview_content,
            git_status,
            git_loading,
            git_err,
            bot_seed,
            bot_override,
            bot_state,
            active_profile,
            soul_md,
            sys_stats,
            boot_ms,
            backend_info,
            refreshing_models,
            restarting_backend,
        ) = {
            let s = self.app.read(cx);
            let seed = s.active_bot_seed();
            let tab = s.right_pane_tab;
            let files_tab = tab == RightPaneTabId::Files;
            let git_tab = tab == RightPaneTabId::Git;
            let bot_tab = tab == RightPaneTabId::Bot;
            let ov = bot_tab.then(|| s.bot_avatars.get(&seed).cloned()).flatten();
            let chat_state = s.active_bot_state();
            (
                s.sand_theme(),
                s.theme_mode(),
                s.right_pane_tab,
                s.inspector_cwd.clone(),
                if files_tab {
                    s.inspector_files.clone()
                } else {
                    Default::default()
                },
                if files_tab {
                    s.inspector_file_error.clone()
                } else {
                    None
                },
                if files_tab {
                    s.inspector_preview_path.clone()
                } else {
                    None
                },
                if files_tab {
                    s.inspector_preview_content.clone()
                } else {
                    Default::default()
                },
                if git_tab {
                    s.inspector_git.clone()
                } else {
                    None
                },
                s.inspector_git_loading,
                if git_tab {
                    s.inspector_git_error.clone()
                } else {
                    None
                },
                seed,
                ov,
                chat_state,
                s.active_profile.clone(),
                s.soul_md.clone(),
                if bot_tab {
                    s.system_stats.clone()
                } else {
                    None
                },
                s.bootstrap_ms,
                if bot_tab {
                    s.rust_backend.clone()
                } else {
                    None
                },
                s.refreshing_models,
                s.restarting_backend,
            )
        };

        if cwd != self.git_cwd {
            self.git_cwd.clone_from(&cwd);
            self.shown_diff_lines = 250;
            self.git_smooth.cancel();
            self.git_scroll.set_offset(Point::default());
            self.files_smooth.cancel();
            self.files_scroll.set_offset(Point::default());
        }
        if cwd != self.synced_cwd {
            self.synced_cwd = cwd.clone();
            self.cwd_input
                .update(cx, |st, cx| st.set_value(cwd.clone(), window, cx));
        }
        if soul_md != self.synced_soul
            && self.soul_input.read(cx).value().as_ref() == self.synced_soul
        {
            self.synced_soul = soul_md.clone();
            self.soul_input
                .update(cx, |st, cx| st.set_value(soul_md.clone(), window, cx));
        }

        if preview_path != self.preview_source {
            self.preview_source.clone_from(&preview_path);
            self.preview_smooth.cancel();
            self.preview_scroll.set_offset(Point::default());
        }

        let tabs: &[(RightPaneTabId, &str)] = &[
            (RightPaneTabId::Files, "Files"),
            (RightPaneTabId::Terminal, "Term"),
            (RightPaneTabId::Git, "Git"),
            (RightPaneTabId::Agents, "Agents"),
            (RightPaneTabId::Trajectory, "Trace"),
            (RightPaneTabId::Bot, "Bot"),
        ];

        let viewport_width = window.viewport_size().width;
        let sidebar_width = if self.app.read(cx).sidebar_collapsed {
            px(68.0)
        } else {
            px(280.0)
        };
        let max_width = (viewport_width - sidebar_width - px(340.0)).max(px(0.0));
        let width = px(self.width.advance(window)).min(max_width);
        let tab_id = self.tab_entrance.id("inspector-tab", tab);
        let terminal =
            (tab == RightPaneTabId::Terminal && self.app.read(cx).right_pane_open).then(|| {
                let app = self.app.clone();
                self.terminal
                    .get_or_insert_with(|| cx.new(|cx| TerminalView::new(app, window, cx)))
                    .clone()
            });

        let panel = div()
            .w(width)
            .h_full()
            .flex_shrink_0()
            .bg(theme.side_pane_bg)
            .border_l_1()
            .border_color(theme.border_default)
            .font_family(FONT_SANS)
            .flex()
            .flex_col()
            // Top Inspector Bar
            .child(
                div()
                    .w_full()
                    .px(px(10.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(theme.border_hairline)
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .flex_shrink_0()
                    .gap(px(6.0))
                    .child(gb_segmented_shell(&theme).flex_wrap().children(tabs.iter().map(
                        |(tid, label)| {
                            let target = *tid;
                            gb_segmented_item(
                                SharedString::from(format!("rp-tab-{label}")),
                                &theme,
                                tab == target,
                                *label,
                            )
                            .on_click(cx.listener(move |v, _, _, cx| {
                                v.app.update(cx, |s, cx| {
                                    s.right_pane_tab = target;
                                    match target {
                                        RightPaneTabId::Files => s.refresh_inspector_files(cx),
                                        RightPaneTabId::Git => s.refresh_inspector_git(cx),
                                        _ => {}
                                    }
                                    cx.notify();
                                });
                            }))
                        },
                    )))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                gb_mini_icon_btn("rp-width-cycle", &theme, false)
                                    .child(cursor_icon("layout-sidebar-right", 13.0, theme.text_secondary))
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.cycle_width(cx);
                                    })),
                            )
                            .child(
                                gb_mini_icon_btn("rp-close", &theme, false)
                                    .child(cursor_icon("x", 13.0, theme.text_secondary))
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        v.app.update(cx, |s, cx| {
                                            s.right_pane_open = false;
                                            s.bump_shell();
                                            cx.notify();
                                        });
                                    })),
                            ),
                    ),
            )
            // Body
            .child(view_enter(tab_id, div().flex_1().min_h_0().w_full().flex().flex_col().child(match tab {
                RightPaneTabId::Files => {
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        // CWD bar
                        .child(
                            div()
                                .w_full()
                                .p(px(8.0))
                                .border_b_1()
                                .border_color(theme.border_hairline)
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    gb_mini_icon_btn("files-up", &theme, false)
                                        .child(cursor_icon("arrow-up", 13.0, theme.text_secondary))
                                        .on_click(cx.listener(|v, _, _, cx| {
                                            v.go_parent_dir(cx);
                                        })),
                                )
                                .child(div().flex_1().min_w_0().child(Input::new(&self.cwd_input)))
                                .child(
                                    gb_mini_icon_btn("files-refresh", &theme, false)
                                        .child(cursor_icon("refresh", 13.0, theme.text_secondary))
                                        .on_click(cx.listener(|v, _, _, cx| {
                                            let next = v.cwd_input.read(cx).value().trim().to_string();
                                            v.app.update(cx, |s, cx| {
                                                if !next.is_empty() {
                                                    s.inspector_cwd = next;
                                                }
                                                s.refresh_inspector_files(cx);
                                            });
                                        })),
                                ),
                        )
                        // Preview or File list
                        .child(if let Some(ppath) = preview_path {
                            let copyable = !preview_content.is_empty()
                                && preview_content != "Loading…"
                                && !preview_content.starts_with("Error reading file:");
                            let content = SharedString::from(preview_content);
                            let content_copy = content.clone();
                            div()
                                .flex_1()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .border_b_1()
                                        .border_color(theme.border_hairline)
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(6.0))
                                        .child(
                                            gb_btn("files-back", &theme, BtnVariant::Ghost, true, false)
                                                .child(cursor_icon("arrow-left", 12.0, theme.text_secondary))
                                                .child("Back")
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.app.update(cx, |s, cx| {
                                                        s.inspector_preview_path = None;
                                                        cx.notify();
                                                    });
                                                })),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .font_family(FONT_MONO)
                                                .text_size(px(11.0))
                                                .text_color(theme.text_secondary)
                                                .truncate()
                                                .child(
                                                    Path::new(&ppath)
                                                        .file_name()
                                                        .map(|n| n.to_string_lossy().into_owned())
                                                        .unwrap_or(ppath),
                                                ),
                                        )
                                        .child(
                                            gb_btn("files-copy", &theme, BtnVariant::Ghost, true, !copyable)
                                                .child("Copy")
                                                .on_click(cx.listener(move |v, _, _, cx| {
                                                    if !copyable {
                                                        return;
                                                    }
                                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                                        content_copy.to_string(),
                                                    ));
                                                    v.app.update(cx, |s, cx| {
                                                        s.show_toast("Copied file content", cx);
                                                    });
                                                })),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .w_full()
                                        .relative()
                                        .child(
                                            div()
                                                .id("files-preview-scroll")
                                                .track_scroll(&self.preview_scroll)
                                                .size_full()
                                                .overflow_scroll()
                                                .p(px(12.0))
                                                .bg(theme.bg_tertiary)
                                                .font_family(FONT_MONO)
                                                .text_size(px(11.5))
                                                .text_color(theme.text_primary)
                                                .whitespace_nowrap()
                                                .child(content),
                                        )
                                        .vertical_scrollbar(&self.preview_scroll)
                                        .horizontal_scrollbar(&self.preview_scroll)
                                        .child(smooth_scroll_layer_xy(&self.preview_smooth)),
                                )
                                .into_any_element()
                        } else {
                            div()
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .relative()
                                .child(
                                    div()
                                        .id("files-list-scroll")
                                        .track_scroll(&self.files_scroll)
                                        .size_full()
                                        .overflow_y_scroll()
                                        .p(px(6.0))
                                .flex()
                                .flex_col()
                                .gap(px(1.0))
                                .when_some(file_err, |d, err| {
                                    d.child(
                                        div()
                                            .p(px(10.0))
                                            .rounded(px(8.0))
                                            .bg(theme.danger_soft)
                                            .text_size(px(12.0))
                                            .text_color(theme.danger_text)
                                            .child(err),
                                    )
                                })
                                .children(files.into_iter().enumerate().map(|(idx, entry)| {
                                    let is_dir = entry.is_dir;
                                    let name = entry.name.clone();
                                    let size_lbl = if is_dir {
                                        String::new()
                                    } else {
                                        format_bytes(entry.size)
                                    };
                                    let hover_bg = theme.bg_hover;
                                    div()
                                        .id(SharedString::from(format!("file-row-{idx}")))
                                        .w_full()
                                        .px(px(8.0))
                                        .py(px(6.0))
                                        .rounded(px(7.0))
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(8.0))
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(hover_bg))
                                        .on_click(cx.listener(move |v, _, _, cx| {
                                            v.app.update(cx, |s, cx| {
                                                s.open_inspector_file(entry.clone(), cx);
                                            });
                                        }))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(7.0))
                                                .min_w_0()
                                                .child(cursor_icon(
                                                    if is_dir { "folder" } else { "file" },
                                                    13.5,
                                                    if is_dir {
                                                        theme.agent_accent
                                                    } else {
                                                        theme.text_tertiary
                                                    },
                                                ))
                                                .child(
                                                    div()
                                                        .text_size(px(12.5))
                                                        .text_color(theme.text_primary)
                                                        .truncate()
                                                        .child(name),
                                                ),
                                        )
                                        .when(!size_lbl.is_empty(), |d| {
                                            d.child(
                                                div()
                                                    .font_family(FONT_MONO)
                                                    .text_size(px(10.5))
                                                    .text_color(theme.text_tertiary)
                                                    .flex_shrink_0()
                                                    .child(size_lbl),
                                            )
                                        })
                                }))
                                )
                                .vertical_scrollbar(&self.files_scroll)
                                .child(smooth_scroll_layer(&self.files_smooth))
                                .into_any_element()
                        })
                        .into_any_element()
                }
                RightPaneTabId::Terminal => div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .children(terminal.or_else(|| self.terminal.clone()))
                    .into_any_element(),
                RightPaneTabId::Git => {
                    let (work_proj_name, agent_edits) = {
                        let s = self.app.read(cx);
                        (
                            s.active_work_project().map(|p| p.name.clone()),
                            s.active_metrics().agent_edits,
                        )
                    };
                    let branch_badge = if let Some(ref g) = git_status {
                        if !g.is_repo {
                            (BadgeTone::Muted, "Non-Git folder".to_string())
                        } else if g.detached_head {
                            let label = g
                                .head_short
                                .as_deref()
                                .map(|h| format!("Detached HEAD ({h})"))
                                .unwrap_or_else(|| "Detached HEAD".into());
                            (BadgeTone::Warning, label)
                        } else {
                            (
                                BadgeTone::Accent,
                                if g.branch.is_empty() { "HEAD".into() } else { g.branch.clone() },
                            )
                        }
                    } else {
                        (BadgeTone::Muted, "Checking Git…".into())
                    };
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .relative()
                        .child(
                            div()
                                .id("git-scroll")
                                .track_scroll(&self.git_scroll)
                                .size_full()
                                .overflow_y_scroll()
                                .restrict_scroll_to_axis()
                                .p(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        // Project + Working Directory context header
                        .child(
                            div()
                                .flex_shrink_0()
                                .px(px(10.0))
                                .py(px(8.0))
                                .rounded(px(10.0))
                                .bg(theme.bg_card)
                                .border_1()
                                .border_color(theme.border_subtle)
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child(work_proj_name.unwrap_or_else(|| "Workspace Directory".into())),
                                        )
                                        .child(gb_badge(&theme, branch_badge.0, branch_badge.1)),
                                )
                                .child(
                                    div()
                                        .font_family(FONT_MONO)
                                        .text_size(px(10.5))
                                        .text_color(theme.text_tertiary)
                                        .truncate()
                                        .child(cwd.clone()),
                                ),
                        )
                        // Git Change Counts vs Agent Edit Counts strip
                        .child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_between()
                                .flex_wrap()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .flex_wrap()
                                        .gap(px(6.0))
                                        .when_some(git_status.as_ref().filter(|g| g.is_repo), |d, g| {
                                            let basis = if g.comparison_basis.is_empty() {
                                                "worktree vs HEAD"
                                            } else {
                                                g.comparison_basis.as_str()
                                            };
                                            d.child(gb_badge(&theme, BadgeTone::Success, format!("+{}", g.total_added)))
                                                .child(gb_badge(&theme, BadgeTone::Danger, format!("-{}", g.total_removed)))
                                                .child(
                                                    div()
                                                        .font_family(FONT_MONO)
                                                        .text_size(px(10.5))
                                                        .text_color(theme.text_tertiary)
                                                        .child(format!("Git ({basis})")),
                                                )
                                        })
                                        .when(agent_edits.lines_added > 0 || agent_edits.lines_removed > 0, |d| {
                                            d.child(gb_badge(
                                                &theme,
                                                BadgeTone::Info,
                                                format!("Agent +{} / -{}", agent_edits.lines_added, agent_edits.lines_removed),
                                            ))
                                        }),
                                )
                                .child(
                                    gb_btn("git-refresh-btn", &theme, BtnVariant::Secondary, true, git_loading)
                                        .when(git_loading, |d| d.child(gpu_spinner(12.0, theme.text_primary)))
                                        .when(!git_loading, |d| d.child(cursor_icon("refresh", 12.0, theme.text_primary)))
                                        .child(if git_loading { "Loading…" } else { "Refresh" })
                                        .on_click(cx.listener(|v, _, _, cx| {
                                            v.app.update(cx, |s, cx| s.refresh_inspector_git(cx));
                                        })),
                                ),
                        )
                        .when_some(git_err, |d, err| {
                            d.child(
                                div()
                                    .flex_shrink_0()
                                    .p(px(10.0))
                                    .rounded(px(8.0))
                                    .bg(theme.danger_soft)
                                    .text_size(px(12.0))
                                    .text_color(theme.danger_text)
                                    .child(err),
                            )
                        })
                        .when(git_status.as_ref().is_some_and(|g| !g.is_repo), |d| {
                            d.child(
                                gb_section_card(&theme).child(
                                    div()
                                        .p(px(14.0))
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_primary)
                                                .child("Not a Git repository"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.5))
                                                .text_color(theme.text_secondary)
                                                .child("Existing non-Git folders work normally without automatically running git init."),
                                        ),
                                ),
                            )
                        })
                        .when_some(git_status.filter(|g| g.is_repo), |d, git| {
                            d.child(
                                gb_section_card(&theme)
                                    .child(gb_section_card_head(&theme).child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child(format!(
                                                "Changed Files ({}) · click to inspect diff",
                                                git.changed_files.len()
                                            )),
                                    ))
                                    .child(if git.changed_files.is_empty() {
                                        div()
                                            .p(px(12.0))
                                            .text_size(px(12.0))
                                            .text_color(theme.text_secondary)
                                            .child("Working tree clean.")
                                    } else {
                                        div()
                                            .p(px(8.0))
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .children(git.changed_files.iter().enumerate().map(|(idx, chg)| {
                                                let tone = if chg.status.contains('D') {
                                                    BadgeTone::Danger
                                                } else if chg.status.contains('?')
                                                    || chg.status.contains('A')
                                                {
                                                    BadgeTone::Success
                                                } else {
                                                    BadgeTone::Warning
                                                };
                                                let file_key = chg.path.clone();
                                                let expanded = self.expanded_git_files.contains(&file_key);
                                                let toggle_key = file_key.clone();
                                                let diff_text = chg.file_diff.clone();
                                                div()
                                                    .flex_shrink_0()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(4.0))
                                                    .child(
                                                        div()
                                                            .id(SharedString::from(format!("git-file-{idx}")))
                                                            .flex()
                                                            .items_center()
                                                            .justify_between()
                                                            .gap(px(7.0))
                                                            .px(px(6.0))
                                                            .py(px(4.0))
                                                            .rounded(px(7.0))
                                                            .cursor_pointer()
                                                            .hover(|st| st.bg(theme.bg_hover))
                                                            .on_click(cx.listener(move |v, _, _, cx| {
                                                                if !v.expanded_git_files.remove(&toggle_key) {
                                                                    v.expanded_git_files.insert(toggle_key.clone());
                                                                }
                                                                cx.notify();
                                                            }))
                                                            .child(
                                                                div()
                                                                    .flex_1()
                                                                    .min_w_0()
                                                                    .flex()
                                                                    .items_center()
                                                                    .gap(px(6.0))
                                                                    .child(gb_badge(&theme, tone, chg.status.clone()))
                                                                    .child(
                                                                        div()
                                                                            .font_family(FONT_MONO)
                                                                            .text_size(px(11.5))
                                                                            .text_color(theme.text_primary)
                                                                            .truncate()
                                                                            .child(chg.path.clone()),
                                                                    ),
                                                            )
                                                            .child(
                                                                div()
                                                                    .flex_shrink_0()
                                                                    .flex()
                                                                    .items_center()
                                                                    .gap(px(5.0))
                                                                    .font_family(FONT_MONO)
                                                                    .text_size(px(10.5))
                                                                    .when(chg.is_binary, |d| {
                                                                        d.child(div().text_color(theme.text_tertiary).child("bin"))
                                                                    })
                                                                    .when(!chg.is_binary, |d| {
                                                                        d.when_some(chg.added, |d, a| {
                                                                            d.child(div().text_color(theme.success_text).child(format!("+{a}")))
                                                                        })
                                                                        .when_some(chg.removed, |d, r| {
                                                                            d.child(div().text_color(theme.danger_text).child(format!("-{r}")))
                                                                        })
                                                                    })
                                                                    .child(cursor_icon(
                                                                        if expanded { "chevron-down" } else { "chevron-right" },
                                                                        11.0,
                                                                        theme.text_tertiary,
                                                                    )),
                                                            ),
                                                    )
                                                    .when(expanded, |d| {
                                                        d.child(if let Some(patch) = diff_text.filter(|s| !s.trim().is_empty()) {
                                                            render_highlighted_code_block(
                                                                SharedString::from(format!("git-fdiff-{idx}")),
                                                                Some("diff"),
                                                                &patch,
                                                                false,
                                                                false,
                                                                &theme,
                                                            )
                                                        } else {
                                                            div()
                                                                .px(px(10.0))
                                                                .py(px(6.0))
                                                                .text_size(px(11.0))
                                                                .text_color(theme.text_tertiary)
                                                                .child("No textual diff patch available (untracked or binary file).")
                                                        })
                                                    })
                                            }))
                                    }),
                            )
                            .when(!git.worktrees.is_empty(), |d| {
                                d.child(
                                    gb_section_card(&theme)
                                        .child(gb_section_card_head(&theme).child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child(format!("Worktrees ({})", git.worktrees.len())),
                                        ))
                                        .child(
                                            div()
                                                .p(px(10.0))
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.0))
                                                .children(git.worktrees.iter().map(|wt| {
                                                    div()
                                                        .font_family(FONT_MONO)
                                                        .text_size(px(11.0))
                                                        .text_color(theme.text_secondary)
                                                        .child(wt.clone())
                                                })),
                                        ),
                                )
                            })
                            .when(!git.diff.trim().is_empty(), |d| {
                                d.child(
                                    gb_section_card(&theme)
                                        .child(gb_section_card_head(&theme).child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child("Full Patch"),
                                        ))
                                        .child(
                                            div()
                                                .p(px(10.0))
                                                .bg(theme.bg_tertiary)
                                                .font_family(FONT_MONO)
                                                .text_size(px(11.0))
                                                .flex()
                                                .flex_col()
                                                .children(git.diff.lines().take(self.shown_diff_lines).map(|line| {
                                                    let color = if line.starts_with('+')
                                                        && !line.starts_with("+++")
                                                    {
                                                        theme.success_text
                                                    } else if line.starts_with('-')
                                                        && !line.starts_with("---")
                                                    {
                                                        theme.danger_text
                                                    } else if line.starts_with("@@") {
                                                        theme.accent_text
                                                    } else {
                                                        theme.text_secondary
                                                    };
                                                    div()
                                                        .whitespace_nowrap()
                                                        .text_color(color)
                                                        .child(line.to_string())
                                                }))
                                                .overflow_x_scrollbar(),
                                        )
                                        .when(git.diff.lines().nth(self.shown_diff_lines).is_some(), |diff| {
                                            diff.child(
                                                gb_row(&theme).child(
                                                    gb_btn("inspector-diff-more", &theme, BtnVariant::Secondary, true, false)
                                                        .child("Show more lines")
                                                        .on_click(cx.listener(|view, _, _, cx| {
                                                            view.shown_diff_lines += 250;
                                                            cx.notify();
                                                        })),
                                                ),
                                            )
                                        }),
                                )
                            })
                        })
                        )
                        .vertical_scrollbar(&self.git_scroll)
                        .child(smooth_scroll_layer(&self.git_smooth))
                        .into_any_element()
                }
                RightPaneTabId::Agents => {
                    let (exec_state, live_op, active_model, subagents, metrics, goal_snap, goal_history) = {
                        let s = self.app.read(cx);
                        let live_op = s
                            .active_chat_ref()
                            .map(|c| {
                                if !c.live_status_text.is_empty() {
                                    c.live_status_text.clone()
                                } else if let Some(t) = c.active_tool_name.as_deref() {
                                    format!("Running {t}")
                                } else {
                                    c.execution_state().label().to_string()
                                }
                            })
                            .unwrap_or_else(|| "Idle".into());
                        (
                            s.active_execution_state(),
                            live_op,
                            s.active_model.clone(),
                            s.active_subagents(),
                            s.active_metrics(),
                            s.active_goal_snapshot(),
                            s.active_goal_events(),
                        )
                    };
                    let in_tok = metrics.input_tokens();
                    let out_tok = metrics.output_tokens();
                    let tps = metrics.output_tps();

                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .relative()
                        .child(
                            div()
                                .id("agents-inspector-scroll")
                                .track_scroll(&self.agents_scroll)
                                .size_full()
                                .overflow_y_scroll()
                                .restrict_scroll_to_axis()
                                .p(px(12.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                // Active Lead Agent card
                                .child(
                                    gb_section_card(&theme)
                                        .child(
                                            gb_section_card_head(&theme)
                                                .child(
                                                    div()
                                                        .text_size(px(12.5))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(theme.text_primary)
                                                        .child("Active Lead Agent"),
                                                )
                                                .child(gb_badge(
                                                    &theme,
                                                    if exec_state == crate::harness::events::ExecutionState::Idle {
                                                        BadgeTone::Muted
                                                    } else if exec_state == crate::harness::events::ExecutionState::Failed {
                                                        BadgeTone::Danger
                                                    } else {
                                                        BadgeTone::Accent
                                                    },
                                                    exec_state.label(),
                                                )),
                                        )
                                        .child(
                                            div()
                                                .p(px(12.0))
                                                .flex()
                                                .flex_col()
                                                .gap(px(5.0))
                                                .font_family(FONT_MONO)
                                                .text_size(px(11.0))
                                                .text_color(theme.text_secondary)
                                                .child(format!("Agent:     lead ({active_profile})"))
                                                .child(format!("Model:     {active_model}"))
                                                .child(format!("Operation: {live_op}"))
                                                .child(format!(
                                                    "Elapsed:   {}",
                                                    metrics
                                                        .elapsed_ms()
                                                        .map(format_elapsed_ms)
                                                        .unwrap_or_else(|| "unavailable".into())
                                                ))
                                                .child(format!(
                                                    "Tokens:    in={} ({}) · out={} ({})",
                                                    in_tok.value.map(format_tokens).unwrap_or_else(|| "—".into()),
                                                    in_tok.provenance.label(),
                                                    out_tok.value.map(format_tokens).unwrap_or_else(|| "—".into()),
                                                    out_tok.provenance.label(),
                                                ))
                                                .child(format!(
                                                    "Speed:     {} ({}, {:.0}s window)",
                                                    tps.tokens_per_sec
                                                        .map(|r| format!("{r:.1} tok/s"))
                                                        .unwrap_or_else(|| "unavailable".into()),
                                                    tps.provenance.label(),
                                                    tps.window_secs,
                                                ))
                                                .when_some(metrics.cache_hit_pct, |d, pct| {
                                                    d.child(format!("Cache hit: {pct:.1}%"))
                                                }),
                                        ),
                                )
                                // Active Goal Summary (if any)
                                .when_some(goal_snap, |d, snap| {
                                    let is_armed = snap.activation
                                        == crate::harness::goal_worker::GoalActivation::Armed;
                                    let recent_events: Vec<_> = goal_history
                                        .iter()
                                        .rev()
                                        .take(6)
                                        .cloned()
                                        .collect();
                                    d.child(
                                        gb_section_card(&theme)
                                            .child(
                                                gb_section_card_head(&theme)
                                                    .child(
                                                        div()
                                                            .text_size(px(12.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Background Goal Worker"),
                                                    )
                                                    .child(gb_badge(&theme, BadgeTone::Accent, snap.goal.phase.label())),
                                            )
                                            .child(
                                                div()
                                                    .p(px(12.0))
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(5.0))
                                                    .text_size(px(11.5))
                                                    .text_color(theme.text_secondary)
                                                    .child(snap.goal.objective)
                                                    .child(
                                                        div()
                                                            .font_family(FONT_MONO)
                                                            .text_size(px(10.5))
                                                            .text_color(theme.text_tertiary)
                                                            .child(format!(
                                                                "Round {}/{} · rev {} · {} · worker {}",
                                                                snap.goal.rounds_started,
                                                                snap.goal.max_rounds,
                                                                snap.goal.revision,
                                                                if is_armed { "armed" } else { "disarmed" },
                                                                if snap.worker_connected { "connected" } else { "offline" }
                                                            )),
                                                    )
                                                    .when_some(snap.goal.stop_reason.clone(), |d, reason| {
                                                        d.child(
                                                            div()
                                                                .text_size(px(11.0))
                                                                .text_color(theme.warn_text)
                                                                .child(format!("Stop reason: {reason}")),
                                                        )
                                                    })
                                                    .when(!recent_events.is_empty(), |d| {
                                                        d.child(
                                                            div()
                                                                .mt(px(4.0))
                                                                .pt(px(6.0))
                                                                .border_t_1()
                                                                .border_color(theme.border_hairline)
                                                                .flex()
                                                                .flex_col()
                                                                .gap(px(3.0))
                                                                .children(recent_events.into_iter().map(|gev| {
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(10.0))
                                                                        .text_color(theme.text_tertiary)
                                                                        .child(format!(
                                                                            "r{} {} — {}",
                                                                            gev.revision,
                                                                            gev.action,
                                                                            gev.detail
                                                                        ))
                                                                })),
                                                        )
                                                    }),
                                            ),
                                    )
                                })
                                // Subagents Parent-Child Tree
                                .child(
                                    gb_section_card(&theme)
                                        .child(gb_section_card_head(&theme).child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child(format!("Subagents ({})", subagents.len())),
                                        ))
                                        .child(if subagents.is_empty() {
                                            div()
                                                .p(px(12.0))
                                                .text_size(px(12.0))
                                                .text_color(theme.text_secondary)
                                                .child("No delegated subagents in this conversation yet.")
                                        } else {
                                            div()
                                                .p(px(8.0))
                                                .flex()
                                                .flex_col()
                                                .gap(px(6.0))
                                                .children(subagents.into_iter().enumerate().map(|(idx, node)| {
                                                    let tone = match node.status {
                                                        SubagentStatus::Running => BadgeTone::Accent,
                                                        SubagentStatus::Completed => BadgeTone::Success,
                                                        SubagentStatus::Failed => BadgeTone::Danger,
                                                        SubagentStatus::Interrupted => BadgeTone::Warning,
                                                    };
                                                    let indent = px((node.depth.saturating_sub(1).min(4) as f32) * 12.0);
                                                    let sid_stop = node.subagent_id.clone();
                                                    div()
                                                        .flex_shrink_0()
                                                        .ml(indent)
                                                        .p(px(9.0))
                                                        .rounded(px(10.0))
                                                        .bg(theme.bg_card)
                                                        .border_1()
                                                        .border_color(theme.border_subtle)
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(4.0))
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .justify_between()
                                                                .gap(px(6.0))
                                                                .child(
                                                                    div()
                                                                        .flex_1()
                                                                        .min_w_0()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap(px(5.0))
                                                                        .when(node.status == SubagentStatus::Running, |d| {
                                                                            d.child(gpu_spinner(11.0, theme.agent_accent))
                                                                        })
                                                                        .child(
                                                                            div()
                                                                                .text_size(px(12.0))
                                                                                .font_weight(FontWeight::MEDIUM)
                                                                                .text_color(theme.text_primary)
                                                                                .truncate()
                                                                                .child(node.goal.clone()),
                                                                        ),
                                                                )
                                                                .child(gb_badge(&theme, tone, node.status.as_str())),
                                                        )
                                                        .child(
                                                            div()
                                                                .font_family(FONT_MONO)
                                                                .text_size(px(10.5))
                                                                .text_color(theme.text_tertiary)
                                                                .child(format!(
                                                                    "{} → {} · {} tools{}",
                                                                    node.parent_agent_id,
                                                                    node.subagent_id,
                                                                    node.tool_count,
                                                                    node.duration_ms
                                                                        .map(|ms| format!(" · {}", format_elapsed_ms(ms)))
                                                                        .unwrap_or_default()
                                                                )),
                                                        )
                                                        .when(!node.current_operation.is_empty(), |d| {
                                                            d.child(
                                                                div()
                                                                    .text_size(px(11.5))
                                                                    .text_color(theme.text_secondary)
                                                                    .child(node.current_operation.clone()),
                                                            )
                                                        })
                                                        .when(node.status == SubagentStatus::Running, |d| {
                                                            d.child(
                                                                div().pt(px(2.0)).child(
                                                                    gb_btn(
                                                                        SharedString::from(format!("stop-sub-{idx}")),
                                                                        &theme,
                                                                        BtnVariant::Danger,
                                                                        true,
                                                                        false,
                                                                    )
                                                                    .child("Stop subagent")
                                                                    .on_click(cx.listener(move |v, _, _, cx| {
                                                                        v.app.update(cx, |s, cx| {
                                                                            s.interrupt_subagent(sid_stop.clone(), cx);
                                                                        });
                                                                    })),
                                                                ),
                                                            )
                                                        })
                                                }))
                                        }),
                                ),
                        )
                        .vertical_scrollbar(&self.agents_scroll)
                        .child(smooth_scroll_layer(&self.agents_smooth))
                        .into_any_element()
                }
                RightPaneTabId::Trajectory => {
                    let (events, focus_call) = {
                        let s = self.app.read(cx);
                        (s.active_trajectory(), s.inspect_trajectory_call_id.clone())
                    };
                    if let Some(ref call_id) = focus_call {
                        for ev in &events {
                            if ev.tool_call_id.as_deref() == Some(call_id.as_str()) {
                                self.expanded_trajectory.insert(ev.event_id.clone());
                            }
                        }
                    }
                    let total_ev = events.len();
                    let skip = total_ev.saturating_sub(self.shown_trajectory_events);
                    let visible: Vec<_> = events.into_iter().skip(skip).collect();

                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .relative()
                        .child(
                            div()
                                .id("trajectory-inspector-scroll")
                                .track_scroll(&self.trajectory_scroll)
                                .size_full()
                                .overflow_y_scroll()
                                .restrict_scroll_to_axis()
                                .p(px(12.0))
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child(format!("Trajectory ({total_ev} events · read-only)")),
                                        )
                                        .when(skip > 0, |d| {
                                            d.child(
                                                gb_btn("traj-load-more", &theme, BtnVariant::Secondary, true, false)
                                                    .child(format!("Load older ({skip})"))
                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                        v.shown_trajectory_events += 80;
                                                        cx.notify();
                                                    })),
                                            )
                                        }),
                                )
                                .when(visible.is_empty(), |d| {
                                    d.child(
                                        gb_section_card(&theme).child(
                                            div()
                                                .p(px(14.0))
                                                .text_size(px(12.0))
                                                .text_color(theme.text_secondary)
                                                .child("No persisted trajectory events in this conversation yet."),
                                        ),
                                    )
                                })
                                .children(visible.into_iter().enumerate().map(|(idx, ev)| {
                                    let ev_id = ev.event_id.clone();
                                    let expanded = self.expanded_trajectory.contains(&ev_id);
                                    let json_on = self.trajectory_json_mode.contains(&ev_id);
                                    let toggle_id = ev_id.clone();
                                    let json_toggle_id = ev_id.clone();
                                    let badge_tone = if ev.is_error {
                                        BadgeTone::Danger
                                    } else {
                                        match ev.kind {
                                            TrajectoryEventKind::PtcExecution | TrajectoryEventKind::PtcSubcall => BadgeTone::Accent,
                                            TrajectoryEventKind::GoalChanged => BadgeTone::Info,
                                            TrajectoryEventKind::TurnCompleted | TrajectoryEventKind::SubagentCompleted => BadgeTone::Success,
                                            TrajectoryEventKind::ApprovalRequested => BadgeTone::Warning,
                                            _ => BadgeTone::Muted,
                                        }
                                    };
                                    let tool_name = ev
                                        .payload
                                        .get("tool")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    let ptc_code = if is_ptc_tool(tool_name) {
                                        ev.payload
                                            .get("arguments")
                                            .and_then(|a| a.get("code"))
                                            .and_then(|c| c.as_str())
                                            .map(String::from)
                                    } else {
                                        None
                                    };
                                    let ptc_lang = if ptc_code.is_some() {
                                        let meta_lang = ev
                                            .payload
                                            .get("arguments")
                                            .and_then(|a| a.get("language"))
                                            .and_then(|l| l.as_str());
                                        resolve_recorded_language(tool_name, None, None, meta_lang)
                                    } else {
                                        None
                                    };
                                    let raw_payload = serde_json::to_string_pretty(&ev.payload)
                                        .unwrap_or_else(|_| ev.payload.to_string());
                                    let id_line = format!(
                                        "seq #{} · T{} S{} · agent={} · run={}{}",
                                        ev.seq,
                                        ev.turn,
                                        ev.step,
                                        ev.agent_id,
                                        ev.run_id.chars().take(10).collect::<String>(),
                                        ev.tool_call_id
                                            .as_deref()
                                            .map(|c| format!(" · call={}", c.chars().take(12).collect::<String>()))
                                            .unwrap_or_default()
                                    );

                                    div()
                                        .flex_shrink_0()
                                        .rounded(px(10.0))
                                        .bg(theme.bg_card)
                                        .border_1()
                                        .border_color(if focus_call.as_deref() == ev.tool_call_id.as_deref() && ev.tool_call_id.is_some() {
                                            theme.agent_accent
                                        } else {
                                            theme.border_subtle
                                        })
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .id(SharedString::from(format!("traj-row-{idx}")))
                                                .px(px(10.0))
                                                .py(px(7.0))
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .gap(px(6.0))
                                                .cursor_pointer()
                                                .hover(|st| st.bg(theme.bg_hover))
                                                .on_click(cx.listener(move |v, _, _, cx| {
                                                    if !v.expanded_trajectory.remove(&toggle_id) {
                                                        v.expanded_trajectory.insert(toggle_id.clone());
                                                    }
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(2.0))
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap(px(6.0))
                                                                .child(gb_badge(&theme, badge_tone, ev.kind.badge_label()))
                                                                .child(
                                                                    div()
                                                                        .text_size(px(12.0))
                                                                        .font_weight(FontWeight::MEDIUM)
                                                                        .text_color(theme.text_primary)
                                                                        .truncate()
                                                                        .child(ev.title.clone()),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(11.0))
                                                                .text_color(theme.text_secondary)
                                                                .truncate()
                                                                .child(ev.summary.clone()),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(5.0))
                                                        .when_some(ev.duration_ms, |d, ms| {
                                                            d.child(
                                                                div()
                                                                    .font_family(FONT_MONO)
                                                                    .text_size(px(10.0))
                                                                    .text_color(theme.text_tertiary)
                                                                    .child(format_elapsed_ms(ms)),
                                                            )
                                                        })
                                                        .child(cursor_icon(
                                                            if expanded { "chevron-down" } else { "chevron-right" },
                                                            11.0,
                                                            theme.text_tertiary,
                                                        )),
                                                ),
                                        )
                                        .when(expanded, |d| {
                                            let copy_src = ptc_code.clone().unwrap_or_else(|| raw_payload.clone());
                                            d.child(
                                                div()
                                                    .px(px(10.0))
                                                    .pb(px(10.0))
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(6.0))
                                                    .child(
                                                        div()
                                                            .font_family(FONT_MONO)
                                                            .text_size(px(10.0))
                                                            .text_color(theme.text_tertiary)
                                                            .child(id_line),
                                                    )
                                                    .when(ptc_code.is_some(), |d| {
                                                        d.child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .justify_between()
                                                                .gap(px(6.0))
                                                                .child(
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(10.0))
                                                                        .text_color(theme.text_tertiary)
                                                                        .child(if json_on {
                                                                            "Original recorded JSON".to_string()
                                                                        } else {
                                                                            format!(
                                                                                "PTC Source · {}",
                                                                                ptc_lang.map(|l| l.display_label()).unwrap_or("Plain text")
                                                                            )
                                                                        }),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .gap(px(4.0))
                                                                        .child(
                                                                            gb_btn(
                                                                                SharedString::from(format!("traj-json-{idx}")),
                                                                                &theme,
                                                                                if json_on { BtnVariant::Primary } else { BtnVariant::Ghost },
                                                                                true,
                                                                                false,
                                                                            )
                                                                            .child("{}")
                                                                            .on_click(cx.listener(move |v, _, _, cx| {
                                                                                if !v.trajectory_json_mode.remove(&json_toggle_id) {
                                                                                    v.trajectory_json_mode.insert(json_toggle_id.clone());
                                                                                }
                                                                                cx.notify();
                                                                            })),
                                                                        )
                                                                        .child(
                                                                            gb_btn(
                                                                                SharedString::from(format!("traj-copy-{idx}")),
                                                                                &theme,
                                                                                BtnVariant::Ghost,
                                                                                true,
                                                                                false,
                                                                            )
                                                                            .child("Copy")
                                                                            .on_click(move |_, _, cx| {
                                                                                cx.write_to_clipboard(ClipboardItem::new_string(copy_src.clone()));
                                                                            }),
                                                                        ),
                                                                ),
                                                        )
                                                    })
                                                    .child(if let Some(ref code) = ptc_code {
                                                        if json_on {
                                                            render_highlighted_code_block(
                                                                SharedString::from(format!("traj-raw-{idx}")),
                                                                Some("json"),
                                                                &raw_payload,
                                                                false,
                                                                false,
                                                                &theme,
                                                            )
                                                        } else {
                                                            render_highlighted_code_block(
                                                                SharedString::from(format!("traj-code-{idx}")),
                                                                ptc_lang.map(|l| l.as_str()),
                                                                code,
                                                                true,
                                                                false,
                                                                &theme,
                                                            )
                                                        }
                                                    } else {
                                                        render_highlighted_code_block(
                                                            SharedString::from(format!("traj-pl-{idx}")),
                                                            Some("json"),
                                                            &raw_payload,
                                                            false,
                                                            false,
                                                            &theme,
                                                        )
                                                    }),
                                            )
                                        })
                                })),
                        )
                        .vertical_scrollbar(&self.trajectory_scroll)
                        .child(smooth_scroll_layer(&self.trajectory_smooth))
                        .into_any_element()
                }
                RightPaneTabId::Bot => {
                    let seed_edit = bot_seed.clone();
                    let display_name = bot_override
                        .as_ref()
                        .and_then(|o| o.display_name.clone())
                        .unwrap_or_else(|| active_profile.clone());
                    let title_edit = display_name.clone();
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .relative()
                        .child(self.bot_nested_scroll.reset_marker())
                        .child(
                            div()
                                .id("bot-inspector-scroll")
                                .track_scroll(&self.bot_scroll)
                                .size_full()
                                .overflow_y_scroll()
                                .restrict_scroll_to_axis()
                                .p(px(14.0))
                        .flex()
                        .flex_col()
                        .gap(px(14.0))
                        // Bot identity header card
                        .child(
                            gb_section_card(&theme).child(
                                div()
                                    .p(px(16.0))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(10.0))
                                    .child(bot_face_with_options(
                                        &theme,
                                        &bot_seed,
                                        bot_override.as_ref().and_then(|o| o.shape_id.as_deref()),
                                        bot_override.as_ref().and_then(|o| o.color_id.as_deref()),
                                        56.0,
                                        bot_state,
                                        BotFaceOptions { slot_key: "inspector", follow_pointer: true, clickable_spin: true, ..Default::default() },
                                    ))
                                    .child(
                                        div()
                                            .text_size(px(15.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child(display_name),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .justify_center()
                                            .gap(px(6.0))
                                            .child(
                                                gb_btn("bot-edit-av", &theme, BtnVariant::Primary, true, false)
                                                    .child("Customize Face")
                                                    .on_click(cx.listener(move |v, _, _, cx| {
                                                        v.app.update(cx, |s, cx| {
                                                            s.bump_shell();
                                                            s.avatar_editor_target = Some(AvatarEditorTarget {
                                                                seed: seed_edit.clone(),
                                                                title: title_edit.clone(),
                                                            });
                                                            cx.notify();
                                                        });
                                                    })),
                                            )
                                            .child(
                                                gb_btn(
                                                    "bot-refresh-models",
                                                    &theme,
                                                    BtnVariant::Secondary,
                                                    true,
                                                    refreshing_models,
                                                )
                                                .when(refreshing_models, |d| d.child(gpu_spinner(12.0, theme.text_primary)))
                                                .child(if refreshing_models {
                                                    "Refreshing…"
                                                } else {
                                                    "Refresh Models"
                                                })
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.app.update(cx, |s, cx| s.refresh_models(cx));
                                                })),
                                            )
                                            .child(
                                                gb_btn(
                                                    "bot-restart-be",
                                                    &theme,
                                                    BtnVariant::Secondary,
                                                    true,
                                                    restarting_backend,
                                                )
                                                .when(restarting_backend, |d| d.child(gpu_spinner(12.0, theme.text_primary)))
                                                .child(if restarting_backend { "Restarting…" } else { "Restart Backend" })
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.app.update(cx, |s, cx| s.restart_backend(cx));
                                                })),
                                            ),
                                    ),
                            ),
                        )
                        // SOUL.md inline editor
                        .child(
                            gb_section_card(&theme)
                                .child(
                                    gb_section_card_head(&theme)
                                        .child(
                                            div()
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_primary)
                                                .child("Bot Persona (SOUL.md)"),
                                        )
                                        .child(
                                            gb_btn("bot-save-soul", &theme, BtnVariant::Primary, true, false)
                                                .child("Save")
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    let txt = v.soul_input.read(cx).value().to_string();
                                                    v.app.update(cx, |s, cx| {
                                                        s.save_memory_file("soul", txt, cx);
                                                    });
                                                })),
                                        ),
                                )
                                .child(
                                    div()
                                        .p(px(10.0))
                                        .font_family(FONT_MONO)
                                        .relative()
                                        .child(self.bot_nested_scroll.mark_nested())
                                        .child(Textarea::new(&self.soul_input)),
                                ),
                        )
                        // Rust Native Runtime Stats card
                        .child(
                            gb_section_card(&theme)
                                .child(gb_section_card_head(&theme).child(
                                    div()
                                        .text_size(px(12.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text_primary)
                                        .child("Rust GPUI Runtime"),
                                ))
                                .child(
                                    div()
                                        .p(px(12.0))
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .font_family(FONT_MONO)
                                        .text_size(px(11.5))
                                        .text_color(theme.text_secondary)
                                        .when_some(sys_stats, |d, st| {
                                            d.child(format!("Host: {}", st.hostname))
                                                .child(format!("OS:   {}", st.os_name))
                                                .child(format!(
                                                    "RAM:  {} / {} MB ({:.0}%)",
                                                    st.memory_used_mb,
                                                    st.memory_total_mb,
                                                    st.memory_percent
                                                ))
                                                .child(format!("Load: {}", st.load_avg))
                                        })
                                        .when_some(boot_ms, |d, ms| {
                                            d.child(format!("Boot: {ms} ms"))
                                        })
                                        .when_some(backend_info.and_then(|b| b.pid), |d, pid| {
                                            d.child(format!("Hermes PID: {pid}"))
                                        })
                                        .child({
                                            let fs = face_frame_stats();
                                            if fs.samples > 0 {
                                                format!(
                                                    "Face GPU: {:.1} fps · avg {:.2}ms · p95 {:.2}ms ({} samples)",
                                                    fs.estimated_fps, fs.avg_ms, fs.p95_ms, fs.samples
                                                )
                                            } else {
                                                "Face GPU: idle (0 samples)".to_string()
                                            }
                                        }),
                                ),
                        )
                        )
                        .vertical_scrollbar(&self.bot_scroll)
                        .child(smooth_scroll_layer(&self.bot_smooth).yield_to(&self.bot_nested_scroll))
                        .into_any_element()
                }
            })));

        // Clip a full-width panel so the slide doesn't squeeze its controls or PTY.
        div()
            .w(width * self.open_progress)
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .opacity(self.open_progress)
            .flex()
            .child(panel)
    }
}

fn format_bytes(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}
