use crate::theme::components::{cursor_icon, FONT_MONO, FONT_SANS};
use crate::theme::tokens::SandTheme;
use gpui::*;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Native GPU Markdown renderer using `pulldown-cmark` supporting GFM tables,
/// headings, blockquotes, lists, task lists, inline code, and fenced code blocks with copy button.
pub fn render_markdown(
    id_prefix: &str,
    source: &str,
    theme: &SandTheme,
) -> Div {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(source, opts);

    let mut container = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .font_family(FONT_SANS)
        .text_size(px(14.5))
        .line_height(px(23.0))
        .text_color(theme.text_primary);

    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();
    let mut code_idx = 0usize;

    let mut in_heading: Option<HeadingLevel> = None;
    let mut in_blockquote = false;
    let mut in_list_item = false;
    let mut list_ordered_idx: Option<u64> = None;
    let mut task_checked: Option<bool> = None;

    let mut in_table = false;
    let mut table_headers: Vec<String> = Vec::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();

    let mut text_buf = String::new();
    let mut bold = false;
    let mut italic = false;

    let flush_text = |container: &mut Div,
                      buf: &mut String,
                      heading: Option<HeadingLevel>,
                      blockquote: bool,
                      list_item: bool,
                      ordered: Option<u64>,
                      checked: Option<bool>,
                      theme: &SandTheme| {
        let trimmed = buf.trim_end();
        if trimmed.is_empty() {
            buf.clear();
            return;
        }
        let content = trimmed.to_string();
        buf.clear();

        if let Some(level) = heading {
            let sz = match level {
                HeadingLevel::H1 => 20.0,
                HeadingLevel::H2 => 17.5,
                HeadingLevel::H3 => 15.5,
                _ => 14.5,
            };
            *container = std::mem::replace(container, div()).child(
                div()
                    .w_full()
                    .mt(px(4.0))
                    .text_size(px(sz))
                    .line_height(px(sz * 1.3))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(content),
            );
        } else if blockquote {
            *container = std::mem::replace(container, div()).child(
                div()
                    .w_full()
                    .pl(px(12.0))
                    .pr(px(10.0))
                    .py(px(6.0))
                    .border_l_2()
                    .border_color(theme.agent_accent)
                    .bg(theme.bg_card)
                    .rounded_r(px(8.0))
                    .text_color(theme.text_secondary)
                    .child(content),
            );
        } else if list_item {
            let prefix = if let Some(chk) = checked {
                if chk { "☑ " } else { "☐ " }.to_string()
            } else if let Some(n) = ordered {
                format!("{n}. ")
            } else {
                "• ".to_string()
            };
            *container = std::mem::replace(container, div()).child(
                div()
                    .w_full()
                    .pl(px(14.0))
                    .flex()
                    .items_start()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_color(theme.text_tertiary)
                            .flex_shrink_0()
                            .child(prefix),
                    )
                    .child(div().flex_1().min_w_0().child(content)),
            );
        } else {
            *container = std::mem::replace(container, div()).child(
                div().w_full().min_w_0().child(content),
            );
        }
    };

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
                in_code_block = true;
                code_buf.clear();
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => "text".into(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let code_str = code_buf.trim_end_matches('\n').to_string();
                let lang_str = if code_lang.is_empty() {
                    "text".to_string()
                } else {
                    code_lang.clone()
                };
                let copy_id = SharedString::from(format!("{id_prefix}-code-{code_idx}"));
                code_idx += 1;
                container = container.child(render_code_block(copy_id, &lang_str, &code_str, theme));
                code_buf.clear();
                code_lang.clear();
            }
            Event::Start(Tag::Heading { level, .. }) => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
                in_heading = Some(level);
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
                in_heading = None;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                in_blockquote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
                in_blockquote = false;
            }
            Event::Start(Tag::List(start)) => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
                list_ordered_idx = start;
            }
            Event::End(TagEnd::List(_)) => {
                list_ordered_idx = None;
            }
            Event::Start(Tag::Item) => {
                in_list_item = true;
                task_checked = None;
            }
            Event::End(TagEnd::Item) => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
                if let Some(n) = list_ordered_idx.as_mut() {
                    *n += 1;
                }
                in_list_item = false;
                task_checked = None;
            }
            Event::TaskListMarker(chk) => {
                task_checked = Some(chk);
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
            }
            Event::Start(Tag::Table(_)) => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
                in_table = true;
                table_headers.clear();
                table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                container = container.child(render_table(&table_headers, &table_rows, theme));
                table_headers.clear();
                table_rows.clear();
            }
            Event::Start(Tag::TableHead) => {
                current_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                table_headers = std::mem::take(&mut current_row);
            }
            Event::Start(Tag::TableRow) => {
                current_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                table_rows.push(std::mem::take(&mut current_row));
            }
            Event::Start(Tag::TableCell) => {
                current_cell.clear();
            }
            Event::End(TagEnd::TableCell) => {
                current_row.push(std::mem::take(&mut current_cell));
            }
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Rule => {
                flush_text(
                    &mut container,
                    &mut text_buf,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                    theme,
                );
                container = container.child(
                    div()
                        .w_full()
                        .h(px(1.0))
                        .my(px(4.0))
                        .bg(theme.border_default),
                );
            }
            Event::Code(c) => {
                if in_table {
                    current_cell.push('`');
                    current_cell.push_str(&c);
                    current_cell.push('`');
                } else {
                    text_buf.push('`');
                    text_buf.push_str(&c);
                    text_buf.push('`');
                }
            }
            Event::Text(t) => {
                if in_code_block {
                    code_buf.push_str(&t);
                } else if in_table {
                    current_cell.push_str(&t);
                } else {
                    let _ = (bold, italic);
                    text_buf.push_str(&t);
                }
            }
            Event::SoftBreak => {
                if in_code_block {
                    code_buf.push('\n');
                } else if in_table {
                    current_cell.push(' ');
                } else {
                    text_buf.push(' ');
                }
            }
            Event::HardBreak => {
                if in_code_block {
                    code_buf.push('\n');
                } else {
                    text_buf.push('\n');
                }
            }
            _ => {}
        }
    }

    if in_code_block && !code_buf.is_empty() {
        let copy_id = SharedString::from(format!("{id_prefix}-code-tail"));
        let lang_str = if code_lang.is_empty() { "text" } else { &code_lang };
        container = container.child(render_code_block(copy_id, lang_str, &code_buf, theme));
    } else {
        flush_text(
            &mut container,
            &mut text_buf,
            in_heading,
            in_blockquote,
            in_list_item,
            list_ordered_idx,
            task_checked,
            theme,
        );
    }

    container
}

pub fn render_code_block(
    id: SharedString,
    lang: &str,
    code: &str,
    theme: &SandTheme,
) -> Div {
    let code_for_copy = code.to_string();
    div()
        .w_full()
        .my(px(4.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(theme.border_default)
        .bg(theme.bg_chrome)
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .px(px(10.0))
                .py(px(5.0))
                .border_b_1()
                .border_color(theme.border_default)
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(11.5))
                .text_color(theme.text_tertiary)
                .child(lang.to_string())
                .child(
                    div()
                        .id(id)
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.bg_hover).text_color(theme.text_primary))
                        .child(cursor_icon("copy", 11.0, theme.text_tertiary))
                        .child("Copy")
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(code_for_copy.clone()));
                        }),
                ),
        )
        .child(
            div()
                .w_full()
                .px(px(12.0))
                .py(px(10.0))
                .font_family(FONT_MONO)
                .text_size(px(12.5))
                .line_height(px(19.0))
                .text_color(theme.text_primary)
                .child(code.to_string()),
        )
}

fn render_table(headers: &[String], rows: &[Vec<String>], theme: &SandTheme) -> Div {
    let mut table = div()
        .w_full()
        .my(px(4.0))
        .rounded(px(9.0))
        .border_1()
        .border_color(theme.border_default)
        .overflow_hidden()
        .flex()
        .flex_col()
        .text_size(px(13.0));

    if !headers.is_empty() {
        table = table.child(
            div()
                .w_full()
                .bg(theme.bg_card)
                .border_b_1()
                .border_color(theme.border_default)
                .flex()
                .children(headers.iter().map(|h| {
                    div()
                        .flex_1()
                        .min_w(px(80.0))
                        .px(px(10.0))
                        .py(px(7.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(h.clone())
                })),
        );
    }

    for row in rows {
        table = table.child(
            div()
                .w_full()
                .border_b_1()
                .border_color(theme.border_hairline)
                .flex()
                .children(row.iter().map(|c| {
                    div()
                        .flex_1()
                        .min_w(px(80.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .text_color(theme.text_secondary)
                        .child(c.clone())
                })),
        );
    }

    table
}
