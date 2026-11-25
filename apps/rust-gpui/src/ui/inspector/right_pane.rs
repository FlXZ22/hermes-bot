use crate::state::app_state::{AppState, AvatarEditorTarget, RightPaneTabId};
use crate::theme::bot_face::bot_face;
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_mini_icon_btn, gb_section_card, gb_section_card_head,
    gb_segmented_item, gb_segmented_shell, BadgeTone, BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::ui::inspector::terminal_view::TerminalView;
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement;
use std::path::Path;

pub struct RightSidePaneView {
    app: Entity<AppState>,
    terminal: Entity<TerminalView>,
    cwd_input: Entity<InputState>,
    soul_input: Entity<TextareaState>,
    synced_cwd: String,
    synced_soul: String,
    width_px: f32,
    files_scroll: ScrollHandle,
    files_smooth: SmoothScroller,
    preview_scroll: ScrollHandle,
    preview_smooth: SmoothScroller,
    git_scroll: ScrollHandle,
    git_smooth: SmoothScroller,
    bot_scroll: ScrollHandle,
    bot_smooth: SmoothScroller,
}

impl RightSidePaneView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (cwd, soul) = {
            let s = app.read(cx);
            (s.inspector_cwd.clone(), s.soul_md.clone())
        };
        let terminal = cx.new(|cx| TerminalView::new(app.clone(), window, cx));
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
            if app_e.read(cx).right_pane_open {
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
        let bot_scroll = ScrollHandle::new();
        let bot_smooth = SmoothScroller::for_handle(bot_scroll.clone());
        Self {
            app,
            terminal,
            cwd_input,
            soul_input,
            synced_cwd: cwd,
            synced_soul: soul,
            width_px: 380.0,
            files_scroll,
            files_smooth,
            preview_scroll,
            preview_smooth,
            git_scroll,
            git_smooth,
            bot_scroll,
            bot_smooth,
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

    fn cycle_width(&mut self, cx: &mut Context<Self>) {
        self.width_px = if self.width_px < 420.0 {
            480.0
        } else if self.width_px < 560.0 {
            600.0
        } else {
            360.0
        };
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
            anim_tick,
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
            let ov = s.bot_avatars.get(&seed).cloned();
            let chat_state = s.active_chat().bot_state();
            (
                s.sand_theme(),
                s.theme_mode(),
                s.right_pane_tab,
                s.inspector_cwd.clone(),
                s.inspector_files.clone(),
                s.inspector_file_error.clone(),
                s.inspector_preview_path.clone(),
                s.inspector_preview_content.clone(),
                s.inspector_git.clone(),
                s.inspector_git_loading,
                s.inspector_git_error.clone(),
                seed,
                ov,
                chat_state,
                s.anim_tick,
                s.active_profile.clone(),
                s.soul_md.clone(),
                s.system_stats.clone(),
                s.bootstrap_ms,
                s.rust_backend.clone(),
                s.refreshing_models,
                s.restarting_backend,
            )
        };

        if cwd != self.synced_cwd {
            self.synced_cwd = cwd.clone();
            self.cwd_input
                .update(cx, |st, cx| st.set_value(cwd.clone(), window, cx));
        }
        if soul_md != self.synced_soul {
            self.synced_soul = soul_md.clone();
            self.soul_input
                .update(cx, |st, cx| st.set_value(soul_md.clone(), window, cx));
        }

        let tabs: &[(RightPaneTabId, &str)] = &[
            (RightPaneTabId::Files, "Files"),
            (RightPaneTabId::Terminal, "Term"),
            (RightPaneTabId::Git, "Git"),
            (RightPaneTabId::Bot, "Bot"),
        ];

        div()
            .w(px(self.width_px))
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
                    .gap(px(6.0))
                    .child(gb_segmented_shell(&theme).children(tabs.iter().map(
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
                                            cx.notify();
                                        });
                                    })),
                            ),
                    ),
            )
            // Body
            .child(match tab {
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
                            let content_copy = preview_content.clone();
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
                                            gb_btn("files-copy", &theme, BtnVariant::Ghost, true, false)
                                                .child("Copy")
                                                .on_click(cx.listener(move |v, _, _, cx| {
                                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                                        content_copy.clone(),
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
                                                .overflow_y_scroll()
                                                .p(px(12.0))
                                                .bg(theme.bg_tertiary)
                                                .font_family(FONT_MONO)
                                                .text_size(px(11.5))
                                                .text_color(theme.text_primary)
                                                .child(preview_content),
                                        )
                                        .vertical_scrollbar(&self.preview_scroll)
                                        .child(smooth_scroll_layer(&self.preview_smooth)),
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
                    .child(self.terminal.clone())
                    .into_any_element(),
                RightPaneTabId::Git => {
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
                                .p(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(cursor_icon("git-branch", 14.0, theme.text_secondary))
                                        .child(gb_badge(
                                            &theme,
                                            BadgeTone::Accent,
                                            git_status
                                                .as_ref()
                                                .filter(|g| g.is_repo && !g.branch.is_empty())
                                                .map(|g| g.branch.clone())
                                                .unwrap_or_else(|| "Not a Git repo".into()),
                                        )),
                                )
                                .child(
                                    gb_btn("git-refresh-btn", &theme, BtnVariant::Secondary, true, git_loading)
                                        .child(cursor_icon("refresh", 12.0, theme.text_primary))
                                        .child(if git_loading { "Loading…" } else { "Refresh" })
                                        .on_click(cx.listener(|v, _, _, cx| {
                                            v.app.update(cx, |s, cx| s.refresh_inspector_git(cx));
                                        })),
                                ),
                        )
                        .when_some(git_err, |d, err| {
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
                        .when_some(git_status.filter(|g| g.is_repo), |d, git| {
                            d.child(
                                gb_section_card(&theme)
                                    .child(gb_section_card_head(&theme).child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child(format!(
                                                "Changed Files ({})",
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
                                            .children(git.changed_files.iter().map(|chg| {
                                                let tone = if chg.status.contains('D') {
                                                    BadgeTone::Danger
                                                } else if chg.status.contains('?')
                                                    || chg.status.contains('A')
                                                {
                                                    BadgeTone::Success
                                                } else {
                                                    BadgeTone::Warning
                                                };
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(7.0))
                                                    .px(px(6.0))
                                                    .py(px(3.0))
                                                    .child(gb_badge(&theme, tone, chg.status.clone()))
                                                    .child(
                                                        div()
                                                            .font_family(FONT_MONO)
                                                            .text_size(px(11.5))
                                                            .text_color(theme.text_primary)
                                                            .truncate()
                                                            .child(chg.path.clone()),
                                                    )
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
                                                .child("Diff"),
                                        ))
                                        .child(
                                            div()
                                                .p(px(10.0))
                                                .bg(theme.bg_tertiary)
                                                .font_family(FONT_MONO)
                                                .text_size(px(11.0))
                                                .flex()
                                                .flex_col()
                                                .children(git.diff.lines().take(250).map(|line| {
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
                                                        .text_color(color)
                                                        .child(line.to_string())
                                                })),
                                        ),
                                )
                            })
                        })
                        )
                        .vertical_scrollbar(&self.git_scroll)
                        .child(smooth_scroll_layer(&self.git_smooth))
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
                        .child(
                            div()
                                .id("bot-inspector-scroll")
                                .track_scroll(&self.bot_scroll)
                                .size_full()
                                .overflow_y_scroll()
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
                                    .child(bot_face(
                                        &theme,
                                        &bot_seed,
                                        bot_override.as_ref().and_then(|o| o.shape_id.as_deref()),
                                        bot_override.as_ref().and_then(|o| o.color_id.as_deref()),
                                        56.0,
                                        bot_state,
                                        anim_tick,
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
                                                .child("Restart Backend")
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
                                        }),
                                ),
                        )
                        )
                        .vertical_scrollbar(&self.bot_scroll)
                        .child(smooth_scroll_layer(&self.bot_smooth))
                        .into_any_element()
                }
            })
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
