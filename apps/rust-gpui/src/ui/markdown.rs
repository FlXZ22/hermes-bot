use crate::theme::components::{cursor_icon, FONT_MONO, FONT_SANS};
use crate::theme::tokens::SandTheme;
use gpui::*;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct RichRun {
    range: Range<usize>,
    bold: bool,
    italic: bool,
    code: bool,
    strikethrough: bool,
}

#[derive(Clone, Debug)]
struct RichText {
    text: String,
    runs: Vec<RichRun>,
}

#[derive(Clone, Debug)]
enum MdBlock {
    Heading {
        level: u8,
        content: RichText,
    },
    Paragraph(RichText),
    Blockquote(RichText),
    ListItem {
        ordered: Option<u64>,
        task_checked: Option<bool>,
        content: RichText,
    },
    CodeBlock {
        lang: String,
        code: String,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Rule,
}

struct CachedDoc {
    source: Arc<str>,
    blocks: Arc<Vec<MdBlock>>,
}

const MD_CACHE_LIMIT: usize = 256;

thread_local! {
    static MD_CACHE: RefCell<HashMap<(usize, u64), CachedDoc>> = RefCell::new(HashMap::with_capacity(128));
}

fn hash_str(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn parse_markdown_cached(source: &str) -> Arc<Vec<MdBlock>> {
    let key = (source.len(), hash_str(source.as_bytes()));
    if let Some(hit) = MD_CACHE.with(|cache| {
        cache
            .borrow()
            .get(&key)
            .filter(|doc| doc.source.as_ref() == source)
            .map(|doc| doc.blocks.clone())
    }) {
        return hit;
    }

    let blocks = Arc::new(parse_markdown_ast(source));
    MD_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if map.len() >= MD_CACHE_LIMIT {
            map.clear();
        }
        map.insert(
            key,
            CachedDoc {
                source: Arc::from(source),
                blocks: blocks.clone(),
            },
        );
    });
    blocks
}

fn parse_markdown_ast(source: &str) -> Vec<MdBlock> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(source, opts);
    let mut blocks = Vec::new();

    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();

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
    let mut runs: Vec<RichRun> = Vec::new();
    let mut bold = false;
    let mut italic = false;
    let mut strikethrough = false;

    let flush_rich = |blocks: &mut Vec<MdBlock>,
                      buf: &mut String,
                      runs: &mut Vec<RichRun>,
                      heading: Option<HeadingLevel>,
                      blockquote: bool,
                      list_item: bool,
                      ordered: Option<u64>,
                      checked: Option<bool>| {
        let trimmed_len = buf.trim_end().len();
        if trimmed_len == 0 {
            buf.clear();
            runs.clear();
            return;
        }
        buf.truncate(trimmed_len);
        let text = std::mem::take(buf);
        let mut out_runs = std::mem::take(runs);
        out_runs.retain_mut(|r| {
            if r.range.start >= trimmed_len {
                false
            } else {
                r.range.end = r.range.end.min(trimmed_len);
                r.range.start < r.range.end
            }
        });
        let rich = RichText {
            text,
            runs: out_runs,
        };

        if let Some(level) = heading {
            let lvl = match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                _ => 4,
            };
            blocks.push(MdBlock::Heading {
                level: lvl,
                content: rich,
            });
        } else if blockquote {
            blocks.push(MdBlock::Blockquote(rich));
        } else if list_item {
            blocks.push(MdBlock::ListItem {
                ordered,
                task_checked: checked,
                content: rich,
            });
        } else {
            blocks.push(MdBlock::Paragraph(rich));
        }
    };

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                );
                in_code_block = true;
                code_buf.clear();
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => "text".to_string(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let lang = if code_lang.trim().is_empty() {
                    "code".to_string()
                } else {
                    code_lang.trim().to_string()
                };
                blocks.push(MdBlock::CodeBlock {
                    lang,
                    code: code_buf.trim_end_matches('\n').to_string(),
                });
                code_buf.clear();
                code_lang.clear();
            }
            Event::Start(Tag::Heading { level, .. }) => {
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                );
                in_heading = Some(level);
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                );
                in_heading = None;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                );
                in_blockquote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                );
                in_blockquote = false;
            }
            Event::Start(Tag::List(start)) => {
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
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
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                );
                if let Some(ref mut n) = list_ordered_idx {
                    *n += 1;
                }
                in_list_item = false;
                task_checked = None;
            }
            Event::TaskListMarker(checked) => {
                task_checked = Some(checked);
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if !in_list_item {
                    flush_rich(
                        &mut blocks,
                        &mut text_buf,
                        &mut runs,
                        in_heading,
                        in_blockquote,
                        false,
                        None,
                        None,
                    );
                }
            }
            Event::Start(Tag::Table(_)) => {
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                );
                in_table = true;
                table_headers.clear();
                table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                blocks.push(MdBlock::Table {
                    headers: std::mem::take(&mut table_headers),
                    rows: std::mem::take(&mut table_rows),
                });
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
            Event::Start(Tag::Strikethrough) => strikethrough = true,
            Event::End(TagEnd::Strikethrough) => strikethrough = false,
            Event::Text(t) => {
                if in_code_block {
                    code_buf.push_str(&t);
                } else if in_table {
                    current_cell.push_str(&t);
                } else {
                    let start = text_buf.len();
                    text_buf.push_str(&t);
                    let end = text_buf.len();
                    if (bold || italic || strikethrough) && start < end {
                        runs.push(RichRun {
                            range: start..end,
                            bold,
                            italic,
                            code: false,
                            strikethrough,
                        });
                    }
                }
            }
            Event::Code(c) => {
                if in_table {
                    current_cell.push_str(&c);
                } else {
                    let start = text_buf.len();
                    text_buf.push_str(&c);
                    let end = text_buf.len();
                    if start < end {
                        runs.push(RichRun {
                            range: start..end,
                            bold,
                            italic: false,
                            code: true,
                            strikethrough,
                        });
                    }
                }
            }
            Event::SoftBreak => {
                if in_code_block {
                    code_buf.push('\n');
                } else if in_table {
                    current_cell.push(' ');
                } else {
                    text_buf.push('\n');
                }
            }
            Event::HardBreak => {
                if in_code_block {
                    code_buf.push('\n');
                } else if in_table {
                    current_cell.push(' ');
                } else {
                    text_buf.push('\n');
                }
            }
            Event::Rule => {
                flush_rich(
                    &mut blocks,
                    &mut text_buf,
                    &mut runs,
                    in_heading,
                    in_blockquote,
                    in_list_item,
                    list_ordered_idx,
                    task_checked,
                );
                blocks.push(MdBlock::Rule);
            }
            _ => {}
        }
    }

    flush_rich(
        &mut blocks,
        &mut text_buf,
        &mut runs,
        in_heading,
        in_blockquote,
        in_list_item,
        list_ordered_idx,
        task_checked,
    );

    if in_code_block && !code_buf.is_empty() {
        let lang = if code_lang.trim().is_empty() {
            "code".to_string()
        } else {
            code_lang.trim().to_string()
        };
        blocks.push(MdBlock::CodeBlock {
            lang,
            code: code_buf,
        });
    }

    blocks
}

fn styled_rich_text(rich: &RichText, theme: &SandTheme) -> StyledText {
    if rich.runs.is_empty() {
        return StyledText::new(rich.text.clone());
    }
    let highlights: Vec<(Range<usize>, HighlightStyle)> = rich
        .runs
        .iter()
        .map(|run| {
            let mut style = HighlightStyle::default();
            if run.bold {
                style.font_weight = Some(FontWeight::SEMIBOLD);
            }
            if run.italic {
                style.font_style = Some(FontStyle::Italic);
            }
            if run.strikethrough {
                style.strikethrough = Some(StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(theme.text_tertiary),
                });
            }
            if run.code {
                style.background_color = Some(theme.bg_tertiary);
                style.color = Some(theme.agent_accent);
            }
            (run.range.clone(), style)
        })
        .collect();
    StyledText::new(rich.text.clone()).with_highlights(highlights)
}

/// Native GPU Markdown renderer using cached `pulldown-cmark` AST + GPUI `StyledText`.
/// Supporting GFM tables, headings, blockquotes, lists, task lists, rich inline runs,
/// and fenced code blocks with copy button at O(1) cache lookup speed.
pub fn render_markdown(id_prefix: &str, source: &str, theme: &SandTheme) -> Div {
    let blocks = parse_markdown_cached(source);

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

    let mut code_idx = 0usize;
    for block in blocks.iter() {
        match block {
            MdBlock::Heading { level, content } => {
                let (sz, lh) = match level {
                    1 => (19.0, 26.0),
                    2 => (16.5, 24.0),
                    3 => (15.0, 22.0),
                    _ => (14.5, 21.0),
                };
                container = container.child(
                    div()
                        .pt(px(4.0))
                        .text_size(px(sz))
                        .line_height(px(lh))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(styled_rich_text(content, theme)),
                );
            }
            MdBlock::Paragraph(content) => {
                container = container.child(div().w_full().child(styled_rich_text(content, theme)));
            }
            MdBlock::Blockquote(content) => {
                container = container.child(
                    div()
                        .w_full()
                        .pl(px(12.0))
                        .py(px(2.0))
                        .border_l_2()
                        .border_color(theme.border_primary)
                        .text_color(theme.text_secondary)
                        .child(styled_rich_text(content, theme)),
                );
            }
            MdBlock::ListItem {
                ordered,
                task_checked,
                content,
            } => {
                let bullet = if let Some(chk) = task_checked {
                    if *chk { "☑" } else { "☐" }.to_string()
                } else if let Some(n) = ordered {
                    format!("{n}.")
                } else {
                    "•".to_string()
                };
                container = container.child(
                    div()
                        .w_full()
                        .pl(px(6.0))
                        .flex()
                        .items_start()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_color(theme.text_tertiary)
                                .flex_shrink_0()
                                .child(bullet),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(styled_rich_text(content, theme)),
                        ),
                );
            }
            MdBlock::CodeBlock { lang, code } => {
                let cid = SharedString::from(format!("{id_prefix}-code-{code_idx}"));
                code_idx += 1;
                container = container.child(render_code_block(cid, lang, code, theme));
            }
            MdBlock::Table { headers, rows } => {
                container = container.child(render_table(headers, rows, theme));
            }
            MdBlock::Rule => {
                container = container.child(div().w_full().h(px(1.0)).my(px(4.0)).bg(theme.border_default));
            }
        }
    }

    container
}

pub fn render_code_block(id: SharedString, lang: &str, code: &str, theme: &SandTheme) -> Div {
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
