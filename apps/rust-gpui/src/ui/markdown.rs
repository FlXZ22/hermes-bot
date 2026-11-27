use crate::theme::components::{cursor_icon, FONT_MONO, FONT_SANS};
use crate::theme::tokens::SandTheme;
use gpui::prelude::FluentBuilder;
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
    link: bool,
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
        headers: Vec<RichText>,
        rows: Vec<Vec<RichText>>,
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

    let parser = Parser::new_ext(source, opts).into_offset_iter();
    let mut blocks = Vec::new();

    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buf = String::new();
    let mut verbatim_start = None;
    let mut code_end = 0;

    let mut in_heading: Option<HeadingLevel> = None;
    let mut in_blockquote = false;
    let mut in_list_item = false;
    let mut list_ordered_idx: Option<u64> = None;
    let mut task_checked: Option<bool> = None;

    let mut in_table = false;
    let mut table_headers = Vec::new();
    let mut table_rows = Vec::new();
    let mut current_row = Vec::new();

    let mut text_buf = String::new();
    let mut runs: Vec<RichRun> = Vec::new();
    let mut bold = false;
    let mut italic = false;
    let mut strikethrough = false;
    let mut link = false;

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

    for (event, range) in parser {
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
                // Top-level fenced content can be copied directly from the input.
                // Nested blocks use the parser's content with container markers removed.
                verbatim_start = if kind.is_fenced() && !in_blockquote && !in_list_item {
                    let fence = &source[range.start..];
                    fence.find(['\r', '\n']).map(|offset| {
                        let newline = &fence[offset..];
                        range.start + offset + if newline.starts_with("\r\n") { 2 } else { 1 }
                    })
                } else {
                    None
                };
                code_end = verbatim_start.unwrap_or(range.start);
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
                let code = verbatim_start.take().map_or_else(
                    || std::mem::take(&mut code_buf),
                    |start| source[start..code_end.max(start)].to_owned(),
                );
                blocks.push(MdBlock::CodeBlock { lang, code });
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
                    *n = n.saturating_add(1);
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
                text_buf.clear();
                runs.clear();
            }
            Event::End(TagEnd::TableCell) => {
                current_row.push(RichText {
                    text: std::mem::take(&mut text_buf),
                    runs: std::mem::take(&mut runs),
                });
            }
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::Strikethrough) => strikethrough = true,
            Event::End(TagEnd::Strikethrough) => strikethrough = false,
            Event::Start(Tag::Link { .. }) => link = true,
            Event::End(TagEnd::Link) => link = false,
            Event::Text(t) => {
                if in_code_block {
                    code_end = range.end;
                    if verbatim_start.is_none() {
                        if t.starts_with('\n')
                            && range.start.checked_sub(1).is_some_and(|start| source.as_bytes()[start] == b'\r')
                        {
                            code_buf.push('\r');
                        }
                        code_buf.push_str(&t);
                    }
                } else {
                    let start = text_buf.len();
                    text_buf.push_str(&t);
                    let end = text_buf.len();
                    if (bold || italic || strikethrough || link) && start < end {
                        runs.push(RichRun {
                            range: start..end,
                            bold,
                            italic,
                            code: false,
                            strikethrough,
                            link,
                        });
                    }
                }
            }
            Event::Code(c) => {
                let start = text_buf.len();
                text_buf.push_str(&c);
                let end = text_buf.len();
                if start < end {
                    runs.push(RichRun {
                        range: start..end,
                        bold,
                        italic,
                        code: true,
                        strikethrough,
                        link,
                    });
                }
            }
            event @ (Event::SoftBreak | Event::HardBreak) => {
                if in_code_block {
                    code_end = range.end;
                    code_buf.push('\n');
                } else if in_table || matches!(event, Event::SoftBreak) {
                    text_buf.push(' ');
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
            if run.link {
                style.color = Some(theme.accent_text);
                style.underline = Some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(theme.accent_text),
                    wavy: false,
                });
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
    render_markdown_body(id_prefix, source, theme, false)
}

pub fn render_thinking_markdown(id_prefix: &str, source: &str, theme: &SandTheme) -> Div {
    render_markdown_body(id_prefix, source, theme, true)
}

fn markdown_block() -> Div {
    div().w_full().min_w_0().overflow_hidden()
}

fn render_markdown_body(id_prefix: &str, source: &str, theme: &SandTheme, muted: bool) -> Div {
    let blocks = parse_markdown_cached(source);
    let ink = if muted { theme.text_secondary } else { theme.text_primary };

    let mut container = markdown_block()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .font_family(FONT_SANS)
        .text_size(px(if muted { 12.5 } else { 14.5 }))
        .line_height(px(if muted { 19.0 } else { 23.0 }))
        .text_color(ink);

    for (index, block) in blocks.iter().enumerate() {
        match block {
            MdBlock::Heading { level, content } => {
                let (sz, lh) = match level {
                    1 => (19.0, 26.0),
                    2 => (16.5, 24.0),
                    3 => (15.0, 22.0),
                    _ => (14.5, 21.0),
                };
                container = container.child(
                    markdown_block()
                        .when(index > 0, |d| d.pt(px(4.0)))
                        .text_size(px(if muted { sz - 2.0 } else { sz }))
                        .line_height(px(if muted { lh - 3.0 } else { lh }))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(styled_rich_text(content, theme)),
                );
            }
            MdBlock::Paragraph(content) => {
                container = container.child(markdown_block().child(styled_rich_text(content, theme)));
            }
            MdBlock::Blockquote(content) => {
                container = container.child(
                    markdown_block()
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
                    markdown_block()
                        .pl(px(6.0))
                        .flex()
                        .items_start()
                        .gap(px(8.0))
                        .child(
                            div()
                                .min_w_0()
                                .text_color(theme.text_tertiary)
                                .flex_shrink_0()
                                .child(bullet),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .child(styled_rich_text(content, theme)),
                        ),
                );
            }
            MdBlock::CodeBlock { lang, code } => {
                let cid = SharedString::from(format!("{id_prefix}-code-{index}"));
                container = container.child(render_code_block(cid, lang, code, theme));
            }
            MdBlock::Table { headers, rows } => {
                let id = SharedString::from(format!("{id_prefix}-table-scroll-{index}"));
                container = container.child(render_table(id, headers, rows, theme));
            }
            MdBlock::Rule => {
                container = container.child(markdown_block().h(px(1.0)).my(px(4.0)).bg(theme.border_default));
            }
        }
    }

    container
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CodeLanguage {
    Rust,
    JavaScript,
    Jsx,
    Python,
    Json,
    Shell,
    Yaml,
    Toml,
    Sql,
    Diff,
    Html,
    Css,
}

impl CodeLanguage {
    fn from_hint(hint: &str) -> Option<Self> {
        let hint = hint.split_ascii_whitespace().next()?;
        [
            (Self::Rust, &["rust", "rs"][..]),
            (
                Self::JavaScript,
                &[
                    "typescript",
                    "ts",
                    "javascript",
                    "js",
                    "mjs",
                    "cjs",
                    "mts",
                    "cts",
                ],
            ),
            (Self::Jsx, &["tsx", "jsx"]),
            (Self::Python, &["python", "python3", "py", "py3"]),
            (Self::Json, &["json", "jsonc"]),
            (
                Self::Shell,
                &[
                    "bash",
                    "shell",
                    "sh",
                    "zsh",
                    "fish",
                    "shellscript",
                    "console",
                ],
            ),
            (Self::Yaml, &["yaml", "yml"]),
            (Self::Toml, &["toml"]),
            (
                Self::Sql,
                &["sql", "postgresql", "postgres", "mysql", "sqlite"],
            ),
            (Self::Diff, &["diff", "patch"]),
            (Self::Html, &["html", "xml", "svg"]),
            (Self::Css, &["css", "scss", "sass", "less"]),
        ]
        .into_iter()
        .find_map(|(language, aliases)| {
            aliases
                .iter()
                .any(|alias| hint.eq_ignore_ascii_case(alias))
                .then_some(language)
        })
    }

    fn keyword(self, word: &str) -> bool {
        match self {
            Self::Rust => matches!(
                word,
                "as" | "async"
                    | "await"
                    | "break"
                    | "const"
                    | "continue"
                    | "crate"
                    | "dyn"
                    | "else"
                    | "enum"
                    | "extern"
                    | "false"
                    | "fn"
                    | "for"
                    | "if"
                    | "impl"
                    | "in"
                    | "let"
                    | "loop"
                    | "match"
                    | "mod"
                    | "move"
                    | "mut"
                    | "pub"
                    | "ref"
                    | "return"
                    | "self"
                    | "Self"
                    | "static"
                    | "struct"
                    | "super"
                    | "trait"
                    | "true"
                    | "type"
                    | "unsafe"
                    | "use"
                    | "where"
                    | "while"
                    | "yield"
            ),
            Self::JavaScript | Self::Jsx => matches!(
                word,
                "abstract"
                    | "any"
                    | "as"
                    | "async"
                    | "await"
                    | "bigint"
                    | "boolean"
                    | "break"
                    | "case"
                    | "catch"
                    | "class"
                    | "const"
                    | "continue"
                    | "declare"
                    | "default"
                    | "delete"
                    | "do"
                    | "else"
                    | "enum"
                    | "export"
                    | "extends"
                    | "false"
                    | "finally"
                    | "for"
                    | "from"
                    | "function"
                    | "if"
                    | "implements"
                    | "import"
                    | "in"
                    | "infer"
                    | "instanceof"
                    | "interface"
                    | "keyof"
                    | "let"
                    | "namespace"
                    | "never"
                    | "new"
                    | "null"
                    | "number"
                    | "of"
                    | "private"
                    | "protected"
                    | "public"
                    | "readonly"
                    | "return"
                    | "satisfies"
                    | "static"
                    | "string"
                    | "super"
                    | "switch"
                    | "symbol"
                    | "this"
                    | "throw"
                    | "true"
                    | "try"
                    | "type"
                    | "typeof"
                    | "undefined"
                    | "unknown"
                    | "var"
                    | "void"
                    | "while"
                    | "with"
                    | "yield"
            ),
            Self::Python => matches!(
                word,
                "and"
                    | "as"
                    | "assert"
                    | "async"
                    | "await"
                    | "break"
                    | "case"
                    | "class"
                    | "continue"
                    | "def"
                    | "del"
                    | "elif"
                    | "else"
                    | "except"
                    | "False"
                    | "finally"
                    | "for"
                    | "from"
                    | "global"
                    | "if"
                    | "import"
                    | "in"
                    | "is"
                    | "lambda"
                    | "match"
                    | "None"
                    | "nonlocal"
                    | "not"
                    | "or"
                    | "pass"
                    | "raise"
                    | "return"
                    | "True"
                    | "try"
                    | "while"
                    | "with"
                    | "yield"
            ),
            Self::Json | Self::Toml => matches!(word, "true" | "false" | "null" | "inf" | "nan"),
            Self::Yaml => ["true", "false", "null", "yes", "no", "on", "off"]
                .iter()
                .any(|keyword| word.eq_ignore_ascii_case(keyword)),
            Self::Shell => matches!(
                word,
                "case"
                    | "do"
                    | "done"
                    | "elif"
                    | "else"
                    | "esac"
                    | "export"
                    | "fi"
                    | "for"
                    | "function"
                    | "if"
                    | "in"
                    | "local"
                    | "readonly"
                    | "return"
                    | "select"
                    | "set"
                    | "then"
                    | "trap"
                    | "until"
                    | "while"
            ),
            Self::Sql => [
                "ADD",
                "ALTER",
                "AND",
                "AS",
                "ASC",
                "BEGIN",
                "BETWEEN",
                "BY",
                "CASE",
                "CHECK",
                "COMMIT",
                "CREATE",
                "CROSS",
                "DEFAULT",
                "DELETE",
                "DESC",
                "DISTINCT",
                "DROP",
                "ELSE",
                "END",
                "EXISTS",
                "FALSE",
                "FETCH",
                "FOREIGN",
                "FROM",
                "GROUP",
                "HAVING",
                "IN",
                "INDEX",
                "INNER",
                "INSERT",
                "INTO",
                "IS",
                "JOIN",
                "LEFT",
                "LIKE",
                "LIMIT",
                "NOT",
                "NULL",
                "OFFSET",
                "ON",
                "OR",
                "ORDER",
                "OUTER",
                "PRIMARY",
                "REFERENCES",
                "RETURNING",
                "RIGHT",
                "ROLLBACK",
                "SELECT",
                "SET",
                "TABLE",
                "THEN",
                "TRUE",
                "UNION",
                "UNIQUE",
                "UPDATE",
                "VALUES",
                "VIEW",
                "WHEN",
                "WHERE",
                "WITH",
            ]
            .iter()
            .any(|keyword| word.eq_ignore_ascii_case(keyword)),
            Self::Css => matches!(word, "important" | "inherit" | "initial" | "unset"),
            Self::Diff | Self::Html => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeToken {
    Keyword,
    String,
    Number,
    Comment,
    Name,
    Section,
    Added,
    Removed,
}

impl CodeToken {
    fn highlight(self, theme: &SandTheme) -> HighlightStyle {
        let color = match self {
            Self::Keyword => theme.accent_text,
            Self::String | Self::Added => theme.success_text,
            Self::Number => theme.warn_text,
            Self::Comment => theme.text_secondary,
            Self::Name => match theme.mode {
                crate::theme::tokens::ThemeMode::Dark => rgb(0xc5a7f0).into(),
                crate::theme::tokens::ThemeMode::Light => rgb(0x744a9e).into(),
            },
            Self::Section => theme.accent_text,
            Self::Removed => theme.danger_text,
        };
        HighlightStyle {
            color: Some(color),
            ..Default::default()
        }
    }
}

/// Scans borrowed source and yields sorted, non-overlapping byte ranges. ASCII
/// delimiters cannot split a UTF-8 scalar. Identifier and fallback steps use char boundaries.
struct CodeTokens<'a> {
    code: &'a str,
    language: Option<CodeLanguage>,
    offset: usize,
    in_tag: bool,
    at_line_start: bool,
}

fn code_tokens<'a>(lang: Option<&str>, code: &'a str) -> CodeTokens<'a> {
    CodeTokens {
        code,
        language: lang.and_then(CodeLanguage::from_hint),
        offset: 0,
        in_tag: false,
        at_line_start: true,
    }
}

fn line_bytes(line: &str) -> usize {
    line.find(['\r', '\n']).unwrap_or(line.len())
}

fn word_bytes(text: &str, allows_hyphen: bool) -> usize {
    text.char_indices()
        .find(|&(_, ch)| {
            !(ch.is_alphanumeric() || ch == '_' || ch == '$' || (allows_hyphen && ch == '-'))
        })
        .map_or(text.len(), |(offset, _)| offset)
}

fn begins_number(text: &str) -> bool {
    text.as_bytes().first().is_some_and(u8::is_ascii_digit)
        || text
            .strip_prefix('-')
            .and_then(|rest| rest.as_bytes().first())
            .is_some_and(u8::is_ascii_digit)
}

fn number_bytes(text: &str) -> usize {
    let bytes = text.as_bytes();
    let sign = usize::from(bytes[0] == b'-');
    let body = &bytes[sign..];
    if body.starts_with(b"0x")
        || body.starts_with(b"0X")
        || body.starts_with(b"0b")
        || body.starts_with(b"0o")
    {
        return sign
            + 2
            + body[2..]
                .iter()
                .take_while(|byte| byte.is_ascii_hexdigit() || **byte == b'_')
                .count();
    }
    let digits = |start: usize| {
        bytes[start..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit() || **byte == b'_')
            .count()
    };
    let mut end = sign + digits(sign);
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 1 + digits(end + 1);
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let start = end + 1 + usize::from(matches!(bytes.get(end + 1), Some(b'-' | b'+')));
        let length = digits(start);
        if length > 0 {
            end = start + length;
        }
    }
    end
}

fn string_bytes(
    text: &str,
    delimiter: &str,
    escape: bool,
    multiline: bool,
    doubled: bool,
) -> usize {
    let mut rest = &text[delimiter.len()..];
    let mut consumed = delimiter.len();
    while let Some(ch) = rest.chars().next() {
        if rest.starts_with(delimiter) {
            if doubled && rest[delimiter.len()..].starts_with(delimiter) {
                let len = delimiter.len() * 2;
                consumed += len;
                rest = &rest[len..];
                continue;
            }
            return consumed + delimiter.len();
        }
        if !multiline && matches!(ch, '\r' | '\n') {
            break;
        }
        let len = if escape && ch == '\\' {
            1 + rest[1..].chars().next().map_or(0, char::len_utf8)
        } else {
            ch.len_utf8()
        };
        consumed += len;
        rest = &rest[len..];
    }
    consumed
}

fn rust_raw_string_bytes(text: &str) -> Option<usize> {
    let opening = text.strip_prefix("br").or_else(|| text.strip_prefix('r'))?;
    let hashes = opening.bytes().take_while(|&byte| byte == b'#').count();
    let content = opening[hashes..].strip_prefix('"')?;
    let start = text.len() - content.len();
    let end = content.match_indices('"').find_map(|(quote, _)| {
        let suffix = content.as_bytes().get(quote + 1..quote + 1 + hashes)?;
        suffix
            .iter()
            .all(|&byte| byte == b'#')
            .then_some(start + quote + 1 + hashes)
    });
    Some(end.unwrap_or(text.len()))
}

fn block_comment_bytes(text: &str, nested: bool) -> usize {
    let mut depth = 1;
    let mut offset = 2;
    let bytes = text.as_bytes();
    while offset + 1 < bytes.len() {
        match &bytes[offset..offset + 2] {
            b"/*" if nested => {
                depth += 1;
                offset += 2;
            }
            b"*/" => {
                depth -= 1;
                offset += 2;
                if depth == 0 {
                    return offset;
                }
            }
            _ => offset += 1,
        }
    }
    text.len()
}

impl Iterator for CodeTokens<'_> {
    type Item = (Range<usize>, CodeToken);

    fn next(&mut self) -> Option<Self::Item> {
        let language = self.language?;
        while self.offset < self.code.len() {
            let start = self.offset;
            let text = &self.code[start..];
            if language == CodeLanguage::Diff {
                let end = start + line_bytes(text);
                self.offset = text
                    .find('\n')
                    .map_or(self.code.len(), |index| start + index + 1);
                let kind = if [
                    "diff --git ",
                    "diff --cc ",
                    "diff --combined ",
                    "index ",
                    "@@",
                    "+++",
                    "---",
                ]
                .iter()
                .any(|prefix| text.starts_with(prefix))
                {
                    Some(CodeToken::Section)
                } else if text.starts_with('+') {
                    Some(CodeToken::Added)
                } else if text.starts_with('-') {
                    Some(CodeToken::Removed)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    return Some((start..end, kind));
                }
                continue;
            }

            let ch = text.chars().next()?;
            let previous = self.code[..start].chars().next_back();
            let (len, token) = self.token(language, text, ch, previous);
            self.offset += len.max(ch.len_utf8());
            if matches!(ch, '\r' | '\n') {
                self.at_line_start = true;
            } else if !ch.is_whitespace() {
                self.at_line_start = false;
            }
            if let Some(token) = token {
                return Some((start..self.offset, token));
            }
        }
        None
    }
}

impl CodeTokens<'_> {
    fn token(
        &mut self,
        language: CodeLanguage,
        text: &str,
        ch: char,
        previous: Option<char>,
    ) -> (usize, Option<CodeToken>) {
        use CodeLanguage::*;
        use CodeToken::{Comment, Keyword, Name, Number, Section, String};

        if (matches!(language, Rust | JavaScript | Jsx | Json) && text.starts_with("//"))
            || (language == Sql && text.starts_with("--"))
            || (ch == '#'
                && (matches!(language, Python | Toml)
                    || (matches!(language, Shell | Yaml)
                        && previous.is_none_or(|ch| ch.is_whitespace() || ";&|()".contains(ch)))))
        {
            return (line_bytes(text), Some(Comment));
        }
        if matches!(language, Rust | JavaScript | Jsx | Json | Sql | Css) && text.starts_with("/*")
        {
            return (block_comment_bytes(text, language == Rust), Some(Comment));
        }
        if matches!(language, Html | Jsx) && text.starts_with("<!--") {
            return (
                text[4..]
                    .find("-->")
                    .map_or(text.len(), |offset| 7 + offset),
                Some(Comment),
            );
        }
        if language == Rust {
            if let Some(len) = rust_raw_string_bytes(text) {
                return (len, Some(String));
            }
            if ch == '\'' {
                let after_quote = &text[1..];
                if after_quote.starts_with('\\') {
                    return (string_bytes(text, "'", true, false, false), Some(String));
                }
                if let Some(value) = after_quote.chars().next() {
                    if after_quote[value.len_utf8()..].starts_with('\'') {
                        return (value.len_utf8() + 2, Some(String));
                    }
                }
                // Rust lifetimes such as 'a are identifiers.
                return (1, None);
            }
        }
        if language == Toml && ch == '[' && self.at_line_start {
            return (line_bytes(text), Some(Section));
        }
        if matches!(language, Html | Jsx) && ch == '<' {
            let name = text[1..].strip_prefix('/').unwrap_or(&text[1..]);
            if name.starts_with(|ch: char| ch.is_alphabetic() || ch == '!') {
                self.in_tag = true;
                return (
                    text.len() - name.len() + word_bytes(name, true),
                    Some(Section),
                );
            }
        }
        if matches!(language, Html | Jsx) && ch == '>' && self.in_tag {
            self.in_tag = false;
            return (1, Some(Section));
        }

        let quoted = if matches!(language, Html | Jsx) && self.in_tag {
            matches!(ch, '"' | '\'')
        } else {
            match language {
                Rust | Json => ch == '"',
                JavaScript | Jsx | Shell => matches!(ch, '"' | '\'' | '`'),
                Python | Yaml | Toml | Sql | Css => matches!(ch, '"' | '\''),
                Html | Diff => false,
            }
        };
        if quoted {
            let delimiter = if matches!(language, Python | Toml)
                && (text.starts_with("\"\"\"") || text.starts_with("'''"))
            {
                &text[..3]
            } else {
                &text[..1]
            };
            let multiline = delimiter.len() == 3
                || ch == '`'
                || matches!(language, Rust | Shell | Sql | Yaml | Html);
            let escapes = !(language == Sql
                || language == Html
                || (ch == '\'' && matches!(language, Shell | Toml | Yaml)));
            let doubled = language == Sql || (language == Yaml && ch == '\'');
            let len = string_bytes(text, delimiter, escapes, multiline, doubled);
            let token = if language == Json && text[len..].trim_start().starts_with(':') {
                Name
            } else {
                String
            };
            return (len, Some(token));
        }
        if language == Html && !self.in_tag {
            return (
                text.find('<')
                    .filter(|&offset| offset > 0)
                    .unwrap_or(ch.len_utf8()),
                None,
            );
        }
        if begins_number(text) {
            return (number_bytes(text), Some(Number));
        }
        if ch.is_alphabetic() || matches!(ch, '_' | '$') {
            let len = word_bytes(text, matches!(language, Html | Css | Yaml | Toml));
            let word = &text[..len];
            let following = text[len..].trim_start();
            let kind = if language.keyword(word) {
                Some(Keyword)
            } else if ((matches!(language, Yaml | Css) && following.starts_with(':'))
                || (language == Toml && following.starts_with('='))
                || self.in_tag
                || (language == Shell && word.starts_with('$')))
                || (language == Rust
                    && matches!(
                        word,
                        "bool"
                            | "char"
                            | "str"
                            | "usize"
                            | "isize"
                            | "u8"
                            | "u16"
                            | "u32"
                            | "u64"
                            | "u128"
                            | "i8"
                            | "i16"
                            | "i32"
                            | "i64"
                            | "i128"
                            | "f32"
                            | "f64"
                    ))
            {
                Some(Name)
            } else {
                None
            };
            return (len, kind);
        }
        (ch.len_utf8(), None)
    }
}

pub fn render_code_block(id: SharedString, lang: &str, code: &str, theme: &SandTheme) -> Div {
    render_highlighted_code_block(id, Some(lang), code, false, false, theme)
}

/// Render code with optional line numbers and soft wrapping. Highlighting uses
/// the supplied language only, so unlabelled program records keep plain text.
pub fn render_highlighted_code_block(
    id: SharedString,
    lang: Option<&str>,
    code: &str,
    line_numbers: bool,
    wrap_lines: bool,
    theme: &SandTheme,
) -> Div {
    let source = SharedString::from(code);
    let copy_source = source.clone();
    let scroll_id = SharedString::from(format!("{id}-scroll"));
    let mut body = div()
        .px(px(12.0))
        .py(px(10.0))
        .flex()
        .flex_col()
        .font_family(FONT_MONO)
        .text_size(px(12.5))
        .line_height(px(19.0))
        .text_color(theme.text_primary)
        .when(wrap_lines, |d| d.flex_1().min_w_0().whitespace_normal())
        .when(!wrap_lines, |d| d.flex_shrink_0().whitespace_nowrap());

    if line_numbers {
        let line_count = code.split_inclusive('\n').count().max(1);
        let gutter_width = px(8.0 * line_count.ilog10().saturating_add(1) as f32 + 20.0);
        let mut spans = code_tokens(lang, code).peekable();
        let mut offset = 0;
        for (index, raw_line) in code
            .split_inclusive('\n')
            .chain(code.is_empty().then_some(""))
            .enumerate()
        {
            let line = raw_line
                .strip_suffix('\n')
                .map_or(raw_line, |line| line.strip_suffix('\r').unwrap_or(line));
            let end = offset + line.len();
            let mut consumed_through_eol = false;
            let highlights = std::iter::from_fn(|| {
                if consumed_through_eol {
                    return None;
                }
                while let Some((range, token)) = spans.peek() {
                    if range.start >= end {
                        return None;
                    }
                    let from = range.start.max(offset);
                    let to = range.end.min(end);
                    let style = token.highlight(theme);
                    if range.end > end {
                        consumed_through_eol = true;
                    } else {
                        spans.next();
                    }
                    if from < to {
                        return Some((from - offset..to - offset, style));
                    }
                    if consumed_through_eol {
                        return None;
                    }
                }
                None
            });
            let line = render_code_text(SharedString::from(line), highlights);
            offset += raw_line.len();
            body = body.child(
                div()
                    .flex()
                    .items_start()
                    .min_h(px(19.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(gutter_width)
                            .pr(px(10.0))
                            .text_right()
                            .text_color(theme.text_tertiary)
                            .child((index + 1).to_string()),
                    )
                    .child(if wrap_lines {
                        div().flex_1().min_w_0().child(line)
                    } else {
                        div().flex_shrink_0().child(line)
                    }),
            );
        }
    } else {
        body = body.child(render_code_text(
            source,
            code_tokens(lang, code).map(|(range, token)| (range, token.highlight(theme))),
        ));
    }

    markdown_block()
        .my(px(4.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(theme.border_default)
        .bg(theme.bg_chrome)
        .flex()
        .flex_col()
        .child(
            markdown_block()
                .px(px(10.0))
                .py(px(5.0))
                .border_b_1()
                .border_color(theme.border_default)
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(11.5))
                .text_color(theme.text_tertiary)
                .child(div().min_w_0().truncate().child(SharedString::from(
                    lang.filter(|hint| !hint.is_empty()).unwrap_or("Code"),
                )))
                .child(
                    div()
                        .id(id)
                        .flex_shrink_0()
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
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                copy_source.to_string(),
                            ));
                        }),
                ),
        )
        .child(
            div()
                .id(scroll_id)
                .w_full()
                .min_w_0()
                .when(!wrap_lines, |d| d.overflow_x_scroll())
                .flex()
                .child(body),
        )
}

fn render_code_text(
    text: SharedString,
    highlights: impl Iterator<Item = (Range<usize>, HighlightStyle)>,
) -> StyledText {
    let mut highlights = highlights.peekable();
    let text = StyledText::new(text);
    if highlights.peek().is_some() {
        text.with_highlights(highlights)
    } else {
        text
    }
}

fn render_table(id: SharedString, headers: &[RichText], rows: &[Vec<RichText>], theme: &SandTheme) -> Div {
    // Every row uses the same column count and minimum width. The grid, not the
    // message bubble, grows when those columns no longer fit in the viewport.
    const MIN_CELL_WIDTH: f32 = 112.0;
    let columns = headers.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
    let render_row = |cells: &[RichText], header: bool| {
        div()
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(theme.border_hairline)
            .when(header, |d| d.bg(theme.bg_card).font_weight(FontWeight::SEMIBOLD))
            .flex()
            .children((0..columns).map(|index| {
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .px(px(10.0))
                    .py(px(7.0))
                    .text_color(if header { theme.text_primary } else { theme.text_secondary })
                    .when_some(cells.get(index), |d, c| d.child(styled_rich_text(c, theme)))
            }))
    };

    markdown_block()
        .my(px(4.0))
        .rounded(px(9.0))
        .border_1()
        .border_color(theme.border_default)
        .text_size(px(13.0))
        .line_height(px(19.0))
        .child(
            div()
                .id(id)
                .w_full()
                .min_w_0()
                .overflow_x_scroll()
                .child(
                    div()
                        .w_full()
                        .min_w(px(MIN_CELL_WIDTH * columns as f32))
                        .flex()
                        .flex_col()
                        .when(!headers.is_empty(), |d| d.child(render_row(headers, true)))
                        .children(rows.iter().map(|row| render_row(row, false))),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::{parse_markdown_ast, parse_markdown_cached, MdBlock};
    use std::sync::Arc;

    use super::{code_tokens, CodeToken};

    fn token_text<'a>(language: Option<&str>, source: &'a str) -> Vec<(&'a str, CodeToken)> {
        let mut previous_end = 0;
        code_tokens(language, source)
            .map(|(range, kind)| {
                assert!(range.start >= previous_end);
                assert!(range.start < range.end);
                assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
                previous_end = range.end;
                (&source[range], kind)
            })
            .collect()
    }

    #[test]
    fn rust_tokens_cover_raw_strings_nested_comments_and_lifetimes() {
        use CodeToken::*;
        let source = r##"pub fn café<'a>(x: &'a str) { let msg = r#"🦀"#; let n = 20; /* outer /* inner */ done */ }"##;
        assert_eq!(
            token_text(Some("rust"), source),
            [
                ("pub", Keyword),
                ("fn", Keyword),
                ("str", Name),
                ("let", Keyword),
                (r##"r#"🦀"#"##, String),
                ("let", Keyword),
                ("20", Number),
                ("/* outer /* inner */ done */", Comment),
            ]
        );
    }

    #[test]
    fn typescript_and_tsx_tokens_do_not_split_non_ascii_text() {
        use CodeToken::*;
        let source = "const café: string = `🙂`; // école\nreturn <Badge label=\"猫\" />";
        assert_eq!(
            token_text(Some("TSX"), source),
            [
                ("const", Keyword),
                ("string", Keyword),
                ("`🙂`", String),
                ("// école", Comment),
                ("return", Keyword),
                ("<Badge", Section),
                ("label", Name),
                ("\"猫\"", String),
                (">", Section),
            ]
        );
        assert_eq!(
            token_text(Some("typescript"), "let n = -3.2e+4"),
            [("let", Keyword), ("-3.2e+4", Number),]
        );
    }

    #[test]
    fn python_and_json_tokenize_unicode_strings_numbers_and_literals() {
        use CodeToken::*;
        let source = "def f():\n    return \"\"\"café\n猫\"\"\" # note\n";
        assert_eq!(
            token_text(Some("python"), source),
            [
                ("def", Keyword),
                ("return", Keyword),
                ("\"\"\"café\n猫\"\"\"", String),
                ("# note", Comment),
            ]
        );
        let source = r#"{"é": ["\u2603", -1.2e-3, true, null]}"#;
        assert_eq!(
            token_text(Some("json"), source),
            [
                (r#""é""#, Name),
                (r#""\u2603""#, String),
                ("-1.2e-3", Number),
                ("true", Keyword),
                ("null", Keyword),
            ]
        );
    }

    #[test]
    fn diff_marks_headers_and_whole_changed_lines() {
        use CodeToken::*;
        let source =
            "diff --git a/café b/café\n--- a/café\n+++ b/café\n@@ -1 +1 @@\n-旧\n+新\n unchanged";
        assert_eq!(
            token_text(Some("diff"), source),
            [
                ("diff --git a/café b/café", Section),
                ("--- a/café", Section),
                ("+++ b/café", Section),
                ("@@ -1 +1 @@", Section),
                ("-旧", Removed),
                ("+新", Added),
            ]
        );
    }

    #[test]
    fn streaming_prefixes_keep_valid_ranges_and_mark_incomplete_strings_and_comments() {
        use CodeToken::*;
        for (language, input, fragment, kind) in [
            ("rust", "fn f() { /* note 🦀", "/* note 🦀", Comment),
            ("rust", r##"let x = r#"🦀"##, r##"r#"🦀"##, String),
            ("ts", "const s = \"猫", "\"猫", String),
            ("python", "return '''é\n猫", "'''é\n猫", String),
            ("html", "<!-- élève", "<!-- élève", Comment),
        ] {
            for (index, _) in input.char_indices() {
                let _ = token_text(Some(language), &input[..index]);
            }
            assert_eq!(
                token_text(Some(language), input).last(),
                Some(&(fragment, kind))
            );
        }
    }

    #[test]
    fn shell_config_sql_and_web_languages_use_their_own_tokens() {
        use CodeToken::*;
        for (language, code, expected) in [
            ("bash", r#"if echo "$HOME"; then # note"#, ("# note", Comment)),
            ("yaml", "color: blue # notes", ("color", Name)),
            ("toml", "[server.settings]\nport = 9119", ("[server.settings]", Section)),
            ("sql", "select 'it''s fine'", ("select", Keyword)),
            ("html", r#"<div class="猫">こんにちは</div>"#, ("<div", Section)),
            ("css", "a { margin: 4px; }", ("margin", Name)),
        ] {
            assert!(token_text(Some(language), code).contains(&expected), "{language}");
        }
    }

    #[test]
    fn quoted_fenced_code_omits_markdown_prefixes_but_keeps_line_endings() {
        let blocks = parse_markdown_ast("> ```sh\r\n> echo hi  \r\n> \r\n> ```\r\n");
        let [MdBlock::CodeBlock { code, .. }] = blocks.as_slice() else {
            panic!("expected a quoted code block: {blocks:?}");
        };
        assert_eq!(code, "echo hi  \r\n\r\n");
    }

    #[test]
    fn missing_and_unknown_languages_leave_programs_unhighlighted() {
        let input = "const x: string = \"hello\"; // JavaScript";
        assert!(token_text(None, input).is_empty());
        assert!(token_text(Some("unknown"), input).is_empty());
        assert!(!token_text(Some("typescript"), input).is_empty());
    }

    #[test]
    fn fenced_copy_keeps_original_indentation_whitespace_and_line_endings() {
        let code = "    let value = \"café\";  \r\n\t// 猫\r\n\r\n";
        for suffix in ["", "  ```\r\n"] {
            let source = format!("  ```rust\r\n{code}{suffix}");
            let blocks = parse_markdown_ast(&source);
            let [MdBlock::CodeBlock { code: parsed, .. }] = blocks.as_slice() else {
                panic!("expected a code block: {blocks:?}");
            };
            assert_eq!(parsed, code);
        }

        let source = "```python\n\tprint(\"猫\")  ";
        let blocks = parse_markdown_ast(source);
        let [MdBlock::CodeBlock { code, .. }] = blocks.as_slice() else {
            panic!("expected an unfinished code block: {blocks:?}");
        };
        assert_eq!(code, "\tprint(\"猫\")  ");
    }

    #[test]
    fn rich_text_links_and_highlight_ranges_are_utf8_safe() {
        let blocks = parse_markdown_ast(
            "# **日本語**\n\nSee [*café* and `🔧`](https://example.com)  \nnext\nline",
        );
        let [MdBlock::Heading {
            content: heading, ..
        }, MdBlock::Paragraph(paragraph)] = blocks.as_slice()
        else {
            panic!("expected two rich blocks: {blocks:?}");
        };
        assert_eq!(&heading.text[heading.runs[0].range.clone()], "日本語");
        assert_eq!(paragraph.text, "See café and 🔧\nnext line");
        assert!(paragraph.runs.iter().any(|run| {
            run.link && run.italic && &paragraph.text[run.range.clone()] == "café"
        }));
        assert!(paragraph
            .runs
            .iter()
            .any(|run| { run.link && run.code && &paragraph.text[run.range.clone()] == "🔧" }));
    }

    #[test]
    fn wide_table_preserves_rich_cells_and_missing_values() {
        let headings = ["A", "B", "C", "D", "E", "F", "G", "H"];
        let url = format!("https://example.com/{}", "世界".repeat(100));
        let source = format!(
            "{}\n{}\n<{url}> | `值`",
            headings.join(" | "),
            ["---"; 8].join(" | ")
        );
        let blocks = parse_markdown_ast(&source);
        let [MdBlock::Table { headers, rows }] = blocks.as_slice() else {
            panic!("expected a table: {blocks:?}");
        };
        assert_eq!(headers.len(), headings.len());
        assert_eq!(rows[0].len(), headings.len());
        assert_eq!(rows[0][0].text, url);
        assert!(rows[0][0].runs[0].link);
        assert!(rows[0][1].runs[0].code);
        assert!(rows[0][7].text.is_empty());
    }

    #[test]
    fn unfinished_fence_keeps_long_lines_indentation_and_blank_lines() {
        let line = format!("    {}  ", "🦀".repeat(512));
        let source = format!("```rust\n{line}\n\n");
        let doc = parse_markdown_cached(&source);
        let [MdBlock::CodeBlock { lang, code }] = doc.as_slice() else {
            panic!("expected a code block: {doc:?}");
        };
        assert_eq!(lang, "rust");
        assert_eq!(code, &format!("{line}\n\n"));
        assert!(Arc::ptr_eq(&doc, &parse_markdown_cached(&source)));
    }

    #[test]
    fn blockquotes_and_task_items_keep_text_at_block_boundaries() {
        let blocks = parse_markdown_ast("> first\n>\n> second\n\n- [x] done\n- [ ] pending");
        let [MdBlock::Blockquote(first), MdBlock::Blockquote(second), MdBlock::ListItem {
            task_checked: Some(true),
            content: done,
            ..
        }, MdBlock::ListItem {
            task_checked: Some(false),
            content: pending,
            ..
        }] = blocks.as_slice()
        else {
            panic!("expected quotes and task items: {blocks:?}");
        };
        assert_eq!(
            (
                &first.text[..],
                &second.text[..],
                &done.text[..],
                &pending.text[..]
            ),
            ("first", "second", "done", "pending")
        );
    }
}
