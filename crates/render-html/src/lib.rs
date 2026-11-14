//! First-party HTML renderer plugin.
//!
//! Renders each document to a standalone HTML page. The page title is the
//! `title` front matter, else the first heading, else the source path.
//! Templates, themes and asset handling come in Phase 4.

use lsdocs_plugin_sdk::doctree::{Align, Document, Node};
use lsdocs_plugin_sdk::protocol::{RenderParams, RenderResult};
use lsdocs_plugin_sdk::{Context, Plugin, PluginError, PluginResult, Renderer};

/// The plugin, as registered with the host.
pub fn plugin() -> Plugin {
    Plugin::new("render-html", env!("CARGO_PKG_VERSION")).renderer(Html)
}

/// The HTML renderer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Html;

impl Renderer for Html {
    fn formats(&self) -> Vec<String> {
        vec!["html".into()]
    }

    async fn render(&self, params: RenderParams, ctx: &Context) -> PluginResult<RenderResult> {
        if params.format != "html" {
            return Err(PluginError::new(format!(
                "unsupported format `{}`",
                params.format
            )));
        }
        let site_title = ctx.site().title.as_deref();
        Ok(RenderResult {
            output: page(&params.document, site_title).into_bytes(),
            extension: "html".into(),
            diagnostics: vec![],
        })
    }
}

/// Renders a full HTML page.
pub fn page(doc: &Document, site_title: Option<&str>) -> String {
    let title = doc
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| {
            doc.children.iter().find_map(|n| match n {
                Node::Heading(h) => Some(plain_text(&h.children)),
                _ => None,
            })
        })
        .unwrap_or_else(|| doc.source.clone());
    let full_title = match site_title {
        Some(site) if site != title => format!("{title} | {site}"),
        _ => title,
    };
    let mut body = String::new();
    for node in &doc.children {
        render(node, &mut body);
    }
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape(&full_title)
    )
}

fn plain_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&t.value),
            Node::InlineCode(c) => out.push_str(&c.value),
            other => out.push_str(&plain_text(other.children())),
        }
    }
    out
}

/// Escapes text for HTML content and attribute values.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn children(nodes: &[Node], out: &mut String) {
    for node in nodes {
        render(node, out);
    }
}

fn wrap(tag: &str, attrs: &str, nodes: &[Node], out: &mut String, newline: bool) {
    out.push_str(&format!("<{tag}{attrs}>"));
    children(nodes, out);
    out.push_str(&format!("</{tag}>"));
    if newline {
        out.push('\n');
    }
}

fn id_attr(id: &Option<String>) -> String {
    id.as_ref()
        .map(|id| format!(" id=\"{}\"", escape(id)))
        .unwrap_or_default()
}

fn render(node: &Node, out: &mut String) {
    match node {
        Node::Section(n) => wrap("section", &id_attr(&n.id), &n.children, out, true),
        Node::Heading(n) => {
            let level = n.level.clamp(1, 6);
            wrap(
                &format!("h{level}"),
                &id_attr(&n.id),
                &n.children,
                out,
                true,
            );
        }
        Node::Paragraph(n) => wrap("p", "", &n.children, out, true),
        Node::List(n) => {
            let (tag, attrs) = match (n.ordered, n.start) {
                (true, Some(start)) if start != 1 => ("ol", format!(" start=\"{start}\"")),
                (true, _) => ("ol", String::new()),
                (false, _) => ("ul", String::new()),
            };
            out.push_str(&format!("<{tag}{attrs}>\n"));
            children(&n.children, out);
            out.push_str(&format!("</{tag}>\n"));
        }
        Node::ListItem(n) => {
            out.push_str("<li>");
            if let Some(checked) = n.checked {
                let mark = if checked { " checked" } else { "" };
                out.push_str(&format!("<input type=\"checkbox\" disabled{mark}> "));
            }
            children(&n.children, out);
            out.push_str("</li>\n");
        }
        Node::BlockQuote(n) => wrap("blockquote", "", &n.children, out, true),
        Node::CodeBlock(n) => {
            let class = n
                .language
                .as_ref()
                .map(|l| format!(" class=\"language-{}\"", escape(l)))
                .unwrap_or_default();
            out.push_str(&format!(
                "<pre><code{class}>{}</code></pre>\n",
                escape(&n.value)
            ));
        }
        Node::Table(n) => {
            out.push_str("<table>\n");
            for row in &n.children {
                let Node::TableRow(row) = row else {
                    render(row, out);
                    continue;
                };
                let cell_tag = if row.header { "th" } else { "td" };
                out.push_str("<tr>");
                for (i, cell) in row.children.iter().enumerate() {
                    let style = match n.align.get(i) {
                        Some(Align::Left) => " style=\"text-align: left\"",
                        Some(Align::Center) => " style=\"text-align: center\"",
                        Some(Align::Right) => " style=\"text-align: right\"",
                        _ => "",
                    };
                    wrap(cell_tag, style, cell.children(), out, false);
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</table>\n");
        }
        Node::TableRow(n) => wrap("tr", "", &n.children, out, true),
        Node::TableCell(n) => wrap("td", "", &n.children, out, false),
        Node::ThematicBreak(_) => out.push_str("<hr>\n"),
        Node::Raw(n) => {
            if n.format == "html" {
                out.push_str(&n.value);
            }
        }
        Node::Text(n) => out.push_str(&escape(&n.value)),
        Node::Emphasis(n) => wrap("em", "", &n.children, out, false),
        Node::Strong(n) => wrap("strong", "", &n.children, out, false),
        Node::InlineCode(n) => out.push_str(&format!("<code>{}</code>", escape(&n.value))),
        Node::Link(n) => {
            let title = n
                .title
                .as_ref()
                .map(|t| format!(" title=\"{}\"", escape(t)))
                .unwrap_or_default();
            wrap(
                "a",
                &format!(" href=\"{}\"{title}", escape(&n.url)),
                &n.children,
                out,
                false,
            );
        }
        Node::Image(n) => {
            let title = n
                .title
                .as_ref()
                .map(|t| format!(" title=\"{}\"", escape(t)))
                .unwrap_or_default();
            out.push_str(&format!(
                "<img src=\"{}\" alt=\"{}\"{title}>",
                escape(&n.url),
                escape(&n.alt)
            ));
        }
        Node::LineBreak(_) => out.push_str("<br>\n"),
        Node::Custom(n) => {
            let attrs = format!(" class=\"{}\"", escape(&n.kind));
            wrap("div", &attrs, &n.children, out, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsdocs_plugin_sdk::doctree::{Heading, Paragraph, text};

    #[test]
    fn renders_a_page_and_escapes() {
        let mut doc = Document::new("a.md");
        doc.children = vec![
            Node::Heading(Heading {
                level: 1,
                id: Some("t".into()),
                children: vec![text("Tom & Jerry")],
                ..Heading::default()
            }),
            Node::Paragraph(Paragraph {
                children: vec![text("<script>")],
                ..Paragraph::default()
            }),
        ];
        let html = page(&doc, Some("Site"));
        assert!(
            html.contains("<title>Tom &amp; Jerry | Site</title>"),
            "{html}"
        );
        assert!(
            html.contains("<h1 id=\"t\">Tom &amp; Jerry</h1>\n<p>&lt;script&gt;</p>\n"),
            "{html}"
        );
    }
}
