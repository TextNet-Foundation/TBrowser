use anyhow::Result;
use async_trait::async_trait;
use indextree::NodeId;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span as RatSpan, Text},
};
use url::Url;

use crate::core::document::{
    Alignment, ColorBorder, Document, FieldKind, FormField, FormItem, InfoKind, ListMarker,
    NodeData, Span, SpanStyle,
};
use crate::core::renderer::base::Renderer;
use crate::core::renderer::code::get_code_renderer;
use crate::core::renderer::meta::{
    FieldValue, ImageState, InteractiveFieldKind, LineMeta, Link, RenderState, RenderedContent,
};
use crate::core::renderer::tmkup_theme::{BlockBorder, TmkupTheme};
use crate::core::settings::Settings;

pub struct TmkupRenderer {
    theme: TmkupTheme,
    settings: Settings,
}

impl TmkupRenderer {
    pub fn new(settings: &Settings) -> Self {
        Self {
            theme: TmkupTheme::from_settings(settings),
            settings: settings.clone(),
        }
    }
}

struct Out {
    lines: Vec<Line<'static>>,
    meta: Vec<LineMeta>,
}

impl Out {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            meta: Vec::new(),
        }
    }

    fn push(&mut self, line: Line<'static>, meta: LineMeta) {
        self.lines.push(line);
        self.meta.push(meta);
    }

    fn push_normal(&mut self, line: Line<'static>) {
        self.push(line, LineMeta::normal());
    }

    fn extend_normal(&mut self, lines: Vec<Line<'static>>) {
        for l in lines {
            self.push_normal(l);
        }
    }
}

struct Ctx<'a> {
    theme: &'a TmkupTheme,
    settings: &'a Settings,
    vw: u16,
    state: &'a RenderState,
    base_url: &'a str,
}

#[async_trait]
impl Renderer for TmkupRenderer {
    fn content_type(&self) -> &str {
        "text/tmkup"
    }

    async fn render(
        &self,
        doc: &Document,
        base_url: &str,
        viewport_width: u16,
        state: &RenderState,
    ) -> Result<RenderedContent> {
        let ctx = Ctx {
            theme: &self.theme,
            settings: &self.settings,
            vw: viewport_width,
            state,
            base_url,
        };
        let mut out = Out::new();
        render_children(doc, doc.root(), &mut out, &ctx);
        Ok(RenderedContent::new(Text::from(out.lines), out.meta))
    }
}

fn resolve_href(base_url: &str, href: &str) -> String {
    if href.is_empty() {
        return base_url.to_string();
    }
    if let Ok(base) = Url::parse(base_url) {
        if let Ok(joined) = base.join(href) {
            return joined.to_string();
        }
    }
    href.to_string()
}

fn collect_links(spans: &[Span], base_url: &str) -> Vec<Link> {
    spans
        .iter()
        .filter_map(|s| {
            s.link.as_ref().map(|l| Link {
                url: resolve_href(base_url, &l.url),
                tooltip: l.tooltip.clone(),
            })
        })
        .collect()
}

fn render_children(doc: &Document, parent: NodeId, out: &mut Out, ctx: &Ctx) {
    for (id, node) in doc.children(parent) {
        render_node(doc, id, node, out, ctx);
    }
}

fn render_child_lines(doc: &Document, parent: NodeId, ctx: &Ctx) -> Vec<Line<'static>> {
    let mut sub = Out::new();
    render_children(doc, parent, &mut sub, ctx);
    sub.lines
}

fn line_width(line: &Line<'static>) -> usize {
    line.spans.iter().map(|s| s.content.chars().count()).sum()
}

fn render_node(doc: &Document, id: NodeId, node: &NodeData, out: &mut Out, ctx: &Ctx) {
    let theme = ctx.theme;

    match node {
        NodeData::Root => {}

        NodeData::Paragraph(spans) => {
            if spans.is_empty() {
                out.push_normal(Line::from(""));
            } else {
                let links = collect_links(spans, ctx.base_url);
                out.push(
                    Line::from(spans_to_ratatui(spans, theme)),
                    LineMeta::with_links(links),
                );
            }
        }

        NodeData::Heading { spans, .. } => {
            let style = Style::default()
                .fg(theme.heading_fg)
                .add_modifier(Modifier::BOLD);
            let rat: Vec<RatSpan<'static>> = spans
                .iter()
                .map(|s| RatSpan::styled(sanitize(&s.text), style))
                .collect();
            out.push_normal(Line::from(rat));
        }

        NodeData::ListItem {
            indent,
            marker,
            spans,
        } => {
            let m = match marker {
                ListMarker::Bullet => "•".to_string(),
                ListMarker::Ordered(path) => path.clone(),
            };
            let prefix = format!("{}{} ", "  ".repeat(*indent as usize), m);
            let links = collect_links(spans, ctx.base_url);
            let mut rat = vec![RatSpan::raw(prefix)];
            rat.extend(spans_to_ratatui(spans, theme));
            out.push(Line::from(rat), LineMeta::with_links(links));
        }

        NodeData::Definition { term } => {
            let italic = Style::default().add_modifier(Modifier::ITALIC);
            let mut rat = vec![RatSpan::raw("    — ")];
            for s in term {
                rat.push(RatSpan::styled(sanitize(&s.text), italic));
            }
            out.push_normal(Line::from(rat));
        }

        NodeData::HorizontalRule { thick } => {
            let ch = if *thick { '━' } else { '─' };
            let width = ((ctx.vw as usize) * 3 / 4).max(1);
            out.push_normal(Line::from(ch.to_string().repeat(width)));
        }

        NodeData::CodeBlock { language, text } => {
            let lang = language.as_deref().unwrap_or("");
            let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();

            let renderer = get_code_renderer(lang, ctx.settings);
            let result = renderer.render(&lines, lang);

            let content: Vec<Vec<Span>> = result
                .text
                .lines
                .into_iter()
                .map(|line| {
                    line.spans
                        .into_iter()
                        .map(|rs| Span {
                            text: rs.content.into_owned(),
                            style: crate::core::document::SpanStyle {
                                fg: rs.style.fg,
                                bg: rs.style.bg,
                                bold: rs.style.add_modifier.contains(Modifier::BOLD),
                                italic: rs.style.add_modifier.contains(Modifier::ITALIC),
                                ..Default::default()
                            },
                            link: None,
                        })
                        .collect()
                })
                .collect();

            let label = theme.format_label(&result.label);
            let frame = build_frame(
                &label,
                &content,
                theme.border_fg,
                result.bg,
                result.fg,
                theme.code_border,
                None,
                theme,
            );
            out.extend_normal(frame);
        }

        NodeData::Image { alt, .. } => match ctx.state.images.get(&id) {
            Some(ImageState::Ready(lines)) => {
                for line in lines {
                    out.push_normal(line.clone());
                }
            }
            Some(ImageState::Failed(err)) => {
                out.push_normal(Line::from(format!("[{}] (Error: {})", alt, err)));
            }
            Some(ImageState::Loading) | None => {
                out.push_normal(Line::from(format!("[{}]", alt)));
            }
        },

        NodeData::BlockQuote { cite, date } => {
            let mut content = render_child_lines(doc, id, ctx);

            if let Some(c) = cite {
                let author_text = match date {
                    Some(d) => format!("— {} ({})", c, d),
                    None => format!("— {}", c),
                };

                let max_len = content.iter().map(line_width).max().unwrap_or(0);
                let author_len = author_text.chars().count();
                let padding = max_len.saturating_sub(author_len);
                let padded = format!("{}{}", " ".repeat(padding), author_text);

                content.push(Line::from(padded));
            }

            let frame = build_frame_lines(
                "",
                &content,
                theme.quote_fg,
                theme.quote_fg,
                Color::Black,
                theme.quote_border,
                None,
            );
            out.extend_normal(frame);
        }

        NodeData::InfoBlock { kind } => {
            let (label, color) = match kind {
                InfoKind::Info => ("INFO", theme.info_fg),
                InfoKind::Warning => ("WARN", theme.warning_fg),
                InfoKind::Success => (" OK ", theme.success_fg),
                InfoKind::Error => ("ERR ", theme.error_fg),
            };
            let content = render_child_lines(doc, id, ctx);
            let label_fmt = theme.format_label(label);
            let frame = build_frame_lines(
                &label_fmt,
                &content,
                color,
                color,
                Color::White,
                theme.info_border,
                Some(color),
            );
            out.extend_normal(frame);
        }

        NodeData::ColorBlock { color, border } => {
            let content = render_child_lines(doc, id, ctx);
            let border_style = match border {
                ColorBorder::Left => BlockBorder::Left,
                ColorBorder::Rounded => BlockBorder::Rounded,
            };
            let frame = build_frame_lines(
                "",
                &content,
                *color,
                *color,
                Color::Black,
                border_style,
                Some(*color),
            );
            out.extend_normal(frame);
        }

        NodeData::Spoiler { title } => {
            let expanded = ctx.state.spoilers.get(&id).copied().unwrap_or(false);
            let marker = if expanded { "▼" } else { "▶" };
            let default_title = [Span::raw("Спойлер")];
            let title_spans = title.as_deref().unwrap_or(&default_title);
            let header = spoiler_header_line(
                &theme.spoiler_line_format,
                marker,
                title_spans,
                theme,
            );

            let mut content = vec![header];

            if expanded {
                content.push(Line::from(""));
                content.extend(render_child_lines(doc, id, ctx));
            }

            let label = theme.format_label("SPOILER");
            let frame = build_frame_lines(
                &label,
                &content,
                theme.spoiler_fg,
                theme.spoiler_fg,
                Color::Black,
                theme.spoiler_border,
                None,
            );

            for (i, line) in frame.into_iter().enumerate() {
                if i == 1 {
                    out.push(line, LineMeta::spoiler(id, expanded));
                } else {
                    out.push_normal(line);
                }
            }
        }

        NodeData::Table {
            headers,
            rows,
            alignments,
        } => {
            render_table(
                headers,
                rows,
                alignments,
                out,
                theme.table_outer_border,
                theme.table_inner_border,
            );
        }

        NodeData::Form { items, .. } => {
            render_form(id, items, out, ctx.state, theme);
        }

        NodeData::GopherBlock { .. } => {}
    }
}

fn render_form(
    form_id: NodeId,
    items: &[FormItem],
    out: &mut Out,
    state: &RenderState,
    theme: &TmkupTheme,
) {
    let mut content: Vec<Vec<Span>> = Vec::new();
    let mut metas: Vec<LineMeta> = Vec::new();
    let mut field_index = 0usize;

    for item in items {
        match item {
            FormItem::Line(spans) => {
                content.push(spans.clone());
                metas.push(LineMeta::normal());
            }
            FormItem::Field(field) => {
                let (lines, line_metas) = field_to_lines(field, state, form_id, field_index);
                content.extend(lines);
                metas.extend(line_metas);
                field_index += 1;
            }
        }
    }

    let label = theme.format_label("FORM");
    let frame = build_frame(
        &label,
        &content,
        theme.border_fg,
        theme.border_fg,
        Color::Black,
        theme.form_border,
        None,
        theme,
    );

    let total = frame.len();
    for (i, line) in frame.into_iter().enumerate() {
        if i == 0 || i == total - 1 {
            out.push_normal(line);
        } else {
            let content_idx = i - 1;
            let meta = metas
                .get(content_idx)
                .cloned()
                .unwrap_or_else(LineMeta::normal);
            out.push(line, meta);
        }
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn build_frame(
    label: &str,
    content: &[Vec<Span>],
    border_color: Color,
    label_bg: Color,
    label_fg: Color,
    border: BlockBorder,
    content_fg: Option<Color>,
    theme: &TmkupTheme,
) -> Vec<Line<'static>> {
    let (tl, tr, bl, br, h, v) = border.chars();

    if border.is_left_only() {
        let mut out = Vec::new();
        for spans in content {
            let mut rat = vec![RatSpan::styled(
                format!("{} ", v),
                Style::default().fg(border_color),
            )];
            for s in spans {
                let mut style = span_style(s, theme);
                if s.style.fg.is_none() {
                    if let Some(fg) = content_fg {
                        style = style.fg(fg);
                    }
                }
                rat.push(RatSpan::styled(sanitize(&s.text), style));
            }
            out.push(Line::from(rat));
        }
        return out;
    }

    let mut content_width = 0usize;
    for spans in content {
        let len: usize = spans.iter().map(|s| s.text.chars().count()).sum();
        content_width = content_width.max(len);
    }

    let label_len = label.chars().count();

    let width = content_width + 6;
    let inner = content_width;
    if width < 4 {
        return Vec::new();
    }

    let border_style = Style::default().fg(border_color);
    let mut out = Vec::new();

    // top line
    let mut top: Vec<RatSpan<'static>> = Vec::new();
    top.push(RatSpan::styled(format!("{}{}", tl, h), border_style));
    if !label.is_empty() {
        top.push(RatSpan::styled(
            label.to_string(),
            Style::default().fg(label_fg).bg(label_bg),
        ));
        let after_len = width - 2 - label_len;
        let after = format!("{}{}", h.to_string().repeat(after_len + 1), tr);
        top.push(RatSpan::styled(after, border_style));
    } else {
        let after = format!("{}{}", h.to_string().repeat(width - 1), tr);
        top.push(RatSpan::styled(after, border_style));
    }
    out.push(Line::from(top));

    // content lines
    for spans in content {
        let mut rat: Vec<RatSpan<'static>> = Vec::new();
        rat.push(RatSpan::styled(format!("{}   ", v), border_style));
        let mut used = 0usize;
        for s in spans {
            let mut style = span_style(s, theme);
            if s.style.fg.is_none() {
                if let Some(fg) = content_fg {
                    style = style.fg(fg);
                }
            }
            rat.push(RatSpan::styled(sanitize(&s.text), style));
            used += s.text.chars().count();
        }
        if used < inner {
            rat.push(RatSpan::raw(" ".repeat(inner - used)));
        }
        rat.push(RatSpan::styled(format!("   {}", v), border_style));
        out.push(Line::from(rat));
    }

    // bottom line
    let bottom = format!("{}{}{}", bl, h.to_string().repeat(width), br);
    out.push(Line::from(RatSpan::styled(bottom, border_style)));

    out
}

fn build_frame_lines(
    label: &str,
    content: &[Line<'static>],
    border_color: Color,
    label_bg: Color,
    label_fg: Color,
    border: BlockBorder,
    content_fg: Option<Color>,
) -> Vec<Line<'static>> {
    let (tl, tr, bl, br, h, v) = border.chars();
    let border_style = Style::default().fg(border_color);

    if border.is_left_only() {
        return content
            .iter()
            .map(|line| {
                let mut rat = vec![RatSpan::styled(format!("{} ", v), border_style)];
                rat.extend(tinted_spans(line, content_fg));
                Line::from(rat)
            })
            .collect();
    }

    let content_width = content.iter().map(line_width).max().unwrap_or(0);
    let label_len = label.chars().count();
    let min_width = if label.is_empty() { 4 } else { label_len + 3 };
    let width = (content_width + 6).max(min_width);
    let inner = width - 6;

    let mut out = Vec::new();

    let mut top: Vec<RatSpan<'static>> = Vec::new();
    top.push(RatSpan::styled(format!("{}{}", tl, h), border_style));
    if !label.is_empty() {
        top.push(RatSpan::styled(
            label.to_string(),
            Style::default().fg(label_fg).bg(label_bg),
        ));
        let after_len = width - 2 - label_len;
        let after = format!("{}{}", h.to_string().repeat(after_len + 1), tr);
        top.push(RatSpan::styled(after, border_style));
    } else {
        let after = format!("{}{}", h.to_string().repeat(width - 1), tr);
        top.push(RatSpan::styled(after, border_style));
    }
    out.push(Line::from(top));

    for line in content {
        let mut rat: Vec<RatSpan<'static>> = Vec::new();
        rat.push(RatSpan::styled(format!("{}   ", v), border_style));
        let used = line_width(line);
        rat.extend(tinted_spans(line, content_fg));
        if used < inner {
            rat.push(RatSpan::raw(" ".repeat(inner - used)));
        }
        rat.push(RatSpan::styled(format!("   {}", v), border_style));
        out.push(Line::from(rat));
    }

    let bottom = format!("{}{}{}", bl, h.to_string().repeat(width), br);
    out.push(Line::from(RatSpan::styled(bottom, border_style)));

    out
}

fn tinted_spans(line: &Line<'static>, content_fg: Option<Color>) -> Vec<RatSpan<'static>> {
    line.spans
        .iter()
        .cloned()
        .map(|mut span| {
            if span.style.fg.is_none() {
                if let Some(fg) = content_fg {
                    span.style.fg = Some(fg);
                }
            }
            span
        })
        .collect()
}

fn render_table(
    headers: &[Vec<Span>],
    rows: &[Vec<Vec<Span>>],
    alignments: &[Alignment],
    out: &mut Out,
    outer: BlockBorder,
    inner: BlockBorder,
) {
    let o = outer.table_chars();
    let i = inner.table_chars();

    let header_texts: Vec<String> = headers
        .iter()
        .map(|c| c.iter().map(|s| sanitize(&s.text)).collect())
        .collect();

    let row_texts: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| c.iter().map(|s| sanitize(&s.text)).collect::<String>())
                .collect()
        })
        .collect();

    let cols = headers
        .len()
        .max(row_texts.iter().map(|r| r.len()).max().unwrap_or(0));
    if cols == 0 {
        return;
    }

    let mut widths = vec![0usize; cols];
    for (i, h) in header_texts.iter().enumerate() {
        widths[i] = widths[i].max(h.chars().count());
    }
    for row in &row_texts {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }

    let make_border = |l: char, m: char, r: char, h: char| -> String {
        let parts: Vec<String> = widths
            .iter()
            .map(|w| h.to_string().repeat(w + 2))
            .collect();
        format!("{}{}{}", l, parts.join(&m.to_string()), r)
    };

    let row_str = |cells: &[String], v: char| -> String {
        let mut parts = Vec::new();
        for i in 0..cols {
            let c = cells.get(i).map(String::as_str).unwrap_or("");
            let w = widths[i];
            let visible = c.chars().count();
            let pad = w.saturating_sub(visible);
            let align = alignments.get(i).copied().unwrap_or(Alignment::Left);
            let padded = match align {
                Alignment::Left => format!("{}{}", c, " ".repeat(pad)),
                Alignment::Right => format!("{}{}", " ".repeat(pad), c),
                Alignment::Center => {
                    let left = pad / 2;
                    let right = pad - left;
                    format!("{}{}{}", " ".repeat(left), c, " ".repeat(right))
                }
            };
            parts.push(format!(" {} ", padded));
        }
        format!("{}{}{}", v, parts.join(&v.to_string()), v)
    };

    out.push_normal(Line::from(make_border(o.top_left, o.top_mid, o.top_right, o.horizontal)));
    if !header_texts.is_empty() {
        out.push_normal(Line::from(row_str(&header_texts, o.vertical)));
        out.push_normal(Line::from(make_border(i.mid_left, i.cross, i.mid_right, i.horizontal)));
    }
    for row in &row_texts {
        out.push_normal(Line::from(row_str(row, o.vertical)));
    }
    out.push_normal(Line::from(make_border(o.bot_left, o.bot_mid, o.bot_right, o.horizontal)));
}

fn required_marker(form_id: NodeId, field_index: usize, state: &RenderState) -> Span {
    let has_err = state.field_errors.contains(&(form_id, field_index));
    Span {
        text: "*".to_string(),
        style: SpanStyle {
            reversed: has_err,
            ..Default::default()
        },
        link: None,
    }
}

fn field_to_lines(
    field: &FormField,
    state: &RenderState,
    form_id: NodeId,
    field_index: usize,
) -> (Vec<Vec<Span>>, Vec<LineMeta>) {
    let key = (form_id, field_index);

    match field {
        FormField::Text {
            label,
            placeholder,
            required,
            value,
            kind,
            ..
        } => {
            let display = match state.fields.get(&key) {
                Some(FieldValue::Text(t)) => {
                    if t.is_empty() {
                        if !placeholder.is_empty() {
                            placeholder.clone()
                        } else {
                            "____".to_string()
                        }
                    } else {
                        match kind {
                            FieldKind::Password => "*".repeat(t.chars().count()),
                            _ => t.clone(),
                        }
                    }
                }
                _ => {
                    if !value.is_empty() {
                        match kind {
                            FieldKind::Password => "*".repeat(value.chars().count()),
                            _ => value.clone(),
                        }
                    } else if !placeholder.is_empty() {
                        placeholder.clone()
                    } else {
                        "____".to_string()
                    }
                }
            };

            let has_err = state.field_errors.contains(&key);

            let mut spans: Vec<Span> = Vec::new();
            if matches!(kind, FieldKind::Email) && has_err {
                spans.push(Span::raw(format!("{}: ", label)));
                spans.push(Span {
                    text: display,
                    style: SpanStyle {
                        reversed: true,
                        ..Default::default()
                    },
                    link: None,
                });
                if *required {
                    spans.push(Span::raw(" "));
                    spans.push(required_marker(form_id, field_index, state));
                }
                spans.push(Span::raw(" (Needs to be an email)"));
            } else {
                spans.push(Span::raw(format!("{}: {} ", label, display)));
                if *required {
                    spans.push(required_marker(form_id, field_index, state));
                }
            }

            let ifk = match kind {
                FieldKind::Email => InteractiveFieldKind::Email,
                FieldKind::Password => InteractiveFieldKind::Password,
                FieldKind::Text => InteractiveFieldKind::Text,
            };
            (
                vec![spans],
                vec![LineMeta::form_text(form_id, field_index, ifk)],
            )
        }
        FormField::Textarea {
            label,
            required,
            placeholder,
            value,
            ..
        } => {
            let display = match state.fields.get(&key) {
                Some(FieldValue::Text(t)) if !t.is_empty() => t.clone(),
                _ => {
                    if value.is_empty() {
                        placeholder.clone()
                    } else {
                        value.clone()
                    }
                }
            };

            let mut content: Vec<Vec<Span>> = Vec::new();
            let mut metas: Vec<LineMeta> = Vec::new();

            let mut head = vec![Span::raw(format!("{}:", label))];
            if *required {
                head.push(Span::raw(" "));
                head.push(required_marker(form_id, field_index, state));
            }
            content.push(head);
            metas.push(LineMeta::form_text(
                form_id,
                field_index,
                InteractiveFieldKind::Textarea,
            ));

            if display.is_empty() {
                content.push(vec![Span::raw(String::new())]);
                metas.push(LineMeta::form_text(
                    form_id,
                    field_index,
                    InteractiveFieldKind::Textarea,
                ));
            } else {
                for line in display.split('\n') {
                    content.push(vec![Span::raw(format!("    {}", line))]);
                    metas.push(LineMeta::form_text(
                        form_id,
                        field_index,
                        InteractiveFieldKind::Textarea,
                    ));
                }
            }

            (content, metas)
        }
        FormField::Checkbox {
            label,
            checked,
            required,
            ..
        } => {
            let display_checked = match state.fields.get(&key) {
                Some(FieldValue::Checked(b)) => *b,
                _ => *checked,
            };
            let mark = if display_checked { "[x]" } else { "[ ]" };
            let mut spans = vec![Span::raw(format!("{} {}", mark, label))];
            if *required {
                spans.push(Span::raw(" "));
                spans.push(required_marker(form_id, field_index, state));
            }
            (
                vec![spans],
                vec![LineMeta::form_checkbox(form_id, field_index)],
            )
        }
        FormField::Radio { options, .. } => {
            let selected_idx = match state.fields.get(&key) {
                Some(FieldValue::Index(i)) => *i,
                _ => options.iter().position(|o| o.checked).unwrap_or(0),
            };
            let mut lines = Vec::new();
            let mut metas = Vec::new();
            for (i, opt) in options.iter().enumerate() {
                let mark = if i == selected_idx { "(x)" } else { "( )" };
                lines.push(vec![Span::raw(format!("{} {}", mark, opt.label))]);
                metas.push(LineMeta::form_radio_option(form_id, field_index, i));
            }
            (lines, metas)
        }
        FormField::Select { options, .. } => {
            let selected_idx = match state.fields.get(&key) {
                Some(FieldValue::Index(i)) => *i,
                _ => options.iter().position(|o| o.selected).unwrap_or(0),
            };
            let selected = options
                .get(selected_idx)
                .map(|o| o.label.clone())
                .unwrap_or_default();

            let is_open = state.select_open.contains(&key);

            if is_open {
                let mut lines = vec![vec![Span::raw(format!("[ {} ▲ ]", selected))]];
                let mut metas = vec![LineMeta::form_select_header(form_id, field_index)];
                for (i, opt) in options.iter().enumerate() {
                    let marker = if i == selected_idx { ">" } else { " " };
                    lines.push(vec![Span::raw(format!("  {} {}", marker, opt.label))]);
                    metas.push(LineMeta::form_select_option(form_id, field_index, i));
                }
                (lines, metas)
            } else {
                (
                    vec![vec![Span::raw(format!("[ {} ▼ ]", selected))]],
                    vec![LineMeta::form_select_collapsed(form_id, field_index)],
                )
            }
        }
        FormField::Submit { label } => {
            let mut lines = vec![vec![Span::raw(format!("[ {} ]", label))]];
            let mut metas = vec![LineMeta::form_submit(form_id)];
            if let Some(err) = state.form_errors.get(&form_id) {
                lines.push(vec![Span {
                    text: err.clone(),
                    style: SpanStyle {
                        fg: Some(Color::Red),
                        ..Default::default()
                    },
                    link: None,
                }]);
                metas.push(LineMeta::normal());
            }
            (lines, metas)
        }
        FormField::Reset { label } => (
            vec![vec![Span::raw(format!("[ {} ]", label))]],
            vec![LineMeta::form_reset(form_id)],
        ),
        FormField::Hidden { .. } => (vec![], vec![]),
    }
}

fn spoiler_header_line(
    format: &str,
    marker: &str,
    title: &[Span],
    theme: &TmkupTheme,
) -> Line<'static> {
    let mut spans: Vec<RatSpan<'static>> = Vec::new();
    for (i, part) in format.split("{title}").enumerate() {
        if i > 0 {
            spans.extend(spans_to_ratatui(title, theme));
        }
        let text = part.replace("{marker}", marker);
        if !text.is_empty() {
            spans.push(RatSpan::raw(text));
        }
    }
    Line::from(spans)
}

fn spans_to_ratatui(spans: &[Span], theme: &TmkupTheme) -> Vec<RatSpan<'static>> {
    spans
        .iter()
        .map(|s| RatSpan::styled(sanitize(&s.text), span_style(s, theme)))
        .collect()
}

fn span_style(s: &Span, theme: &TmkupTheme) -> Style {
    let mut st = Style::default();
    if s.style.bold {
        st = st.add_modifier(Modifier::BOLD);
    }
    if s.style.italic {
        st = st.add_modifier(Modifier::ITALIC);
    }
    if s.style.strikethrough {
        st = st.add_modifier(Modifier::CROSSED_OUT);
    }
    if s.style.superscript {
        st = st.add_modifier(Modifier::BOLD);
    }
    if s.style.subscript {
        st = st.add_modifier(Modifier::DIM);
    }
    if s.style.reversed {
        st = st.add_modifier(Modifier::REVERSED);
    }
    if s.style.code {
        st = st.fg(theme.code_fg);
    }
    if let Some(fg) = s.style.fg {
        st = st.fg(fg);
    }
    if let Some(bg) = s.style.bg {
        st = st.bg(bg);
    }
    if s.link.is_some() && s.style.fg.is_none() {
        st = st.fg(theme.link_fg).add_modifier(Modifier::UNDERLINED);
    }
    st
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser::get_parser;

    async fn render_to_string(markup: &str) -> String {
        let settings = Settings::new();
        let parser = get_parser("text/tmkup");
        let doc = parser.parse(markup, "text/tmkup").await;
        let renderer = TmkupRenderer::new(&settings);
        let rendered = renderer
            .render(&doc, "tmutp://example.tnet/", 80, &RenderState::default())
            .await
            .unwrap();
        rendered
            .text
            .lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn heading_and_body_inside_info_block_are_rendered() {
        let out = render_to_string("!!!\n# Title\nBody\n!!!").await;
        assert!(out.contains("INFO"), "{out}");
        assert!(out.contains("Title"), "{out}");
        assert!(out.contains("Body"), "{out}");
    }

    #[tokio::test]
    async fn list_inside_quote_block_is_rendered() {
        let out = render_to_string("> intro\n> - one\n> - two").await;
        assert!(out.contains("intro"), "{out}");
        assert!(out.contains("• one"), "{out}");
        assert!(out.contains("• two"), "{out}");
    }

    #[tokio::test]
    async fn nested_quote_heading_is_rendered() {
        let out = render_to_string("> > # Deep\n> > text").await;
        assert!(out.contains("Deep"), "{out}");
        assert!(out.contains("text"), "{out}");
    }
    #[tokio::test]
    async fn spoiler_title_formatting_is_applied() {
        let out = render_to_string("??? {title=\"**Bold** and plain\"}\nbody\n???").await;
        assert!(out.contains("[▶] Bold and plain"), "{out}");
        assert!(!out.contains("**"), "{out}");
    }

    #[test]
    fn frame_tints_uncolored_content() {
        let content = vec![Line::from(RatSpan::raw("text"))];
        let out = build_frame_lines(
            "",
            &content,
            Color::Blue,
            Color::Blue,
            Color::Black,
            BlockBorder::Rounded,
            Some(Color::Red),
        );
        let span = out
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.contains("text"))
            .unwrap();
        assert_eq!(span.style.fg, Some(Color::Red));
    }

    #[test]
    fn frame_keeps_explicit_content_color() {
        let content = vec![Line::from(RatSpan::styled(
            "text",
            Style::default().fg(Color::Green),
        ))];
        let out = build_frame_lines(
            "",
            &content,
            Color::Blue,
            Color::Blue,
            Color::Black,
            BlockBorder::Rounded,
            Some(Color::Red),
        );
        let span = out
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.contains("text"))
            .unwrap();
        assert_eq!(span.style.fg, Some(Color::Green));
    }

}
