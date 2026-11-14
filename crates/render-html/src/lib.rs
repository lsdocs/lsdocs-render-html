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
//! | `page.nav` | Site navigation from `transform-nav`, if it ran (`items`, `prev`, `next`). |
//! | `site.title`, `site.base_url` | From the site config. |
//!
//! Relative links to other Markdown pages (`guide.md#setup`) are rewritten
//! to their HTML output (`guide.html#setup`), in the body and through the
//! `link` filter in templates (`{{ item.url | link }}`).
//!
//! After every page is rendered, the plugin writes `sitemap.xml` (via the
//! `render/site` request) if the site has an absolute `base_url`. Pages
//! whose front matter says `sitemap: false` are left out.
//!
//! Other options:
//!
//! | Option | Default | Meaning |
//! |---|---|---|
//! | `heading_anchors` | `true` | Add a `#` link to each heading with an id. |
//! | `sitemap` | `true` | Write `sitemap.xml`. Without an absolute `base_url`, `true` given explicitly is a warning. |

use std::path::Path;
use std::sync::OnceLock;

use lsdocs_plugin_sdk::doctree::{Align, Document, Node};
use lsdocs_plugin_sdk::protocol::{
    Diagnostic, OutputFile, RenderParams, RenderResult, RenderSiteParams, RenderSiteResult,
    Severity, SiteInfo,
};
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
    const SITE: bool = true;

    fn formats(&self) -> Vec<String> {
        vec!["html".into()]
    }

    async fn render_site(
        &self,
        params: RenderSiteParams,
        ctx: &Context,
    ) -> PluginResult<RenderSiteResult> {
        let wanted = ctx.options().get("sitemap").and_then(|v| v.as_bool());
        if wanted == Some(false) {
            return Ok(RenderSiteResult::default());
        }
        let base = ctx
            .site()
            .base_url
            .as_deref()
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
        let Some(base) = base else {
            let mut result = RenderSiteResult::default();
            if wanted == Some(true) {
                result.diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    message: "the sitemap needs an absolute `base_url` (http:// or https://); \
                              not writing sitemap.xml"
                        .into(),
                    code: Some("sitemap".into()),
                    ..Diagnostic::default()
                });
            }
            return Ok(result);
        };
        Ok(RenderSiteResult {
            files: vec![OutputFile {
                path: "sitemap.xml".into(),
                output: sitemap(base, &params).into_bytes(),
            }],
            diagnostics: vec![],
        })
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
        let options = Options::from_value(ctx.options());
        Ok(RenderResult {
            output: render_page(env, &params.document, ctx.site(), &options)?.into_bytes(),
            extension: "html".into(),
            diagnostics: vec![],
        })
    }
}

/// Rendering options, from the plugin's `options`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Add a `#` link to each heading that has an id.
    pub heading_anchors: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            heading_anchors: true,
        }
    }
}

impl Options {
    /// Reads the options from the plugin's `options` value; missing or
    /// mistyped ones keep their defaults.
    pub fn from_value(value: &lsdocs_plugin_sdk::doctree::Value) -> Self {
        let defaults = Self::default();
        Self {
            heading_anchors: value
                .get("heading_anchors")
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.heading_anchors),
        }
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
    // Escaped here, not by minijinja, which would also escape every `/`.
    env.add_filter("link", |url: String| {
        Value::from_safe_string(escape(&rewrite_link(&url)))
    });
    env.add_template_owned(TEMPLATE_NAME, source)
        .map_err(|e| format!("invalid template: {e}"))?;
    Ok(env)
}

/// Renders a full page with the given template environment.
pub fn render_page(
    env: &Environment<'_>,
    doc: &Document,
    site: &SiteInfo,
    options: &Options,
) -> Result<String, minijinja::Error> {
    let depth = doc.source.matches('/').count();
    env.get_template(TEMPLATE_NAME)?.render(context! {
        content => Value::from_safe_string(body(doc, options)),
        page => context! {
            title => doc.title().unwrap_or_else(|| doc.source.clone()),
            source => &doc.source,
            metadata => Value::from(minijinja::value::Serde(&doc.metadata)),
            nav => doc.metadata.get("nav").map(|nav| Value::from(minijinja::value::Serde(nav))),
            // Only ever `../` repeated, so it needs no escaping.
            root => Value::from_safe_string("../".repeat(depth)),
        },
        site => context! {
            title => &site.title,
            base_url => &site.base_url,
        },
    })
}

/// Renders a page with the built-in template and default options.
pub fn page(doc: &Document, site: &SiteInfo) -> String {
    let env = environment(None, site).expect("the built-in template is valid");
    render_page(&env, doc, site, &Options::default()).expect("the built-in template renders")
}

/// A sitemap (sitemaps.org protocol) of the rendered pages, with URLs under
/// `base`. `index.html` pages are listed by their directory URL.
pub fn sitemap(base: &str, params: &RenderSiteParams) -> String {
    let base = base.trim_end_matches('/');
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in &params.pages {
        if page.metadata.get("sitemap").and_then(|v| v.as_bool()) == Some(false) {
            continue;
        }
        let path = match page.path.strip_suffix("index.html") {
            Some(dir) if dir.is_empty() || dir.ends_with('/') => dir,
            _ => &page.path,
        };
        out.push_str(&format!(
            "  <url><loc>{}</loc></url>\n",
            escape(&format!("{base}/{}", url_path(path)))
        ));
    }
    out.push_str("</urlset>\n");
    out
}

/// Percent-encodes a path for use in a URL, leaving `/` and unreserved
/// characters as they are.
fn url_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Renders the document's nodes, without a page around them.
pub fn body(doc: &Document, options: &Options) -> String {
    let mut out = String::new();
    children(options, &doc.children, &mut out);
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

fn children(opts: &Options, nodes: &[Node], out: &mut String) {
    for node in nodes {
        render(opts, node, out);
    }
}

fn wrap(opts: &Options, tag: &str, attrs: &str, nodes: &[Node], out: &mut String, newline: bool) {
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

fn render(opts: &Options, node: &Node, out: &mut String) {
    match node {
        Node::Section(n) => wrap(opts, "section", &id_attr(&n.id), &n.children, out, true),
        Node::Heading(n) => {
            let level = n.level.clamp(1, 6);
            out.push_str(&format!("<h{level}{}>", id_attr(&n.id)));
            children(opts, &n.children, out);
            if opts.heading_anchors
                && let Some(id) = &n.id
            {
                out.push_str(&format!(
                    " <a class=\"anchor\" href=\"#{}\" aria-label=\"Link to this section\">#</a>",
                    escape(id)
                ));
            }
            out.push_str(&format!("</h{level}>\n"));
        }
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
            out.push_str("</table>\n");
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
                escape(&rewrite_link(&n.url)),
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
        Node::Custom(n) => {
            let attrs = format!(" class=\"{}\"", escape(&n.kind));
            wrap(opts, "div", &attrs, &n.children, out, true);
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
            html.contains("<h1 id=\"t\">Tom &amp; Jerry <a class=\"anchor\" href=\"#t\" aria-label=\"Link to this section\">#</a></h1>\n<p>&lt;script&gt; <a href=\"other.html#part\">other</a></p>\n"),
            "{html}"
        );
        assert!(html.contains("<a href=\"index.html\">Site</a>"), "{html}");
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
        let html = render_page(&env, &doc("a.md"), &site, &Options::default()).unwrap();
        assert!(html.starts_with("[Tom &amp; Jerry]<h1 id=\"t\">"), "{html}");
        let missing = environment(Some("theme/missing.html"), &site).unwrap_err();
        assert!(missing.starts_with("failed to read template"), "{missing}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sitemaps_list_page_urls() {
        use lsdocs_plugin_sdk::protocol::RenderedPage;
        let page = |path: &str, metadata: serde_json::Value| RenderedPage {
            source: String::new(),
            path: path.into(),
            metadata: serde_json::from_value(metadata).unwrap(),
        };
        let params = RenderSiteParams {
            format: "html".into(),
            pages: vec![
                page("guide/index.html", serde_json::json!({})),
                page("guide/a b.html", serde_json::json!({})),
                page("hidden.html", serde_json::json!({"sitemap": false})),
                page("index.html", serde_json::json!({})),
                page("reindex.html", serde_json::json!({})),
            ],
        };
        assert_eq!(
            sitemap("https://example.com/docs/", &params),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n\
             \x20 <url><loc>https://example.com/docs/guide/</loc></url>\n\
             \x20 <url><loc>https://example.com/docs/guide/a%20b.html</loc></url>\n\
             \x20 <url><loc>https://example.com/docs/</loc></url>\n\
             \x20 <url><loc>https://example.com/docs/reindex.html</loc></url>\n\
             </urlset>\n"
        );
    }

    #[test]
    fn templates_can_rewrite_links() {
        let env = environment(None, &site()).unwrap();
        let out = env
            .render_str(
                "{{ 'a/b.md#x' | link }} {{ 'https://x.org/y.md?a&b' | link }}",
                (),
            )
            .unwrap();
        assert_eq!(out, "a/b.html#x https://x.org/y.md?a&amp;b");
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
