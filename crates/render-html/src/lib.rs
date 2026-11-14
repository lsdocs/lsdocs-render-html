//! First-party HTML renderer plugin.
//!
//! Renders each document's body to HTML, then places it in a page template
//! (minijinja). The built-in theme is used unless the plugin's `template`
//! option names a file, relative to the site directory:
//!
//! ```toml
//! [[plugins]]
//! name = "html"
//! builtin = "render-html"
//! options = { template = "theme/page.html" }
//! ```
//!
//! Templates see:
//!
//! | Variable | Value |
//! |---|---|
//! | `content` | The rendered body (already HTML). |
//! | `page.title` | `title` front matter, else the first heading, else the source path. |
//! | `page.source` | The source path, e.g. `guide/intro.md`. |
//! | `page.metadata` | All front matter. |
//! | `page.root` | Relative path from this page to the site root, e.g. `../`. |
//! | `site.title`, `site.base_url` | From the site config. |
//!
//! Relative links to other Markdown pages (`guide.md#setup`) are rewritten
//! to their HTML output (`guide.html#setup`).

use std::path::Path;
use std::sync::OnceLock;

use lsdocs_plugin_sdk::doctree::{Align, Document, Node};
use lsdocs_plugin_sdk::protocol::{RenderParams, RenderResult, SiteInfo};
use lsdocs_plugin_sdk::{Context, Plugin, PluginError, PluginResult, Renderer};
use minijinja::{Environment, Value, context};

/// The built-in page template.
pub const DEFAULT_TEMPLATE: &str = include_str!("../theme/page.html");

const TEMPLATE_NAME: &str = "page.html";

/// The plugin, as registered with the host.
pub fn plugin() -> Plugin {
    Plugin::new("render-html", env!("CARGO_PKG_VERSION")).renderer(Html::default())
}

/// The HTML renderer. The template is loaded on first use, since its path
/// comes from the plugin's options.
#[derive(Debug, Default)]
pub struct Html {
    env: OnceLock<Result<Environment<'static>, String>>,
}

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
        let env = self
            .env
            .get_or_init(|| {
                environment(
                    ctx.options().get("template").and_then(|t| t.as_str()),
                    ctx.site(),
                )
            })
            .as_ref()
            .map_err(|e| PluginError::new(e.clone()))?;
        Ok(RenderResult {
            output: render_page(env, &params.document, ctx.site())?.into_bytes(),
            extension: "html".into(),
            diagnostics: vec![],
        })
    }
}

/// Builds the template environment from the built-in template, or from
/// `template` (relative to the site root) if given.
fn environment(template: Option<&str>, site: &SiteInfo) -> Result<Environment<'static>, String> {
    let source = match template {
        None => DEFAULT_TEMPLATE.to_owned(),
        Some(path) => {
            let full = match &site.root {
                Some(root) => Path::new(root).join(path),
                None => Path::new(path).to_path_buf(),
            };
            std::fs::read_to_string(&full)
                .map_err(|e| format!("failed to read template {}: {e}", full.display()))?
        }
    };
    let mut env = Environment::new();
    env.add_template_owned(TEMPLATE_NAME, source)
        .map_err(|e| format!("invalid template: {e}"))?;
    Ok(env)
}

/// Renders a full page with the given template environment.
pub fn render_page(
    env: &Environment<'_>,
    doc: &Document,
    site: &SiteInfo,
) -> Result<String, minijinja::Error> {
    let depth = doc.source.matches('/').count();
    env.get_template(TEMPLATE_NAME)?.render(context! {
        content => Value::from_safe_string(body(doc)),
        page => context! {
            title => page_title(doc),
            source => &doc.source,
            metadata => Value::from(minijinja::value::Serde(&doc.metadata)),
            // Only ever `../` repeated, so it needs no escaping.
            root => Value::from_safe_string("../".repeat(depth)),
        },
        site => context! {
            title => &site.title,
            base_url => &site.base_url,
        },
    })
}

/// Renders a page with the built-in template.
pub fn page(doc: &Document, site: &SiteInfo) -> String {
    let env = environment(None, site).expect("the built-in template is valid");
    render_page(&env, doc, site).expect("the built-in template renders")
}

fn page_title(doc: &Document) -> String {
    doc.metadata
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| {
            doc.descendants().find_map(|n| match n {
                Node::Heading(h) => Some(plain_text(&h.children)),
                _ => None,
            })
        })
        .unwrap_or_else(|| doc.source.clone())
}

/// Renders the document's nodes, without a page around them.
pub fn body(doc: &Document) -> String {
    let mut out = String::new();
    children(&doc.children, &mut out);
    out
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

fn title_attr(title: &Option<String>) -> String {
    title
        .as_ref()
        .map(|t| format!(" title=\"{}\"", escape(t)))
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
                (true, Some(start)) => ("ol", format!(" start=\"{start}\"")),
                (true, None) => ("ol", String::new()),
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
        Node::BlockQuote(n) => {
            out.push_str("<blockquote>\n");
            children(&n.children, out);
            out.push_str("</blockquote>\n");
        }
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
            let attrs = format!(
                " href=\"{}\"{}",
                escape(&rewrite_link(&n.url)),
                title_attr(&n.title)
            );
            wrap("a", &attrs, &n.children, out, false);
        }
        Node::Image(n) => out.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\"{}>",
            escape(&n.url),
            escape(&n.alt),
            title_attr(&n.title)
        )),
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
    use lsdocs_plugin_sdk::doctree::{Heading, Link, Paragraph, text};

    fn site() -> SiteInfo {
        SiteInfo {
            title: Some("Site".into()),
            ..SiteInfo::default()
        }
    }

    fn doc(source: &str) -> Document {
        let mut doc = Document::new(source);
        doc.children = vec![
            Node::Heading(Heading {
                level: 1,
                id: Some("t".into()),
                children: vec![text("Tom & Jerry")],
                ..Heading::default()
            }),
            Node::Paragraph(Paragraph {
                children: vec![
                    text("<script> "),
                    Node::Link(Link {
                        url: "other.md#part".into(),
                        children: vec![text("other")],
                        ..Link::default()
                    }),
                ],
                ..Paragraph::default()
            }),
        ];
        doc
    }

    #[test]
    fn renders_a_page_and_escapes() {
        let html = page(&doc("a.md"), &site());
        assert!(
            html.contains("<title>Tom &amp; Jerry | Site</title>"),
            "{html}"
        );
        assert!(
            html.contains("<h1 id=\"t\">Tom &amp; Jerry</h1>\n<p>&lt;script&gt; <a href=\"other.html#part\">other</a></p>\n"),
            "{html}"
        );
        assert!(html.contains("<a href=\"index.html\">Site</a>"), "{html}");
    }

    #[test]
    fn nested_pages_link_home_relatively() {
        let html = page(&doc("guide/deep/a.md"), &site());
        assert!(
            html.contains("<a href=\"../../index.html\">Site</a>"),
            "{html}"
        );
    }

    #[test]
    fn custom_templates_are_loaded_from_the_site_root() {
        let dir = std::env::temp_dir().join(format!("lsdocs-render-html-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("theme")).unwrap();
        std::fs::write(
            dir.join("theme/page.html"),
            "[{{ page.title }}]{{ content }}",
        )
        .unwrap();
        let site = SiteInfo {
            root: Some(dir.to_string_lossy().into_owned()),
            ..site()
        };
        let env = environment(Some("theme/page.html"), &site).unwrap();
        let html = render_page(&env, &doc("a.md"), &site).unwrap();
        assert!(html.starts_with("[Tom &amp; Jerry]<h1 id=\"t\">"), "{html}");
        let missing = environment(Some("theme/missing.html"), &site).unwrap_err();
        assert!(missing.starts_with("failed to read template"), "{missing}");
        std::fs::remove_dir_all(&dir).unwrap();
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
