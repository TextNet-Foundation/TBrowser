use std::collections::HashMap;

use async_trait::async_trait;
use ratatui::style::Color;

use crate::core::document::{
    Alignment, ColorBorder, Document, FieldKind, FormField, FormItem, InfoKind, LinkInfo,
    ListMarker, MetaValue, NodeData, RadioOption, SelectOption, Span, SpanStyle,
};
use crate::core::parser::base::Parser;

pub struct TmkupParser;

#[async_trait]
impl Parser for TmkupParser {
    fn content_type(&self) -> &str {
        "text/tmkup"
    }

    async fn parse(&self, raw: &str, content_type: &str) -> Document {
        let mut doc = Document::new(content_type);

        let (meta, variables, body) = extract_meta(raw);
        for (k, v) in &meta {
            doc.meta.insert(k.clone(), v.clone());
        }
        doc.meta
            .insert("variables".to_string(), MetaValue::Map(variables.clone()));

        // meta.title → doc.title (если задан)
        if let Some(MetaValue::Str(t)) = doc.meta.get("title") {
            doc.title = t.clone();
        }

        let lines = strip_comments(&body);
        let lines = trim_empty(&lines);
        parse_lines(&lines, &mut doc, &variables);

        if doc.title.is_empty() {
            let mut found: Option<String> = None;
            for (_, node) in doc.iter() {
                if let NodeData::Heading { level: 1, spans } = node {
                    found = Some(spans.iter().map(|s| s.text.as_str()).collect());
                    break;
                }
            }
            doc.title = found.unwrap_or_else(|| "TMkUp Document".to_string());
        }

        doc
    }
}

// ============================================================
// Meta
// ============================================================

fn extract_meta(raw: &str) -> (HashMap<String, MetaValue>, HashMap<String, String>, String) {
    let mut meta = HashMap::new();
    let mut variables = HashMap::new();

    let mut body = raw.to_string();

    let trimmed = raw.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---tmk-meta") {
        if let Some(end) = rest.find("---tmk-meta") {
            let meta_block = &rest[..end];
            let after = &rest[end + "---tmk-meta".len()..];
            body = after.trim_start_matches('\n').to_string();

            let mut in_vars = false;
            let mut var_indent = 0usize;
            for line in meta_block.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                let indent = line.len() - line.trim_start().len();

                if in_vars {
                    if indent > var_indent {
                        if let Some((k, v)) = line.trim().split_once(':') {
                            variables.insert(
                                k.trim().to_string(),
                                v.trim().trim_matches('"').to_string(),
                            );
                            continue;
                        }
                    } else {
                        in_vars = false;
                    }
                }

                if let Some((k, v)) = line.split_once(':') {
                    let k = k.trim().to_string();
                    let v = v.trim();
                    if v.is_empty() && k == "variables" {
                        in_vars = true;
                        var_indent = indent;
                    } else {
                        meta.insert(k, MetaValue::Str(v.trim_matches('"').to_string()));
                    }
                }
            }
        }
    }

    (meta, variables, body)
}

// ============================================================
// Комментарии и пустые строки
// ============================================================

fn strip_comments(raw: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_multiline = false;
    let mut in_fence = false;

    for line in raw.lines() {
        // Внутри ```-блока содержимое сырое: комментарии там не распознаются
        if in_fence {
            if is_fence_line(line) {
                in_fence = false;
            }
            result.push(line.to_string());
            continue;
        }
        if is_fence_line(line) {
            in_fence = true;
            result.push(line.to_string());
            continue;
        }

        if in_multiline {
            if let Some(end) = line.find("*/") {
                let (text, opened) = strip_line_comments(&line[end + 2..]);
                in_multiline = opened;
                let text = text.trim();
                if !text.is_empty() {
                    result.push(text.to_string());
                }
            }
            continue;
        }

        let (text, opened_multiline) = strip_line_comments(line);
        in_multiline = opened_multiline;

        let text = text.trim_end();
        if !text.is_empty() {
            result.push(text.to_string());
        } else if line.trim().is_empty() {
            // Пустая строка значима для разметки: она закрывает блоки и разделяет
            // абзацы, поэтому сохраняем её как есть.
            result.push(String::new());
        }
        // Иначе строка состояла только из комментария — выбрасываем её целиком.
    }

    result
}

// Открывающая/закрывающая линия ``` с учётом префиксов цитаты (`> ```).
fn is_fence_line(line: &str) -> bool {
    let mut rest = line.trim_start();
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.trim_start();
    }
    rest.starts_with("```")
}

// Убирает комментарии из одной строки, не задевая то, что комментарием не является:
//  - URL вида `tmutp://host/path` (`//` после `:`)
//  - `//` внутри инлайн-кода: `` `a // b` ``
//  - экранированные символы `\//`
//  - `//`, приклеенный к слову без пробела (`http://x/a//b`)
// Возвращает (оставшийся текст, открылся_ли_многострочный_комментарий).
fn strip_line_comments(line: &str) -> (String, bool) {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut in_code = false;

    while i < chars.len() {
        let c = chars[i];

        if c == '\\' && i + 1 < chars.len() {
            out.push(c);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }

        if c == '`' {
            in_code = !in_code;
            out.push(c);
            i += 1;
            continue;
        }

        if !in_code && c == '/' {
            match chars.get(i + 1).copied() {
                Some('/') => {
                    let prev = if i == 0 { None } else { Some(chars[i - 1]) };
                    let is_comment = match prev {
                        // начало строки — всегда комментарий
                        None => true,
                        // пробел/табуляция перед `//` — комментарий
                        Some(p) if p.is_whitespace() => true,
                        // `scheme://`, `a//b` — часть адреса, не комментарий
                        _ => false,
                    };
                    if is_comment {
                        return (out, false);
                    }
                    out.push(c);
                    i += 1;
                    continue;
                }
                Some('*') => {
                    let mut j = i + 2;
                    let mut closed = false;
                    while j + 1 < chars.len() {
                        if chars[j] == '*' && chars[j + 1] == '/' {
                            closed = true;
                            j += 2;
                            break;
                        }
                        j += 1;
                    }
                    if closed {
                        i = j;
                        continue;
                    }
                    return (out, true);
                }
                _ => {}
            }
        }

        out.push(c);
        i += 1;
    }

    (out, false)
}

fn trim_empty(lines: &[String]) -> Vec<String> {
    let mut start = 0;
    let mut end = lines.len();
    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }

    let mut result = Vec::new();
    let mut prev_empty = false;
    for line in &lines[start..end] {
        if line.trim().is_empty() {
            if !prev_empty {
                result.push(String::new());
                prev_empty = true;
            }
        } else {
            result.push(line.clone());
            prev_empty = false;
        }
    }
    result
}

fn trim_blank_edges(mut lines: Vec<String>) -> Vec<String> {
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

// ============================================================
// Разбор строк в блоки
// ============================================================

fn parse_lines(lines: &[String], doc: &mut Document, vars: &HashMap<String, String>) {
    parse_blocks(lines, doc, doc.root(), vars);
}

fn parse_blocks(
    lines: &[String],
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) {
    let mut i = 0;
    while i < lines.len() {
        let stripped = lines[i].trim();

        if stripped.is_empty() {
            doc.add_child(parent, NodeData::Paragraph(Vec::new()));
            i += 1;
            continue;
        }

        if stripped.starts_with("!!!") {
            i = parse_info_block(lines, i, doc, parent, vars);
        } else if stripped.starts_with("???") {
            i = parse_spoiler_block(lines, i, doc, parent, vars);
        } else if stripped.starts_with("%%") {
            i = parse_color_block(lines, i, doc, parent, vars);
        } else if stripped.starts_with('>') {
            i = parse_quote_block(lines, i, doc, parent, vars);
        } else if stripped.starts_with("```") {
            i = parse_code_block(lines, i, doc, parent);
        } else if stripped.starts_with('|') {
            i = parse_table_block(lines, i, doc, parent, vars);
        } else if stripped.starts_with("@@gopher-type") {
            i = parse_gopher_block(lines, i, doc, parent);
        } else if stripped.starts_with("@@") {
            i = parse_form_block(lines, i, doc, parent, vars);
        } else if is_list_item(&lines[i]) {
            i = parse_list_block(lines, i, doc, parent, vars);
        } else {
            i = parse_normal_line(lines, i, doc, parent, vars);
        }
    }
}

// ============================================================
// Блоки
// ============================================================

fn parse_info_block(
    lines: &[String],
    start: usize,
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) -> usize {
    let stripped = lines[start].trim();
    let rest = stripped.trim_start_matches('!').trim();
    let attrs = parse_attrs(rest);
    let kind = match attrs.get("type").map(String::as_str) {
        Some("warning") => InfoKind::Warning,
        Some("success") => InfoKind::Success,
        Some("error") => InfoKind::Error,
        _ => InfoKind::Info,
    };

    let node = doc.add_child(parent, NodeData::InfoBlock { kind });

    let mut inner = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim().starts_with("!!!") {
            i += 1;
            break;
        }
        inner.push(lines[i].clone());
        i += 1;
    }
    parse_blocks(&trim_blank_edges(inner), doc, node, vars);
    i
}

fn parse_spoiler_block(
    lines: &[String],
    start: usize,
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) -> usize {
    let stripped = lines[start].trim();
    let rest = stripped.trim_start_matches('?').trim();
    let attrs = parse_attrs(rest);
    let title = attrs.get("title").map(|t| parse_inline(t, vars));

    let node = doc.add_child(parent, NodeData::Spoiler { title });

    let mut inner = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim().starts_with("???") {
            i += 1;
            break;
        }
        inner.push(lines[i].clone());
        i += 1;
    }
    parse_blocks(&trim_blank_edges(inner), doc, node, vars);
    i
}

fn parse_color_block(
    lines: &[String],
    start: usize,
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) -> usize {
    let stripped = lines[start].trim();
    let rest = stripped.trim_start_matches('%').trim();
    let attrs = parse_attrs(rest);

    let color = attrs
        .get("color")
        .and_then(|c| color_by_name(c))
        .unwrap_or(Color::White);

    let border = match attrs.get("border").map(String::as_str) {
        Some("left") => ColorBorder::Left,
        _ => ColorBorder::Rounded,
    };

    let node = doc.add_child(parent, NodeData::ColorBlock { color, border });

    let mut inner = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim().starts_with("%%") {
            i += 1;
            break;
        }
        inner.push(lines[i].clone());
        i += 1;
    }
    parse_blocks(&trim_blank_edges(inner), doc, node, vars);
    i
}

fn parse_quote_block(
    lines: &[String],
    start: usize,
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) -> usize {
    let mut cite = None;
    let mut date = None;

    let first = lines[start].trim();
    let after_gt = first.strip_prefix('>').unwrap_or(first).trim();
    if after_gt.starts_with('{') && after_gt.ends_with('}') {
        let attrs = parse_attrs(after_gt);
        if attrs.contains_key("cite") || attrs.contains_key("date") {
            cite = attrs.get("cite").cloned();
            date = attrs.get("date").cloned();
        }
    }

    let node = doc.add_child(parent, NodeData::BlockQuote { cite, date });

    let mut inner = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let line = lines[i].trim();
        let Some(after) = line.strip_prefix('>') else {
            break;
        };
        let after = after.strip_prefix(' ').unwrap_or(after);

        if i == start && has_quote_attrs(after) {
            i += 1;
            continue;
        }

        inner.push(after.to_string());
        i += 1;
    }
    parse_blocks(&trim_blank_edges(inner), doc, node, vars);
    i
}

fn has_quote_attrs(line: &str) -> bool {
    let line = line.trim();
    if !(line.starts_with('{') && line.ends_with('}')) {
        return false;
    }
    let attrs = parse_attrs(line);
    attrs.contains_key("cite") || attrs.contains_key("date")
}

fn parse_code_block(lines: &[String], start: usize, doc: &mut Document, parent: indextree::NodeId) -> usize {
    let header = lines[start].trim().trim_start_matches('`').trim();
    let language = if header.is_empty() {
        None
    } else {
        Some(header.to_string())
    };

    let mut content = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim().starts_with("```") {
            i += 1;
            break;
        }
        content.push(lines[i].clone());
        i += 1;
    }

    doc.add_child(parent, NodeData::CodeBlock {
        language,
        text: content.join("\n"),
    });

    i
}

fn parse_table_block(
    lines: &[String],
    start: usize,
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) -> usize {
    let mut table_lines = Vec::new();
    let mut i = start;
    while i < lines.len() && lines[i].trim().starts_with('|') {
        table_lines.push(lines[i].clone());
        i += 1;
    }

    if table_lines.len() < 2 {
        for line in &table_lines {
            let spans = parse_inline(line, vars);
            doc.add_child(parent, NodeData::Paragraph(spans));
        }
        return i;
    }

    let split_row = |line: &str| -> Vec<String> {
        let stripped = line.trim().trim_start_matches('|').trim_end_matches('|');
        stripped.split('|').map(|c| c.trim().to_string()).collect()
    };

    let header_cells = split_row(&table_lines[0]);
    let sep_cells = split_row(&table_lines[1]);

    let alignments: Vec<Alignment> = sep_cells
        .iter()
        .map(|c| {
            let start_colon = c.starts_with(':');
            let end_colon = c.ends_with(':');
            match (start_colon, end_colon) {
                (true, true) => Alignment::Center,
                (false, true) => Alignment::Right,
                _ => Alignment::Left,
            }
        })
        .collect();

    let headers: Vec<Vec<Span>> = header_cells
        .iter()
        .map(|c| parse_inline(c, vars))
        .collect();

    let mut rows = Vec::new();
    for line in &table_lines[2..] {
        let cells = split_row(line);
        let row: Vec<Vec<Span>> = cells.iter().map(|c| parse_inline(c, vars)).collect();
        rows.push(row);
    }

    doc.add_child(parent, NodeData::Table {
        headers,
        rows,
        alignments,
    });

    i
}

fn parse_gopher_block(lines: &[String], start: usize, doc: &mut Document, parent: indextree::NodeId) -> usize {
    let header = lines[start].trim().to_string();
    let mut content = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim().starts_with("@@endgopher") {
            i += 1;
            break;
        }
        content.push(lines[i].clone());
        i += 1;
    }
    doc.add_child(parent, NodeData::GopherBlock {
        header,
        lines: content,
    });
    // Съедаем одну пустую строку сразу после блока
    if i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    i
}

fn parse_form_block(
    lines: &[String],
    start: usize,
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) -> usize {
    let stripped = lines[start].trim();
    let rest = stripped.trim_start_matches('@').trim();
    let attrs = parse_attrs(rest);

    let action = attrs.get("action").cloned().unwrap_or_default();
    let method = attrs.get("method").cloned().unwrap_or_else(|| "post".to_string());

    let mut items: Vec<FormItem> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let raw = lines[i].as_str();
        let line = raw.trim();
        if line.starts_with("@@") {
            i += 1;
            break;
        }

        if line.is_empty() {
            items.push(FormItem::Line(Vec::new()));
            i += 1;
            continue;
        }

        if let Some((field, consumed)) = try_parse_block_field(lines, i, vars) {
            items.push(FormItem::Field(field));
            i += consumed;
            continue;
        }

        if let Some(field) = try_parse_single_field(line, vars) {
            items.push(FormItem::Field(field));
            i += 1;
            continue;
        }

        let spans = parse_inline(line, vars);
        items.push(FormItem::Line(spans));
        i += 1;
    }

    doc.add_child(parent, NodeData::Form {
        action,
        method,
        items,
    });

    i
}

// ============================================================
// Нормальные строки
// ============================================================

fn parse_normal_line(
    lines: &[String],
    i: usize,
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) -> usize {
    let line = &lines[i];
    let stripped = line.trim();

    if stripped == "---" {
        doc.add_child(parent, NodeData::HorizontalRule { thick: false });
        return i + 1;
    }
    if stripped == "***" {
        doc.add_child(parent, NodeData::HorizontalRule { thick: true });
        return i + 1;
    }

    if let Some(level) = heading_level(stripped) {
        let text = stripped.trim_start_matches('#').trim_end_matches('#').trim();
        let spans = parse_inline(text, vars);
        doc.add_child(parent, NodeData::Heading { level, spans });
        return i + 1;
    }

    if let Some((name, text)) = try_footnote_def(stripped) {
        let sup = superscript_digits(&name);
        let mut spans = vec![Span {
            text: format!("⁽ {} ⁾", sup),
            style: SpanStyle {
                bold: true,
                fg: Some(Color::Blue),
                ..Default::default()
            },
            link: None,
        }];
        spans.push(Span::raw(" — "));
        spans.extend(parse_inline(&text, vars));
        doc.add_child(parent, NodeData::Paragraph(spans));
        return i + 1;
    }

    if let Some(text) = stripped.strip_prefix(':') {
        let spans = parse_inline(text.trim(), vars);
        doc.add_child(parent, NodeData::Definition { term: spans });
        return i + 1;
    }

    if let Some((alt, src, width, height)) = try_parse_image(stripped, vars) {
        doc.add_child(parent, NodeData::Image {
            alt,
            src,
            width,
            height,
        });
        return i + 1;
    }

    let spans = parse_inline(stripped, vars);
    if !spans.is_empty() {
        doc.add_child(parent, NodeData::Paragraph(spans));
    }
    i + 1
}


// ============================================================
// Вспомогательное для строк
// ============================================================

fn heading_level(line: &str) -> Option<u8> {
    let mut count = 0u8;
    for c in line.chars() {
        if c == '#' {
            count += 1;
        } else {
            break;
        }
    }
    if (1..=6).contains(&count) {
        Some(count)
    } else {
        None
    }
}

fn try_footnote_def(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("[^")?;
    let close = rest.find("]:")?;
    let name = rest[..close].to_string();
    let text = rest[close + 2..].trim_start().to_string();
    Some((name, text))
}

fn try_parse_image(
    line: &str,
    vars: &HashMap<String, String>,
) -> Option<(String, String, Option<u16>, Option<u16>)> {
    let rest = line.strip_prefix("![")?;
    let close = rest.find("](")?;
    let alt = substitute_vars(&rest[..close], vars);
    let after = &rest[close + 2..];
    let paren = after.find(')')?;
    let mut src_full = after[..paren].to_string();
    src_full = substitute_vars(&src_full, vars);

    let mut width = None;
    let mut height = None;

    if let Some(start) = src_full.find('{') {
        if let Some(end) = src_full.find('}') {
            let attr_str: String = src_full[start + 1..end].to_string();
            src_full = format!("{}{}", &src_full[..start], &src_full[end + 1..]);
            for part in attr_str.split_whitespace() {
                if let Some((k, v)) = part.split_once('=') {
                    let v = v.trim_matches('"');
                    match k.trim() {
                        "width" => width = v.parse().ok(),
                        "height" => height = v.parse().ok(),
                        _ => {}
                    }
                }
            }
        }
    }

    Some((alt, src_full.trim().to_string(), width, height))
}

// ============================================================
// Списки
// ============================================================

fn is_list_item(line: &str) -> bool {
    let s = line.trim_start();
    if s.starts_with("- ") || s.starts_with("* ") {
        return true;
    }
    is_ordered_item(s).is_some()
}

fn is_ordered_item(s: &str) -> Option<(String, String)> {
    let sep = s.find(". ")?;
    let num_part = &s[..sep];
    let rest = &s[sep + 2..];

    if num_part.is_empty() {
        return None;
    }
    if !num_part.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    if !num_part.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }

    Some((num_part.to_string(), rest.to_string()))
}

fn strip_list_marker(s: &str) -> Option<&str> {
    if let Some(r) = s.strip_prefix("- ") {
        return Some(r);
    }
    if let Some(r) = s.strip_prefix("* ") {
        return Some(r);
    }
    if let Some((_, r)) = is_ordered_item(s) {
        let offset = s.len() - r.len();
        return Some(&s[offset..]);
    }
    None
}

fn parse_list_block(
    lines: &[String],
    start: usize,
    doc: &mut Document,
    parent: indextree::NodeId,
    vars: &HashMap<String, String>,
) -> usize {
    let mut end = start;
    while end < lines.len() && is_list_item(&lines[end]) {
        end += 1;
    }
    let block = &lines[start..end];
    if block.is_empty() {
        return start;
    }

    let first = block[0].trim_start();
    let ordered = is_ordered_item(first).is_some();

    let mut counters: Vec<u64> = Vec::new();

    for line in block {
        let indent_chars = line.len() - line.trim_start().len();
        let level = indent_chars / 2;
        let indent = level as u8;
        let stripped = line.trim_start();

        let content = match strip_list_marker(stripped) {
            Some(c) => c,
            None => continue,
        };

        counters.truncate(level + 1);
        while counters.len() <= level {
            counters.push(0);
        }
        counters[level] += 1;

        let (marker, text) = if ordered {
            let path: String = if level == 0 {
                format!("{}.", counters[0])
            } else {
                counters[..=level]
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(".")
            };
            (ListMarker::Ordered(path), content)
        } else {
            (ListMarker::Bullet, content)
        };

        let spans = parse_inline(text, vars);
        doc.add_child(parent, NodeData::ListItem {
            indent,
            marker,
            spans,
        });
    }

    end
}

// ============================================================
// Атрибуты
// ============================================================

fn parse_attrs(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let s = text.trim();
    let s = s.trim_start_matches('{').trim_end_matches('}').trim();

    let mut chars = s.chars().peekable();
    let mut buf = String::new();
    let mut tokens: Vec<String> = Vec::new();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                buf.push('"');
                for nc in chars.by_ref() {
                    buf.push(nc);
                    if nc == '"' {
                        break;
                    }
                }
            }
            ' ' | '\t' => {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
            }
            _ => buf.push(c),
        }
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }

    for tok in tokens {
        if let Some((k, v)) = tok.split_once('=') {
            let v = v.trim_matches('"').to_string();
            map.insert(k.trim().to_string(), v);
        } else {
            map.insert(tok.trim().to_string(), "true".to_string());
        }
    }

    map
}

fn color_by_name(name: &str) -> Option<Color> {
    Some(match name {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::Gray,
        "bblack" => Color::DarkGray,
        "bred" => Color::LightRed,
        "bgreen" => Color::LightGreen,
        "byellow" => Color::LightYellow,
        "bblue" => Color::LightBlue,
        "bmagenta" => Color::LightMagenta,
        "bcyan" => Color::LightCyan,
        "bwhite" => Color::White,
        _ => return None,
    })
}

// ============================================================
// Поля формы
// ============================================================

fn try_parse_block_field(
    lines: &[String],
    start: usize,
    vars: &HashMap<String, String>,
) -> Option<(FormField, usize)> {
    let stripped = lines[start].trim();

    if let Some(rest) = stripped.strip_prefix("[radio") {
        if let Some(name) = extract_attr_string(rest, "name") {
            let mut options = Vec::new();
            let mut i = start + 1;
            while i < lines.len() {
                let l = lines[i].trim();
                if l == "[/radio]" {
                    i += 1;
                    break;
                }
                if let Some((label, attrs)) = parse_option_line(l) {
                    let value = attrs.get("value").cloned().unwrap_or_else(|| label.clone());
                    let checked = attrs.contains_key("checked");
                    options.push(RadioOption {
                        label: substitute_vars(&label, vars),
                        value: substitute_vars(&value, vars),
                        checked,
                    });
                }
                i += 1;
            }
            return Some((FormField::Radio { name, options }, i - start));
        }
    }

    if let Some(rest) = stripped.strip_prefix("[select") {
        if let Some(name) = extract_attr_string(rest, "name") {
            let multiple = rest.contains("multiple");
            let mut options = Vec::new();
            let mut i = start + 1;
            while i < lines.len() {
                let l = lines[i].trim();
                if l == "[/select]" {
                    i += 1;
                    break;
                }
                if let Some((label, attrs)) = parse_option_line(l) {
                    let value = attrs.get("value").cloned().unwrap_or_else(|| label.clone());
                    let selected = attrs.contains_key("selected");
                    options.push(SelectOption {
                        label: substitute_vars(&label, vars),
                        value: substitute_vars(&value, vars),
                        selected,
                    });
                }
                i += 1;
            }
            return Some((
                FormField::Select {
                    name,
                    options,
                    multiple,
                },
                i - start,
            ));
        }
    }

    None
}

fn parse_option_line(line: &str) -> Option<(String, HashMap<String, String>)> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let label = rest[..close].to_string();
    let after = rest[close + 1..].trim();
    let attrs = if after.starts_with('{') {
        parse_attrs(after)
    } else {
        HashMap::new()
    };
    Some((label, attrs))
}

fn extract_attr_string(s: &str, key: &str) -> Option<String> {
    let pattern = format!("{}=\"", key);
    let pos = s.find(&pattern)?;
    let after = &s[pos + pattern.len()..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn try_parse_single_field(line: &str, vars: &HashMap<String, String>) -> Option<FormField> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let label_raw = rest[..close].to_string();
    let after = rest[close + 1..].trim();

    if !after.starts_with('{') {
        return None;
    }

    let attrs = parse_attrs(after);

    let field_type = attrs.get("type").map(String::as_str).unwrap_or("text");

    let label = if label_raw.trim().is_empty() {
        attrs.get("label").cloned().unwrap_or_default()
    } else {
        label_raw.trim().to_string()
    };

    let name = attrs.get("name").cloned().unwrap_or_default();
    let placeholder = attrs.get("placeholder").cloned().unwrap_or_default();
    let value = attrs.get("value").cloned().unwrap_or_default();
    let required = attrs.contains_key("required");
    let checked = attrs.contains_key("checked");

    Some(match field_type {
        "email" => FormField::Text {
            label,
            name,
            placeholder,
            required,
            value,
            kind: FieldKind::Email,
        },
        "password" => FormField::Text {
            label,
            name,
            placeholder,
            required,
            value,
            kind: FieldKind::Password,
        },
        "textarea" => {
            let rows = attrs.get("rows").and_then(|s| s.parse().ok()).unwrap_or(3);
            FormField::Textarea {
                label,
                name,
                rows,
                placeholder,
                required,
                value,
            }
        }
        "checkbox" => FormField::Checkbox {
            label,
            name,
            checked,
            required,
        },
        "submit" => FormField::Submit { label },
        "reset" => FormField::Reset { label },
        "hidden" => FormField::Hidden { name, value },
        _ => FormField::Text {
            label,
            name,
            placeholder,
            required,
            value,
            kind: FieldKind::Text,
        },
    })
    .map(|f| substitute_field_vars(f, vars))
}

fn substitute_field_vars(field: FormField, vars: &HashMap<String, String>) -> FormField {
    match field {
        FormField::Text {
            label,
            name,
            placeholder,
            required,
            value,
            kind,
        } => FormField::Text {
            label: substitute_vars(&label, vars),
            name,
            placeholder: substitute_vars(&placeholder, vars),
            required,
            value: substitute_vars(&value, vars),
            kind,
        },
        FormField::Textarea {
            label,
            name,
            rows,
            placeholder,
            required,
            value,
        } => FormField::Textarea {
            label: substitute_vars(&label, vars),
            name,
            rows,
            placeholder: substitute_vars(&placeholder, vars),
            required,
            value: substitute_vars(&value, vars),
        },
        FormField::Checkbox {
            label,
            name,
            checked,
            required,
        } => FormField::Checkbox {
            label: substitute_vars(&label, vars),
            name,
            checked,
            required,
        },
        FormField::Radio { name, options } => FormField::Radio {
            name,
            options: options
                .into_iter()
                .map(|o| RadioOption {
                    label: substitute_vars(&o.label, vars),
                    value: o.value,
                    checked: o.checked,
                })
                .collect(),
        },
        FormField::Select {
            name,
            options,
            multiple,
        } => FormField::Select {
            name,
            options: options
                .into_iter()
                .map(|o| SelectOption {
                    label: substitute_vars(&o.label, vars),
                    value: o.value,
                    selected: o.selected,
                })
                .collect(),
            multiple,
        },
        FormField::Submit { label } => FormField::Submit {
            label: substitute_vars(&label, vars),
        },
        FormField::Reset { label } => FormField::Reset {
            label: substitute_vars(&label, vars),
        },
        FormField::Hidden { name, value } => FormField::Hidden {
            name,
            value: substitute_vars(&value, vars),
        },
    }
}

// ============================================================
// Переменные
// ============================================================

fn substitute_vars(text: &str, vars: &HashMap<String, String>) -> String {
    if vars.is_empty() || !text.contains("{{") {
        return text.to_string();
    }

    let mut result = String::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '{' && chars[i + 1] == '{' {
            let mut j = i + 2;
            let mut name = String::new();
            while j < chars.len() {
                if j + 1 < chars.len() && chars[j] == '}' && chars[j + 1] == '}' {
                    break;
                }
                name.push(chars[j]);
                j += 1;
            }
            if j + 1 < chars.len() {
                let key = name.trim().to_string();
                if let Some(v) = vars.get(&key) {
                    result.push_str(v);
                } else {
                    result.push_str("{{");
                    result.push_str(&name);
                    result.push_str("}}");
                }
                i = j + 2;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

// ============================================================
// Инлайн-парсер
// ============================================================

fn parse_inline(text: &str, vars: &HashMap<String, String>) -> Vec<Span> {
    let substituted = substitute_vars(text, vars);
    let chars: Vec<char> = substituted.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut buf = String::new();
    let mut style = SpanStyle::default();
    let mut fg_stack: Vec<Option<Color>> = Vec::new();
    let mut bg_stack: Vec<Option<Color>> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let rem = chars.len() - i;

        // [ ] [x] [~] — как текст, не как стили
        if rem >= 3 && chars[i] == '[' && chars[i + 2] == ']' {
            let mid = chars[i + 1];
            if mid == ' ' || mid == 'x' || mid == 'X' || mid == '~' {
                buf.push('[');
                buf.push(mid);
                buf.push(']');
                i += 3;
                continue;
            }
        }

        if rem >= 3 && chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*' {
            flush(&mut spans, &mut buf, &style);
            style.bold = !style.bold;
            style.italic = !style.italic;
            i += 3;
            continue;
        }
        if rem >= 2 && chars[i] == '*' && chars[i + 1] == '*' {
            flush(&mut spans, &mut buf, &style);
            style.bold = !style.bold;
            i += 2;
            continue;
        }
        if rem >= 2 && chars[i] == '~' && chars[i + 1] == '~' {
            flush(&mut spans, &mut buf, &style);
            style.strikethrough = !style.strikethrough;
            i += 2;
            continue;
        }
        if rem >= 2 && chars[i] == '=' && chars[i + 1] == '=' {
            flush(&mut spans, &mut buf, &style);
            if style.bg.is_none() {
                style.bg = Some(Color::Yellow);
                style.fg = Some(Color::Black);
            } else {
                style.bg = None;
                style.fg = None;
            }
            i += 2;
            continue;
        }

        match chars[i] {
            '*' => {
                flush(&mut spans, &mut buf, &style);
                style.italic = !style.italic;
                i += 1;
            }
            '`' => {
                flush(&mut spans, &mut buf, &style);
                style.code = !style.code;
                if style.code {
                    style.fg = Some(Color::Yellow);
                } else if style.fg == Some(Color::Yellow) {
                    style.fg = None;
                }
                i += 1;
            }
            '^' => {
                flush(&mut spans, &mut buf, &style);
                style.superscript = !style.superscript;
                i += 1;
            }
            '~' => {
                flush(&mut spans, &mut buf, &style);
                style.subscript = !style.subscript;
                i += 1;
            }
            '{' => {
                if let Some((consumed, tag)) = parse_color_tag(&chars[i..]) {
                    flush(&mut spans, &mut buf, &style);
                    apply_color_tag(&mut style, tag, &mut fg_stack, &mut bg_stack);
                    i += consumed;
                } else {
                    buf.push('{');
                    i += 1;
                }
            }
            '[' => {
                if let Some((consumed, content)) = try_parse_link(&chars[i..]) {
                    flush(&mut spans, &mut buf, &style);
                    match content {
                        LinkContent::Link {
                            text,
                            url,
                            tooltip,
                        } => {
                            let inner = parse_inline(&text, &HashMap::new());
                            for mut s in inner {
                                s.style.bold |= style.bold;
                                s.style.italic |= style.italic;
                                s.style.strikethrough |= style.strikethrough;
                                s.style.superscript |= style.superscript;
                                s.style.subscript |= style.subscript;
                                if s.style.fg.is_none() {
                                    s.style.fg = style.fg;
                                }
                                if s.style.bg.is_none() {
                                    s.style.bg = style.bg;
                                }
                                if s.style.fg.is_none() {
                                    s.style.fg = Some(Color::Cyan);
                                }
                                s.link = Some(LinkInfo {
                                    url: url.clone(),
                                    tooltip: tooltip.clone(),
                                });
                                spans.push(s);
                            }
                        }
                        LinkContent::Footnote(name) => {
                            let mut s = style;
                            s.bold = true;
                            s.fg = Some(Color::Blue);
                            let sup = superscript_digits(&name);
                            spans.push(Span {
                                text: format!("⁽ {} ⁾", sup),
                                style: s,
                                link: None,
                            });
                        }
                    }
                    i += consumed;
                } else {
                    buf.push('[');
                    i += 1;
                }
            }
            '\\' if i + 1 < chars.len() => {
                buf.push(chars[i + 1]);
                i += 2;
            }
            _ => {
                buf.push(chars[i]);
                i += 1;
            }
        }
    }

    flush(&mut spans, &mut buf, &style);
    spans
}

fn flush(spans: &mut Vec<Span>, buf: &mut String, style: &SpanStyle) {
    if buf.is_empty() {
        return;
    }
    spans.push(Span {
        text: std::mem::take(buf),
        style: *style,
        link: None,
    });
}

// ============================================================
// Цвета
// ============================================================

enum ColorTag {
    Fg(Color),
    Bg(Color),
    ResetAll,
    ResetFg,
    ResetBg,
}

fn parse_color_tag(chars: &[char]) -> Option<(usize, ColorTag)> {
    let mut i = 1;
    let mut name = String::new();
    while i < chars.len() && chars[i] != '}' {
        name.push(chars[i]);
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let consumed = i + 1;
    let name = name.trim();

    if name == "/" {
        return Some((consumed, ColorTag::ResetAll));
    }
    if let Some(rest) = name.strip_prefix('/') {
        if rest == "bg" || rest.starts_with("bg-") {
            return Some((consumed, ColorTag::ResetBg));
        }
        return Some((consumed, ColorTag::ResetFg));
    }
    if let Some(rest) = name.strip_prefix("bg-") {
        if let Some(c) = color_by_name(rest) {
            return Some((consumed, ColorTag::Bg(c)));
        }
    }
    if let Some(c) = color_by_name(name) {
        return Some((consumed, ColorTag::Fg(c)));
    }

    None
}

fn apply_color_tag(
    style: &mut SpanStyle,
    tag: ColorTag,
    fg_stack: &mut Vec<Option<Color>>,
    bg_stack: &mut Vec<Option<Color>>,
) {
    match tag {
        ColorTag::Fg(c) => {
            fg_stack.push(style.fg);
            style.fg = Some(c);
        }
        ColorTag::Bg(c) => {
            bg_stack.push(style.bg);
            style.bg = Some(c);
        }
        // `{/}` removes colors only; other attributes stay untouched.
        ColorTag::ResetAll => {
            style.fg = None;
            style.bg = None;
            fg_stack.clear();
            bg_stack.clear();
        }
        ColorTag::ResetFg => style.fg = fg_stack.pop().flatten(),
        ColorTag::ResetBg => style.bg = bg_stack.pop().flatten(),
    }
}

// ============================================================
// Ссылки и сноски
// ============================================================

enum LinkContent {
    Link {
        text: String,
        url: String,
        tooltip: Option<String>,
    },
    Footnote(String),
}

fn try_parse_link(chars: &[char]) -> Option<(usize, LinkContent)> {
    if chars.len() < 2 || chars[0] != '[' {
        return None;
    }

    if chars[1] == '^' {
        let mut i = 2;
        let mut name = String::new();
        while i < chars.len() && chars[i] != ']' {
            name.push(chars[i]);
            i += 1;
        }
        if i >= chars.len() {
            return None;
        }
        return Some((i + 1, LinkContent::Footnote(name)));
    }

    let mut i = 1;
    let mut text = String::new();
    let mut depth = 0;
    while i < chars.len() {
        match chars[i] {
            '[' => {
                depth += 1;
                text.push('[');
            }
            ']' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                text.push(']');
            }
            c => text.push(c),
        }
        i += 1;
    }
    if i >= chars.len() || chars[i] != ']' {
        return None;
    }
    i += 1;
    if i >= chars.len() || chars[i] != '(' {
        return None;
    }
    i += 1;

    let mut url = String::new();
    while i < chars.len() && chars[i] != ')' && chars[i] != ' ' && chars[i] != '"' {
        url.push(chars[i]);
        i += 1;
    }
    while i < chars.len() && chars[i] == ' ' {
        i += 1;
    }

    let mut tooltip = None;
    if i < chars.len() && chars[i] == '"' {
        i += 1;
        let mut t = String::new();
        while i < chars.len() && chars[i] != '"' {
            t.push(chars[i]);
            i += 1;
        }
        if i < chars.len() {
            i += 1;
        }
        tooltip = Some(t);
    }

    if i >= chars.len() || chars[i] != ')' {
        return None;
    }
    i += 1;

    Some((
        i,
        LinkContent::Link {
            text,
            url,
            tooltip,
        },
    ))
}

// ============================================================
// Сноски: суперскрипт
// ============================================================

fn superscript_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '+' => '⁺',
            '-' => '⁻',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_lines, strip_comments, trim_empty};
    use crate::core::document::{Document, NodeData};
    use indextree::NodeId;
    use std::collections::HashMap;

    #[test]
    fn keeps_url_double_slash() {
        let lines = strip_comments("[site](tmutp://tnet.example/index.tmkup)");
        assert_eq!(lines, vec!["[site](tmutp://tnet.example/index.tmkup)"]);
    }

    #[test]
    fn keeps_url_in_plain_text() {
        let lines = strip_comments("Go to https://tnet.example/page now");
        assert_eq!(lines, vec!["Go to https://tnet.example/page now"]);
    }

    #[test]
    fn strips_line_comment() {
        let lines = strip_comments("Hello // comment");
        assert_eq!(lines, vec!["Hello"]);
    }

    #[test]
    fn strips_comment_at_start() {
        let lines = strip_comments("// just a comment\nBody");
        assert_eq!(lines, vec!["Body"]);
    }

    #[test]
    fn keeps_double_slash_in_inline_code() {
        let lines = strip_comments("Путь `a // b` тут");
        assert_eq!(lines, vec!["Путь `a // b` тут"]);
    }

    #[test]
    fn keeps_escaped_double_slash() {
        let lines = strip_comments("a \\// b");
        assert_eq!(lines, vec!["a \\// b"]);
    }

    #[test]
    fn keeps_comment_markers_inside_fence() {
        let raw = "```rust\nlet x = 1; // comment\n/* block */\n```";
        let lines = strip_comments(raw);
        assert_eq!(
            lines,
            vec!["```rust", "let x = 1; // comment", "/* block */", "```"]
        );
    }

    #[test]
    fn strips_block_comment_single_line() {
        let lines = strip_comments("Before /* mid */ After");
        assert_eq!(lines, vec!["Before  After"]);
    }

    #[test]
    fn strips_multiline_block_comment() {
        let lines = strip_comments("A\n/* start\nmiddle\nend */\nB");
        assert_eq!(lines, vec!["A", "B"]);
    }

    #[test]
    fn multiline_block_comment_trainling_text_is_kept() {
        let lines = strip_comments("/* start\nend */ Tail");
        assert_eq!(lines, vec!["Tail"]);
    }

    #[test]
    fn keeps_blank_lines() {
        let lines = strip_comments("para one\n\npara two");
        assert_eq!(lines, vec!["para one", "", "para two"]);
    }

    #[test]
    fn comment_only_line_does_not_create_blank() {
        let lines = strip_comments("a\n// note\nb");
        assert_eq!(lines, vec!["a", "b"]);
    }

    fn parse_doc(raw: &str) -> Document {
        let mut doc = Document::new("text/tmkup");
        let lines = strip_comments(raw);
        let lines = trim_empty(&lines);
        parse_lines(&lines, &mut doc, &HashMap::new());
        doc
    }

    fn find_node(doc: &Document, pred: impl Fn(&NodeData) -> bool) -> Option<NodeId> {
        for (id, node) in doc.iter() {
            if pred(node) {
                return Some(id);
            }
        }
        None
    }

    fn has_child_heading(doc: &Document, parent: NodeId) -> bool {
        for (_, node) in doc.children(parent) {
            if matches!(node, NodeData::Heading { .. }) {
                return true;
            }
        }
        false
    }

    #[test]
    fn heading_inside_info_block() {
        let doc = parse_doc("!!!\n# Title\nBody\n!!!");
        let info = find_node(&doc, |n| matches!(n, NodeData::InfoBlock { .. })).unwrap();
        assert!(has_child_heading(&doc, info));
    }

    #[test]
    fn list_inside_quote_block() {
        let doc = parse_doc("> intro\n> - one\n> - two");
        let quote = find_node(&doc, |n| matches!(n, NodeData::BlockQuote { .. })).unwrap();
        let mut items = 0;
        for (_, node) in doc.children(quote) {
            if matches!(node, NodeData::ListItem { .. }) {
                items += 1;
            }
        }
        assert_eq!(items, 2);
    }

    #[test]
    fn quote_inside_info_block() {
        let doc = parse_doc("!!!\n> # Q\n> text\n!!!");
        let info = find_node(&doc, |n| matches!(n, NodeData::InfoBlock { .. })).unwrap();
        let mut quote = None;
        for (id, node) in doc.children(info) {
            if matches!(node, NodeData::BlockQuote { .. }) {
                quote = Some(id);
            }
        }
        assert!(has_child_heading(&doc, quote.unwrap()));
    }

    #[test]
    fn block_after_info_block_is_parsed_normally() {
        let doc = parse_doc("!!!\nbody\n!!!\n# After");
        let mut level_one = 0;
        for (_, node) in doc.iter() {
            if matches!(node, NodeData::Heading { level: 1, .. }) {
                level_one += 1;
            }
        }
        assert_eq!(level_one, 1);
    }

    #[test]
    fn nested_quote_keeps_heading() {
        let doc = parse_doc("> > # Deep\n> > text");
        let outer = find_node(&doc, |n| matches!(n, NodeData::BlockQuote { .. })).unwrap();
        let mut inner = None;
        for (id, node) in doc.children(outer) {
            if matches!(node, NodeData::BlockQuote { .. }) {
                inner = Some(id);
            }
        }
        assert!(has_child_heading(&doc, inner.unwrap()));
    }

    #[test]
    fn code_block_inside_quote_keeps_slashes() {
        let doc = parse_doc("> ```\n> let x = 1; // code\n> ```");
        let code = find_node(&doc, |n| matches!(n, NodeData::CodeBlock { .. })).unwrap();
        match doc.get(code) {
            Some(NodeData::CodeBlock { text, .. }) => {
                assert_eq!(text.as_str(), "let x = 1; // code");
            }
            _ => panic!("expected code block"),
        }
    }

    #[test]
    fn blank_line_inside_info_block_keeps_spacing() {
        let doc = parse_doc("!!!\ntext\n\nmore\n!!!");
        let info = find_node(&doc, |n| matches!(n, NodeData::InfoBlock { .. })).unwrap();
        let mut empty_paragraph = false;
        for (_, node) in doc.children(info) {
            if matches!(node, NodeData::Paragraph(spans) if spans.is_empty()) {
                empty_paragraph = true;
            }
        }
        assert!(empty_paragraph);
    }
    #[test]
    fn spoiler_title_keeps_inline_formatting() {
        let doc = parse_doc("??? {title=\"**Bold** and plain\"}\nbody\n???");
        let title = find_node(&doc, |n| matches!(n, NodeData::Spoiler { .. }))
            .and_then(|id| match doc.get(id) {
                Some(NodeData::Spoiler { title }) => title.clone(),
                _ => None,
            })
            .expect("spoiler title");
        assert_eq!(title.len(), 2, "{title:?}");
        assert_eq!(title[0].text, "Bold");
        assert!(title[0].style.bold);
        assert_eq!(title[1].text, " and plain");
        assert!(!title[1].style.bold);
    }

    #[test]
    fn spoiler_without_title_has_no_spans() {
        let doc = parse_doc("???\nbody\n???");
        let title = find_node(&doc, |n| matches!(n, NodeData::Spoiler { .. }))
            .and_then(|id| match doc.get(id) {
                Some(NodeData::Spoiler { title }) => Some(title.is_none()),
                _ => None,
            });
        assert_eq!(title, Some(true));
    }

    fn paragraph_spans(doc: &Document) -> Vec<crate::core::document::Span> {
        find_node(doc, |n| matches!(n, NodeData::Paragraph(_)))
            .and_then(|id| match doc.get(id) {
                Some(NodeData::Paragraph(spans)) => Some(spans.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn nested_color_restores_outer_color() {
        let doc = parse_doc("{red}a {green}b{/green} c{/red}");
        let spans = paragraph_spans(&doc);
        let by_text = |t: &str| {
            spans
                .iter()
                .find(|s| s.text == t)
                .map(|s| s.style.fg)
                .unwrap()
        };
        assert_eq!(by_text("a "), Some(ratatui::style::Color::Red));
        assert_eq!(by_text("b"), Some(ratatui::style::Color::Green));
        assert_eq!(by_text(" c"), Some(ratatui::style::Color::Red));
    }

    #[test]
    fn reset_all_clears_only_colors() {
        let doc = parse_doc("**{red}{bg-blue}x{/}**y");
        let spans = paragraph_spans(&doc);
        let styled = spans.iter().find(|s| s.text == "x").unwrap();
        assert_eq!(styled.style.fg, Some(ratatui::style::Color::Red));
        assert_eq!(styled.style.bg, Some(ratatui::style::Color::Blue));
        let plain = spans.iter().find(|s| s.text == "y").unwrap();
        assert_eq!(plain.style.fg, None);
        assert_eq!(plain.style.bg, None);
        assert!(!plain.style.bold);
    }

    #[test]
    fn quote_first_line_with_color_tag_is_kept() {
        let doc = parse_doc("> {blue}q{/blue}");
        let quote = find_node(&doc, |n| matches!(n, NodeData::BlockQuote { .. })).unwrap();
        let count = doc.children(quote).len();
        assert_eq!(count, 1);
        let spans = paragraph_spans(&doc);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "q");
        assert_eq!(spans[0].style.fg, Some(ratatui::style::Color::Blue));
    }

    #[test]
    fn quote_attrs_line_is_still_consumed() {
        let doc = parse_doc("> {cite=\"Иван\"}\n> text");
        let cite = find_node(&doc, |n| matches!(n, NodeData::BlockQuote { .. }))
            .and_then(|id| match doc.get(id) {
                Some(NodeData::BlockQuote { cite, .. }) => cite.clone(),
                _ => None,
            });
        assert_eq!(cite.as_deref(), Some("Иван"));
        let spans = paragraph_spans(&doc);
        assert_eq!(spans[0].text, "text");
    }

}
