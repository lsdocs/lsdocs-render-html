//! The body writer: a document's nodes as HTML, without a page around
//! them.
//!
//! The markup is a contract between this plugin and its themes: every
//! class and attribute it emits is prefixed `lsd-` (or is a standard
//! HTML attribute), and `docs/content/reference/html-markup.md` lists
//! them. Change them with care.

use lsdocs_plugin_sdk::doctree::{
    Admonition, Align, Caption, CodeBlock, Custom, Document, Heading, Link, LinkTarget, Node,
};
use lsdocs_plugin_sdk::relative_path;

use crate::Options;

/// Elements a block `custom` node may ask for with `html_tag`. A `pre`
/// keeps its content's spaces and newlines; for text drawn like a code
/// block, its child is an inline `custom` node asking for `code`, as code
/// blocks are `pre` and `code`.
const BLOCK_TAGS: &[&str] = &[
    "div", "p", "h1", "h2", "h3", "h4", "h5", "h6", "section", "aside", "nav", "pre",
];

/// Elements an inline `custom` node may ask for with `html_tag`.
const INLINE_TAGS: &[&str] = &[
    "span", "abbr", "cite", "code", "dfn", "em", "kbd", "mark", "q", "samp", "small", "strong",
    "sub", "sup", "var",
];

/// Renders the document's nodes, without a page around them.
pub fn body(doc: &Document, options: &Options) -> String {
    let page = Page {
        options,
        source: &doc.source,
    };
    let mut out = String::new();
    children(&page, &doc.children, &mut out);
    out
}

/// What rendering a node needs besides the node: the options and the
/// source path of the page it is on.
struct Page<'a> {
    options: &'a Options,
    source: &'a str,
}

/// Where a document's HTML is written, relative to the output directory:
/// its source path with the extension replaced by `html`.
pub fn output_path(source: &str) -> String {
    match source.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() && !stem.ends_with('/') => format!("{stem}.html"),
        _ => format!("{source}.html"),
    }
}

/// The `href` of a link on the page `from`: built from its target if it has
/// a resolved one, else its URL (with links to `.md` files rewritten).
pub fn link_href(from: &str, link: &Link) -> String {
    match &link.target {
        Some(LinkTarget {
            document,
            anchor,
            label: None,
        }) if document.is_some() || anchor.is_some() => {
            let mut href = match document {
                Some(document) if document != from || anchor.is_none() => {
                    relative_path(from, &output_path(document))
                }
                _ => String::new(),
            };
            if let Some(anchor) = anchor {
                href.push('#');
                href.push_str(anchor);
            }
            href
        }
        _ => rewrite_link(&link.url),
    }
}

/// `note` becomes `Note`.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
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

/// Rewrites a relative link to a Markdown page so it points at the page's
/// HTML output. Absolute URLs, fragments and other files are unchanged.
pub fn rewrite_link(url: &str) -> String {
    let is_external = url.starts_with("//")
        || url.starts_with('#')
        || url.split_once(':').is_some_and(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        });
    if is_external {
        return url.to_owned();
    }
    let split = url.find(['#', '?']).unwrap_or(url.len());
    let (path, rest) = url.split_at(split);
    for ext in [".md", ".markdown"] {
        if let Some(stem) = path.strip_suffix(ext)
            && !stem.is_empty()
        {
            return format!("{stem}.html{rest}");
        }
    }
    url.to_owned()
}

fn children(opts: &Page<'_>, nodes: &[Node], out: &mut String) {
    let mut i = 0;
    while i < nodes.len() {
        // Consecutive footnote definitions share one `lsd-footnotes` block.
        if matches!(nodes[i], Node::FootnoteDefinition(_)) {
            let end = nodes[i..]
                .iter()
                .position(|n| !matches!(n, Node::FootnoteDefinition(_)))
                .map_or(nodes.len(), |n| i + n);
            out.push_str("<div class=\"lsd-footnotes\" role=\"doc-endnotes\">\n");
            for node in &nodes[i..end] {
                render(opts, node, out);
            }
            out.push_str("</div>\n");
            i = end;
        } else {
            render(opts, &nodes[i], out);
            i += 1;
        }
    }
}

fn wrap(opts: &Page<'_>, tag: &str, attrs: &str, nodes: &[Node], out: &mut String, newline: bool) {
    out.push_str(&format!("<{tag}{attrs}>"));
    children(opts, nodes, out);
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

fn title_attr(title: &Option<String>) -> String {
    title
        .as_ref()
        .map(|t| format!(" title=\"{}\"", escape(t)))
        .unwrap_or_default()
}

/// A heading; `anchor` is the id its `#` link points to (its section's id
/// for a section title).
fn heading(opts: &Page<'_>, n: &Heading, anchor: Option<&str>, out: &mut String) {
    let level = n.level.clamp(1, 6);
    out.push_str(&format!("<h{level}{}>", id_attr(&n.id)));
    children(opts, &n.children, out);
    if opts.options.heading_anchors
        && let Some(id) = anchor.or(n.id.as_deref())
    {
        out.push_str(&format!(
            " <a class=\"lsd-anchor\" href=\"#{}\" aria-label=\"Link to this section\">#</a>",
            escape(id)
        ));
    }
    out.push_str(&format!("</h{level}>\n"));
}

/// A code block, or a `custom:highlight` around one, with a caption
/// after it: the pair `code_figure` writes as one figure.
fn code_with_caption(nodes: &[Node]) -> Option<(&Node, &Caption)> {
    match nodes {
        [code, Node::Caption(caption)] if is_code(code) => Some((code, caption)),
        _ => None,
    }
}

fn is_code(node: &Node) -> bool {
    match node {
        Node::CodeBlock(_) => true,
        Node::Custom(n) => {
            n.kind == "highlight" && n.attrs.get("html").is_some_and(|h| h.is_string())
        }
        _ => false,
    }
}

/// A code block's info string (after the language) as options: words
/// separated by white space, each `key` or `key=value`, where the value
/// may be quoted (`title="a b.rs"`, or with `'`) to hold white space.
fn info_options(info: &str) -> Vec<(String, Option<String>)> {
    let mut options = Vec::new();
    let mut chars = info.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut key = String::new();
        while let Some(c) = chars.next_if(|c| !c.is_whitespace() && *c != '=') {
            key.push(c);
        }
        let value = if chars.next_if_eq(&'=').is_some() {
            let mut value = String::new();
            match chars.next_if(|c| *c == '"' || *c == '\'') {
                Some(quote) => {
                    for c in chars.by_ref() {
                        if c == quote {
                            break;
                        }
                        value.push(c);
                    }
                }
                None => {
                    while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
                        value.push(c);
                    }
                }
            }
            Some(value)
        } else {
            None
        };
        if key.is_empty() && value.is_none() {
            return options;
        }
        options.push((key, value));
    }
}

/// A code block (`code`: a code block, or a `custom:highlight` with its
/// HTML) as `<lsd-copy><figure class="lsd-code">`, with the language,
/// line numbers and emphasized lines as data, and its caption and title
/// (`title=` in its info), if any, as a `figcaption.lsd-code-title`
/// before the `<pre>`.
fn code_figure(
    opts: &Page<'_>,
    id: Option<&str>,
    code: &Node,
    caption: Option<&Caption>,
    out: &mut String,
) {
    let (block, highlighted) = match code {
        Node::Custom(n) => (
            n.children.iter().find_map(|c| match c {
                Node::CodeBlock(b) => Some(b),
                _ => None,
            }),
            n.attrs.get("html").and_then(serde_json::Value::as_str),
        ),
        Node::CodeBlock(b) => (Some(b), None),
        _ => (None, None),
    };
    let language = block
        .and_then(|b| b.language.as_deref())
        .or_else(|| match code {
            Node::Custom(n) => n.attrs.get("language").and_then(serde_json::Value::as_str),
            _ => None,
        });
    let mut attrs = String::from(" class=\"lsd-code\"");
    if let Some(id) = id {
        attrs.push_str(&format!(" id=\"{}\"", escape(id)));
    }
    if let Some(language) = language {
        attrs.push_str(&format!(" data-language=\"{}\"", escape(language)));
    }
    // The `info` of a code block: `linenos`, `lineno-start=N`,
    // `emphasize-lines=SPEC` and `title=TEXT` (Sphinx's options), or
    // MkDocs' `linenums=N` and `hl_lines="1 3-4"` (and anything else a
    // parser leaves there, which is ignored).
    let mut title = None;
    for (key, value) in info_options(block.and_then(|b| b.info.as_deref()).unwrap_or_default()) {
        match (key.as_str(), value) {
            ("linenos", None) => attrs.push_str(" data-linenos"),
            ("lineno-start", Some(n)) => {
                attrs.push_str(&format!(" data-line-start=\"{}\"", escape(&n)));
            }
            ("linenums", Some(n)) => {
                attrs.push_str(&format!(" data-linenos data-line-start=\"{}\"", escape(&n)));
            }
            ("emphasize-lines", Some(spec)) => {
                attrs.push_str(&format!(" data-lines=\"{}\"", escape(&spec)));
            }
            ("hl_lines", Some(spec)) => {
                let spec = spec.split_whitespace().collect::<Vec<_>>().join(",");
                attrs.push_str(&format!(" data-lines=\"{}\"", escape(&spec)));
            }
            ("title", Some(text)) if !text.is_empty() => title = Some(text),
            _ => {}
        }
    }
    out.push_str(&format!("<lsd-copy><figure{attrs}>"));
    // The title bar: the caption, or the `title=` text; with both, the
    // title in `strong` and the caption in a paragraph under it.
    match (title, caption) {
        (None, None) => {}
        (Some(title), None) => out.push_str(&format!(
            "<figcaption class=\"lsd-code-title\">{}</figcaption>",
            escape(&title)
        )),
        (None, Some(caption)) => wrap(
            opts,
            "figcaption",
            " class=\"lsd-code-title\"",
            &caption.children,
            out,
            false,
        ),
        (Some(title), Some(caption)) => {
            out.push_str(&format!(
                "<figcaption class=\"lsd-code-title\"><strong>{}</strong>",
                escape(&title)
            ));
            wrap(opts, "p", "", &caption.children, out, false);
            out.push_str("</figcaption>");
        }
    }
    match (highlighted, block) {
        (Some(html), _) => out.push_str(html.trim_end_matches('\n')),
        (None, Some(block)) => {
            let class = block
                .language
                .as_ref()
                .map(|l| format!(" class=\"language-{}\"", escape(l)))
                .unwrap_or_default();
            out.push_str(&format!(
                "<pre><code{class}>{}</code></pre>",
                escape(&block.value)
            ));
        }
        (None, None) => {}
    }
    out.push_str("</figure></lsd-copy>\n");
}

fn render(opts: &Page<'_>, node: &Node, out: &mut String) {
    match node {
        Node::Section(n) => {
            out.push_str(&format!("<section{}>\n", id_attr(&n.id)));
            let rest = match n.children.split_first() {
                Some((Node::Heading(h), rest)) => {
                    heading(opts, h, n.id.as_deref(), out);
                    rest
                }
                _ => &n.children[..],
            };
            children(opts, rest, out);
            out.push_str("</section>\n");
        }
        Node::Heading(n) => heading(opts, n, None, out),
        Node::Paragraph(n) => wrap(opts, "p", "", &n.children, out, true),
        Node::List(n) => {
            let (tag, attrs) = match (n.ordered, n.start) {
                (true, Some(start)) => ("ol", format!(" start=\"{start}\"")),
                (true, None) => ("ol", String::new()),
                (false, _) => ("ul", String::new()),
            };
            out.push_str(&format!("<{tag}{attrs}>\n"));
            children(opts, &n.children, out);
            out.push_str(&format!("</{tag}>\n"));
        }
        Node::ListItem(n) => {
            out.push_str("<li>");
            if let Some(checked) = n.checked {
                let mark = if checked { " checked" } else { "" };
                out.push_str(&format!("<input type=\"checkbox\" disabled{mark}> "));
            }
            children(opts, &n.children, out);
            out.push_str("</li>\n");
        }
        Node::BlockQuote(n) => {
            out.push_str("<blockquote>\n");
            children(opts, &n.children, out);
            out.push_str("</blockquote>\n");
        }
        Node::CodeBlock(_) => code_figure(opts, None, node, None, out),
        Node::Table(n) => {
            out.push_str("<div class=\"lsd-table\">\n<table>\n");
            for row in &n.children {
                let Node::TableRow(row) = row else {
                    render(opts, row, out);
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
                    wrap(opts, cell_tag, style, cell.children(), out, false);
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</table>\n</div>\n");
        }
        Node::TableRow(n) => wrap(opts, "tr", "", &n.children, out, true),
        Node::TableCell(n) => wrap(opts, "td", "", &n.children, out, false),
        Node::ThematicBreak(_) => out.push_str("<hr>\n"),
        Node::Raw(n) => {
            if n.format == "html" {
                out.push_str(&n.value);
            }
        }
        Node::Text(n) => out.push_str(&escape(&n.value)),
        Node::Emphasis(n) => wrap(opts, "em", "", &n.children, out, false),
        Node::Strong(n) => wrap(opts, "strong", "", &n.children, out, false),
        Node::InlineCode(n) => out.push_str(&format!("<code>{}</code>", escape(&n.value))),
        Node::Link(n) => {
            let attrs = format!(
                " href=\"{}\"{}",
                escape(&link_href(opts.source, n)),
                title_attr(&n.title)
            );
            wrap(opts, "a", &attrs, &n.children, out, false);
        }
        Node::Image(n) => out.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\"{}>",
            escape(&n.url),
            escape(&n.alt),
            title_attr(&n.title)
        )),
        Node::LineBreak(_) => out.push_str("<br>\n"),
        Node::Admonition(n) => {
            let title = n.title.clone().unwrap_or_else(|| capitalize(&n.kind));
            out.push_str(&format!(
                "<div class=\"lsd-admonition\" data-kind=\"{}\">\n<p class=\"lsd-admonition-title\">{}</p>\n",
                escape(&n.kind),
                escape(&title)
            ));
            children(opts, &n.children, out);
            out.push_str("</div>\n");
        }
        Node::DefinitionList(n) => {
            out.push_str("<dl class=\"lsd-definition-list\">\n");
            children(opts, &n.children, out);
            out.push_str("</dl>\n");
        }
        Node::DefinitionTerm(n) => wrap(opts, "dt", "", &n.children, out, true),
        Node::DefinitionDescription(n) => wrap(opts, "dd", "", &n.children, out, true),
        Node::FootnoteReference(n) => {
            let label = escape(&n.label);
            out.push_str(&format!(
                "<sup class=\"lsd-footnote-ref\"><a href=\"#fn-{label}\" id=\"fnref-{label}\">{label}</a></sup>"
            ));
        }
        Node::FootnoteDefinition(n) => {
            let label = escape(&n.label);
            out.push_str(&format!(
                "<div class=\"lsd-footnote\" id=\"fn-{label}\">\n<p class=\"lsd-footnote-label\"><a href=\"#fnref-{label}\">{label}</a></p>\n"
            ));
            children(opts, &n.children, out);
            out.push_str("</div>\n");
        }
        // Delimited for client-side typesetting (KaTeX or MathJax). A span
        // even for display math, which may sit inside a paragraph.
        Node::Math(n) if n.display => out.push_str(&format!(
            "<span class=\"lsd-math lsd-math-display\">\\[{}\\]</span>",
            escape(&n.value)
        )),
        Node::Math(n) => out.push_str(&format!(
            "<span class=\"lsd-math lsd-math-inline\">\\({}\\)</span>",
            escape(&n.value)
        )),
        // A code block with a caption is one `lsd-code` figure.
        Node::Figure(n) if code_with_caption(&n.children).is_some() => {
            if let Some((code, caption)) = code_with_caption(&n.children) {
                code_figure(opts, n.id.as_deref(), code, Some(caption), out);
            }
        }
        Node::Figure(n) => {
            out.push_str(&format!(
                "<figure class=\"lsd-figure\"{}>\n",
                id_attr(&n.id)
            ));
            children(opts, &n.children, out);
            out.push_str("</figure>\n");
        }
        Node::Caption(n) => wrap(opts, "figcaption", "", &n.children, out, true),
        Node::Anchor(n) => out.push_str(&format!("<span id=\"{}\"></span>", escape(&n.id))),
        // Highlighted code from `transform-highlight`: its HTML as is.
        Node::Custom(n)
            if n.kind == "highlight" && n.attrs.get("html").is_some_and(|h| h.is_string()) =>
        {
            code_figure(opts, None, node, None, out);
        }
        Node::Custom(n) if n.kind == "tab-set" => {
            out.push_str("<lsd-tabs class=\"lsd-tabs\">\n");
            children(opts, &n.children, out);
            out.push_str("</lsd-tabs>\n");
        }
        Node::Custom(n) if n.kind == "tab-item" => {
            let label = escape(
                n.attrs
                    .get("label")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
            );
            out.push_str(&format!(
                "<div class=\"lsd-tab\" data-label=\"{label}\" role=\"group\" aria-label=\"{label}\">\n"
            ));
            children(opts, &n.children, out);
            out.push_str("</div>\n");
        }
        // An API item (from the API documentation framework): a block
        // with its kind and the scope it documents as data.
        Node::Custom(n) if n.kind == "api-item" => {
            let mut attrs = String::from(" class=\"lsd-api-item\"");
            for (name, key) in [("data-kind", "item_kind"), ("data-scope", "scope")] {
                if let Some(v) = n.attrs.get(key).and_then(serde_json::Value::as_str) {
                    attrs.push_str(&format!(" {name}=\"{}\"", escape(v)));
                }
            }
            if let Some(id) = n.attrs.get("id").and_then(serde_json::Value::as_str) {
                attrs.push_str(&format!(" id=\"{}\"", escape(id)));
            }
            wrap(opts, "div", &attrs, &n.children, out, true);
        }
        // An inline custom node: a span, or one of a few inline elements.
        Node::Custom(n) if n.attrs.get("inline") == Some(&serde_json::Value::Bool(true)) => {
            let tag = n
                .attrs
                .get("html_tag")
                .and_then(serde_json::Value::as_str)
                .filter(|t| INLINE_TAGS.contains(t))
                .unwrap_or("span");
            let mut attrs = format!(" class=\"lsd-{}\"", escape(&n.kind));
            if let Some(title) = n.attrs.get("title").and_then(serde_json::Value::as_str) {
                attrs.push_str(&format!(" title=\"{}\"", escape(title)));
            }
            wrap(opts, tag, &attrs, &n.children, out, false);
        }
        // A block with text `content` that no plugin turned into anything
        // else (a directive nobody handles): a `div` holding a warning
        // with its `argument` and `options` (as JSON) and its content as
        // code blocks, then the children, so none of it is lost.
        Node::Custom(n)
            if n.attrs
                .get("content")
                .is_some_and(serde_json::Value::is_string) =>
        {
            let block = |title: &str, language: Option<&str>, value: String| {
                Node::CodeBlock(CodeBlock {
                    language: language.map(str::to_owned),
                    info: Some(format!("title={title}")),
                    value,
                    ..CodeBlock::default()
                })
            };
            let mut blocks = Vec::new();
            for key in ["argument", "options"] {
                if let Some(value) = n.attrs.get(key) {
                    let json = serde_json::to_string_pretty(value).unwrap_or_default();
                    blocks.push(block(key, Some("json"), json));
                }
            }
            if let Some(content) = n.attrs.get("content").and_then(serde_json::Value::as_str) {
                blocks.push(block("content", None, content.to_owned()));
            }
            out.push_str(&format!("<div{}>\n", custom_attrs(n)));
            let warning = Node::Admonition(Admonition {
                kind: "warning".into(),
                title: Some(format!("Unknown doc tree node \"{}\"", n.kind)),
                children: blocks,
                ..Admonition::default()
            });
            render(opts, &warning, out);
            children(opts, &n.children, out);
            out.push_str("</div>\n");
        }
        Node::Custom(n) => {
            let tag = n
                .attrs
                .get("html_tag")
                .and_then(serde_json::Value::as_str)
                .filter(|t| BLOCK_TAGS.contains(t))
                .unwrap_or("div");
            wrap(opts, tag, &custom_attrs(n), &n.children, out, true);
        }
    }
}

/// A block `custom` node's attributes: `lsd-KIND` as its class, and its
/// `id`, if any.
fn custom_attrs(n: &Custom) -> String {
    let mut attrs = format!(" class=\"lsd-{}\"", escape(&n.kind));
    if let Some(id) = n.attrs.get("id").and_then(serde_json::Value::as_str) {
        attrs.push_str(&format!(" id=\"{}\"", escape(id)));
    }
    attrs
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use lsdocs_plugin_sdk::doctree::{Paragraph, text};

    fn render_nodes(nodes: Vec<Node>) -> String {
        let mut doc = Document::new("a.md");
        doc.children = nodes;
        body(&doc, &Options::default())
    }

    #[test]
    fn admonitions_carry_kind_and_title() {
        use lsdocs_plugin_sdk::doctree::Admonition;
        let html = render_nodes(vec![Node::Admonition(Admonition {
            kind: "warning".into(),
            children: vec![Node::Paragraph(Paragraph {
                children: vec![text("Careful")],
                ..Paragraph::default()
            })],
            ..Admonition::default()
        })]);
        assert!(
            html.contains("<div class=\"lsd-admonition\" data-kind=\"warning\">"),
            "{html}"
        );
        assert!(
            html.contains("<p class=\"lsd-admonition-title\">Warning</p>"),
            "{html}"
        );
    }

    #[test]
    fn code_blocks_are_figures() {
        use lsdocs_plugin_sdk::doctree::CodeBlock;
        let html = render_nodes(vec![Node::CodeBlock(CodeBlock {
            language: Some("rust".into()),
            value: "a < b".into(),
            ..CodeBlock::default()
        })]);
        assert_eq!(
            html,
            "<lsd-copy><figure class=\"lsd-code\" data-language=\"rust\">\
             <pre><code class=\"language-rust\">a &lt; b</code></pre></figure></lsd-copy>\n"
        );
    }

    #[test]
    fn a_title_and_a_caption_share_the_title_bar() {
        use lsdocs_plugin_sdk::doctree::{CodeBlock, Figure};
        let html = render_nodes(vec![Node::Figure(Figure {
            children: vec![
                Node::CodeBlock(CodeBlock {
                    info: Some("title=a.py".into()),
                    value: "a".into(),
                    ..CodeBlock::default()
                }),
                Node::Caption(Caption {
                    children: vec![text("Example")],
                    ..Caption::default()
                }),
            ],
            ..Figure::default()
        })]);
        assert_eq!(
            html,
            "<lsd-copy><figure class=\"lsd-code\"><figcaption class=\"lsd-code-title\">\
             <strong>a.py</strong><p>Example</p></figcaption>\
             <pre><code>a</code></pre></figure></lsd-copy>\n"
        );
    }

    #[test]
    fn info_options_take_quoted_values() {
        let options = info_options(" linenos title=\"a b.rs\"  hl_lines='1 3' x=1 =y ");
        let options: Vec<_> = options
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_deref()))
            .collect();
        assert_eq!(
            options,
            [
                ("linenos", None),
                ("title", Some("a b.rs")),
                ("hl_lines", Some("1 3")),
                ("x", Some("1")),
                ("", Some("y")),
            ]
        );
        assert!(info_options("").is_empty());
        assert_eq!(info_options("t=\"open").len(), 1);
    }

    #[test]
    fn titles_and_mkdocs_options_on_the_code_figure() {
        use lsdocs_plugin_sdk::doctree::CodeBlock;
        let html = render_nodes(vec![Node::CodeBlock(CodeBlock {
            language: Some("rust".into()),
            info: Some("title=\"src/<a>.rs\" linenums=\"3\" hl_lines=\"1 2-3\"".into()),
            value: "a".into(),
            ..CodeBlock::default()
        })]);
        assert_eq!(
            html,
            "<lsd-copy><figure class=\"lsd-code\" data-language=\"rust\" \
             data-linenos data-line-start=\"3\" data-lines=\"1,2-3\">\
             <figcaption class=\"lsd-code-title\">src/&lt;a&gt;.rs</figcaption>\
             <pre><code class=\"language-rust\">a</code></pre></figure></lsd-copy>\n"
        );
    }

    #[test]
    fn headings_link_to_themselves() {
        let html = body(&doc("a.md"), &Options::default());
        assert!(html.contains("class=\"lsd-anchor\""), "{html}");
    }

    #[test]
    fn captions_and_lines_are_data_on_the_code_figure() {
        use lsdocs_plugin_sdk::doctree::{CodeBlock, Figure};
        let html = render_nodes(vec![Node::Figure(Figure {
            id: Some("ex".into()),
            children: vec![
                Node::CodeBlock(CodeBlock {
                    language: Some("python".into()),
                    info: Some("linenos lineno-start=5 emphasize-lines=1,3-4".into()),
                    value: "a = 1".into(),
                    ..CodeBlock::default()
                }),
                Node::Caption(Caption {
                    children: vec![text("Example")],
                    ..Caption::default()
                }),
            ],
            ..Figure::default()
        })]);
        assert_eq!(
            html,
            "<lsd-copy><figure class=\"lsd-code\" id=\"ex\" data-language=\"python\" \
             data-linenos data-line-start=\"5\" data-lines=\"1,3-4\">\
             <figcaption class=\"lsd-code-title\">Example</figcaption>\
             <pre><code class=\"language-python\">a = 1</code></pre></figure></lsd-copy>\n"
        );
    }

    #[test]
    fn tabs_are_an_element_of_labelled_panels() {
        use lsdocs_plugin_sdk::doctree::Custom;
        let item = |label: &str, body: &str| {
            Node::Custom(Custom {
                kind: "tab-item".into(),
                attrs: [("label".to_owned(), serde_json::json!(label))].into(),
                children: vec![Node::Paragraph(Paragraph {
                    children: vec![text(body)],
                    ..Paragraph::default()
                })],
                ..Custom::default()
            })
        };
        let html = render_nodes(vec![Node::Custom(Custom {
            kind: "tab-set".into(),
            children: vec![item("Rust", "One"), item("C & C++", "Two")],
            ..Custom::default()
        })]);
        assert_eq!(
            html,
            "<lsd-tabs class=\"lsd-tabs\">\n\
             <div class=\"lsd-tab\" data-label=\"Rust\" role=\"group\" aria-label=\"Rust\">\n\
             <p>One</p>\n</div>\n\
             <div class=\"lsd-tab\" data-label=\"C &amp; C++\" role=\"group\" aria-label=\"C &amp; C++\">\n\
             <p>Two</p>\n</div>\n\
             </lsd-tabs>\n"
        );
    }

    #[test]
    fn tables_api_items_and_custom_kinds() {
        use lsdocs_plugin_sdk::doctree::{Custom, Table, TableRow};
        let cell = |t: &str| {
            Node::TableCell(lsdocs_plugin_sdk::doctree::TableCell {
                children: vec![text(t)],
                ..Default::default()
            })
        };
        let html = render_nodes(vec![
            Node::Table(Table {
                children: vec![Node::TableRow(TableRow {
                    header: true,
                    children: vec![cell("h")],
                    ..TableRow::default()
                })],
                ..Table::default()
            }),
            Node::Custom(Custom {
                kind: "api-item".into(),
                attrs: [
                    ("item_kind".to_owned(), serde_json::json!("function")),
                    ("scope".to_owned(), serde_json::json!("pkg.f")),
                ]
                .into(),
                ..Custom::default()
            }),
            Node::Custom(Custom {
                kind: "toc".into(),
                ..Custom::default()
            }),
        ]);
        assert_eq!(
            html,
            "<div class=\"lsd-table\">\n<table>\n<tr><th>h</th></tr>\n</table>\n</div>\n\
             <div class=\"lsd-api-item\" data-kind=\"function\" data-scope=\"pkg.f\"></div>\n\
             <div class=\"lsd-toc\"></div>\n"
        );
    }

    /// A node nobody handles keeps its class and id, and shows its
    /// argument and options (as JSON) and its content in a warning.
    #[test]
    fn unknown_nodes_show_their_content_in_a_warning() {
        let html = render_nodes(vec![Node::Custom(Custom {
            kind: "example".into(),
            attrs: [
                ("argument".to_owned(), serde_json::json!("one two")),
                ("options".to_owned(), serde_json::json!({"flag": true})),
                ("id".to_owned(), serde_json::json!("x")),
                ("content".to_owned(), serde_json::json!("a < b")),
            ]
            .into(),
            ..Custom::default()
        })]);
        assert_eq!(
            html,
            "<div class=\"lsd-example\" id=\"x\">\n\
             <div class=\"lsd-admonition\" data-kind=\"warning\">\n\
             <p class=\"lsd-admonition-title\">Unknown doc tree node &quot;example&quot;</p>\n\
             <lsd-copy><figure class=\"lsd-code\" data-language=\"json\">\
             <figcaption class=\"lsd-code-title\">argument</figcaption>\
             <pre><code class=\"language-json\">&quot;one two&quot;</code></pre></figure></lsd-copy>\n\
             <lsd-copy><figure class=\"lsd-code\" data-language=\"json\">\
             <figcaption class=\"lsd-code-title\">options</figcaption>\
             <pre><code class=\"language-json\">{\n  &quot;flag&quot;: true\n}</code></pre>\
             </figure></lsd-copy>\n\
             <lsd-copy><figure class=\"lsd-code\">\
             <figcaption class=\"lsd-code-title\">content</figcaption>\
             <pre><code>a &lt; b</code></pre></figure></lsd-copy>\n\
             </div>\n</div>\n"
        );
    }

    /// Preformatted text with markup in it (a grammar, say): `pre` and
    /// `code`, as a code block, spaces and newlines kept.
    #[test]
    fn custom_preformatted_text() {
        use lsdocs_plugin_sdk::doctree::{Custom, Link, Strong};
        let code = Node::Custom(Custom {
            kind: "productions".into(),
            attrs: [
                ("inline".to_owned(), serde_json::json!(true)),
                ("html_tag".to_owned(), serde_json::json!("code")),
            ]
            .into(),
            children: vec![
                Node::Strong(Strong {
                    children: vec![text("a")],
                    ..Strong::default()
                }),
                text("   ::= "),
                Node::Link(Link {
                    url: "#b".into(),
                    children: vec![text("b")],
                    ..Link::default()
                }),
                text("\n  | <c>"),
            ],
            ..Custom::default()
        });
        let html = render_nodes(vec![Node::Custom(Custom {
            kind: "productionlist".into(),
            attrs: [("html_tag".to_owned(), serde_json::json!("pre"))].into(),
            children: vec![code],
            ..Custom::default()
        })]);
        assert_eq!(
            html,
            "<pre class=\"lsd-productionlist\"><code class=\"lsd-productions\">\
             <strong>a</strong>   ::= <a href=\"#b\">b</a>\n  | &lt;c&gt;</code></pre>\n"
        );
    }

    fn doc(source: &str) -> Document {
        let mut doc = Document::new(source);
        doc.children = vec![Node::Heading(Heading {
            level: 1,
            id: Some("t".into()),
            children: vec![text("Tom & Jerry")],
            ..Heading::default()
        })];
        doc
    }

    #[test]
    fn renders_the_shared_block_kinds() {
        use lsdocs_plugin_sdk::doctree::{
            Admonition, Anchor, Caption, DefinitionDescription, DefinitionList, DefinitionTerm,
            Figure, FootnoteDefinition, FootnoteReference, Image, Math,
        };
        let para = |nodes: Vec<Node>| {
            Node::Paragraph(Paragraph {
                children: nodes,
                ..Paragraph::default()
            })
        };
        let mut doc = Document::new("a.md");
        doc.children = vec![
            Node::Admonition(Admonition {
                kind: "warning".into(),
                children: vec![para(vec![text("Careful")])],
                ..Admonition::default()
            }),
            Node::DefinitionList(DefinitionList {
                children: vec![
                    Node::DefinitionTerm(DefinitionTerm {
                        children: vec![text("Term")],
                        ..DefinitionTerm::default()
                    }),
                    Node::DefinitionDescription(DefinitionDescription {
                        children: vec![para(vec![text("Meaning")])],
                        ..DefinitionDescription::default()
                    }),
                ],
                ..DefinitionList::default()
            }),
            para(vec![
                Node::Anchor(Anchor {
                    id: "here".into(),
                    ..Anchor::default()
                }),
                Node::Math(Math {
                    value: "a<b".into(),
                    ..Math::default()
                }),
                Node::FootnoteReference(FootnoteReference {
                    label: "1".into(),
                    ..FootnoteReference::default()
                }),
            ]),
            Node::Math(Math {
                value: "x^2".into(),
                display: true,
                ..Math::default()
            }),
            Node::Figure(Figure {
                id: Some("fig".into()),
                children: vec![
                    Node::Image(Image {
                        url: "a.png".into(),
                        alt: "A".into(),
                        ..Image::default()
                    }),
                    Node::Caption(Caption {
                        children: vec![text("An A")],
                        ..Caption::default()
                    }),
                ],
                ..Figure::default()
            }),
            Node::FootnoteDefinition(FootnoteDefinition {
                label: "1".into(),
                children: vec![para(vec![text("Note.")])],
                ..FootnoteDefinition::default()
            }),
        ];
        let expected = [
            "<div class=\"lsd-admonition\" data-kind=\"warning\">",
            "<p class=\"lsd-admonition-title\">Warning</p>",
            "<p>Careful</p>",
            "</div>",
            "<dl class=\"lsd-definition-list\">",
            "<dt>Term</dt>",
            "<dd><p>Meaning</p>",
            "</dd>",
            "</dl>",
            "<p><span id=\"here\"></span><span class=\"lsd-math lsd-math-inline\">\\(a&lt;b\\)</span>\
             <sup class=\"lsd-footnote-ref\"><a href=\"#fn-1\" id=\"fnref-1\">1</a></sup></p>",
            "<span class=\"lsd-math lsd-math-display\">\\[x^2\\]</span>\
             <figure class=\"lsd-figure\" id=\"fig\">",
            "<img src=\"a.png\" alt=\"A\"><figcaption>An A</figcaption>",
            "</figure>",
            "<div class=\"lsd-footnotes\" role=\"doc-endnotes\">",
            "<div class=\"lsd-footnote\" id=\"fn-1\">",
            "<p class=\"lsd-footnote-label\"><a href=\"#fnref-1\">1</a></p>",
            "<p>Note.</p>",
            "</div>",
            "</div>",
            "",
        ]
        .join("\n");
        assert_eq!(body(&doc, &Options::default()), expected);
    }

    #[test]
    fn footnotes_inside_sections_are_grouped() {
        use lsdocs_plugin_sdk::doctree::{FootnoteDefinition, Section};
        let def = |label: &str| {
            Node::FootnoteDefinition(FootnoteDefinition {
                label: label.into(),
                ..FootnoteDefinition::default()
            })
        };
        let html = render_nodes(vec![Node::Section(Section {
            id: Some("s".into()),
            children: vec![
                Node::Heading(Heading {
                    level: 1,
                    children: vec![text("S")],
                    ..Heading::default()
                }),
                def("1"),
                def("2"),
            ],
            ..Section::default()
        })]);
        assert_eq!(
            html.matches("<div class=\"lsd-footnotes\"").count(),
            1,
            "{html}"
        );
        let open = html.find("<div class=\"lsd-footnotes\"").unwrap();
        assert!(html[open..].contains("id=\"fn-1\""), "{html}");
        assert!(html[open..].contains("id=\"fn-2\""), "{html}");
    }

    #[test]
    fn sections_and_link_targets() {
        use lsdocs_plugin_sdk::doctree::Section;
        let target = |document: Option<&str>, anchor: Option<&str>| {
            Some(LinkTarget {
                document: document.map(Into::into),
                anchor: anchor.map(Into::into),
                label: None,
            })
        };
        let link = |url: &str, target: Option<LinkTarget>| Link {
            url: url.into(),
            target,
            ..Link::default()
        };
        let from = "guide/a.md";
        assert_eq!(
            link_href(from, &link("x", target(Some("ref/b.rst"), Some("s")))),
            "../ref/b.html#s"
        );
        assert_eq!(link_href(from, &link("x", target(None, Some("s")))), "#s");
        assert_eq!(
            link_href(from, &link("x", target(Some(from), Some("s")))),
            "#s"
        );
        assert_eq!(
            link_href(from, &link("x", target(Some(from), None))),
            "a.html"
        );
        // No target, or an unresolved label: the URL, `.md` rewritten.
        assert_eq!(link_href(from, &link("c.md#t", None)), "c.html#t");
        let label = Some(LinkTarget {
            label: Some("l".into()),
            ..LinkTarget::default()
        });
        assert_eq!(link_href(from, &link("c.md", label)), "c.html");

        let mut doc = Document::new(from);
        doc.children = vec![Node::Section(Section {
            id: Some("intro".into()),
            children: vec![
                Node::Heading(Heading {
                    level: 1,
                    children: vec![text("Intro")],
                    ..Heading::default()
                }),
                Node::Paragraph(Paragraph {
                    children: vec![text("Hi")],
                    ..Paragraph::default()
                }),
            ],
            ..Section::default()
        })];
        assert_eq!(
            body(&doc, &Options::default()),
            "<section id=\"intro\">\n<h1>Intro <a class=\"lsd-anchor\" href=\"#intro\" \
             aria-label=\"Link to this section\">#</a></h1>\n<p>Hi</p>\n</section>\n"
        );
    }

    #[test]
    fn highlighted_code_is_written_as_is() {
        use lsdocs_plugin_sdk::doctree::{CodeBlock, Custom};
        let mut doc = Document::new("a.md");
        let code = Node::CodeBlock(CodeBlock {
            value: "x".into(),
            ..CodeBlock::default()
        });
        doc.children = vec![
            Node::Custom(Custom {
                kind: "highlight".into(),
                attrs: [("html".to_owned(), serde_json::json!("<pre>X</pre>\n"))].into(),
                children: vec![code.clone()],
                ..Custom::default()
            }),
            // Without its HTML, the children are rendered instead.
            Node::Custom(Custom {
                kind: "highlight".into(),
                children: vec![code],
                ..Custom::default()
            }),
        ];
        assert_eq!(
            body(&doc, &Options::default()),
            "<lsd-copy><figure class=\"lsd-code\"><pre>X</pre></figure></lsd-copy>\n\
             <div class=\"lsd-highlight\"><lsd-copy><figure class=\"lsd-code\">\
             <pre><code>x</code></pre></figure></lsd-copy>\n</div>\n"
        );
    }

    #[test]
    fn heading_anchors_can_be_turned_off() {
        let options = Options::from_value(&serde_json::json!({"heading_anchors": false}));
        assert_eq!(
            body(&doc("a.md"), &options).lines().next(),
            Some("<h1 id=\"t\">Tom &amp; Jerry</h1>")
        );
    }

    #[test]
    fn link_rewriting() {
        assert_eq!(rewrite_link("guide.md"), "guide.html");
        assert_eq!(rewrite_link("../a/b.markdown#x"), "../a/b.html#x");
        assert_eq!(rewrite_link("page.md?q=1"), "page.html?q=1");
        assert_eq!(
            rewrite_link("https://example.com/x.md"),
            "https://example.com/x.md"
        );
        assert_eq!(
            rewrite_link("//cdn.example.com/x.md"),
            "//cdn.example.com/x.md"
        );
        assert_eq!(
            rewrite_link("mailto:me@example.com"),
            "mailto:me@example.com"
        );
        assert_eq!(rewrite_link("#section"), "#section");
        assert_eq!(rewrite_link("image.png"), "image.png");
        assert_eq!(rewrite_link(".md"), ".md");
    }
}
