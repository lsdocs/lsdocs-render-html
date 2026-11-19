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
//! ```
//!
//! with its settings in `.config/lsdocs/html/settings.toml`:
//!
//! ```toml
//! template = "theme/page.html"
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
//! The body's markup is a contract with themes: classes and data
//! attributes prefixed `lsd-`, listed in `docs/content/reference/html-markup.md`.
//! `custom:highlight` nodes from `transform-highlight` are written as the
//! highlighted HTML they carry, in an `lsd-code` figure; the default theme
//! colors their tokens. Other `custom` nodes are a `div` (or the block
//! element their `html_tag` names: `p`, `h1` to `h6`, `section`, `aside`,
//! `nav`) with their kind, prefixed `lsd-`, as its class (and their `id`
//! attribute as its id) around their children, or, with the attribute
//! `inline: true`, a `span` (or the element their `html_tag` names, among
//! `abbr`, `code`, `kbd` and other inline elements) with their `title`
//! attribute as its title. `custom:tab-set` and `custom:tab-item` (from
//! `sphinx-addons`) are an `<lsd-tabs>` element of labelled `lsd-tab`s.
//!
//! Relative links to other Markdown pages (`guide.md#setup`) are rewritten
//! to their HTML output (`guide.html#setup`), in the body and through the
//! `link` filter in templates (`{{ item.url | link }}`).
//!
//! After every page is rendered, the plugin writes `sitemap.xml` (via the
//! `render/transform/all` request) if the site has an absolute `base_url`. Pages
//! whose front matter says `sitemap: false` are left out.
//!
//! Other settings:
//!
//! | Option | Default | Meaning |
//! |---|---|---|
//! | `heading_anchors` | `true` | Add a `#` link to each heading with an id. |
//! | `sitemap` | `true` | Write `sitemap.xml`. Without an absolute `base_url`, `true` given explicitly is a warning. |

use std::path::Path;
use std::sync::OnceLock;

use lsdocs_plugin_sdk::doctree::Document;
use lsdocs_plugin_sdk::protocol::{
    Diagnostic, OutputFile, RenderParams, RenderResult, RenderTransformAllParams,
    RenderTransformAllResult, Severity, SiteInfo,
};
use lsdocs_plugin_sdk::{Context, Plugin, PluginError, PluginResult, Renderer, relative_path};
use minijinja::{Environment, State, Value, context};

mod body;

pub use body::{body, escape, link_href, output_path, rewrite_link};

/// The built-in page template.
pub const DEFAULT_TEMPLATE: &str = include_str!("../theme/page.html");

const TEMPLATE_NAME: &str = "page.html";

/// The plugin, as registered with the host.
pub fn plugin() -> Plugin {
    Plugin::new("render-html", env!("CARGO_PKG_VERSION"))
        .settings(|c| {
            lsdocs_plugin_sdk::config::settings_toml_keys(
                c,
                &[
                    ("template", lsdocs_plugin_sdk::config::Kind::String),
                    ("heading_anchors", lsdocs_plugin_sdk::config::Kind::Bool),
                    ("sitemap", lsdocs_plugin_sdk::config::Kind::Bool),
                ],
            )
        })
        .renderer(Html::default())
        .cache_key(cache_key)
}

/// The cache key: a digest of the custom template, if there is one, since
/// pages depend on it. If it cannot be read, nothing is cached (rendering
/// will report the error).
fn cache_key(setup: &lsdocs_plugin_sdk::Setup) -> Option<String> {
    let init = &setup.params;
    let Some(path) = setup.settings.get("template").and_then(|t| t.as_str()) else {
        return Some("built-in template".into());
    };
    let full = match &init.site.root {
        Some(root) => Path::new(root).join(path),
        None => Path::new(path).to_path_buf(),
    };
    let text = std::fs::read(full).ok()?;
    Some(format!("template {:016x}", fnv1a(&text)))
}

/// The 64-bit FNV-1a hash: small, and the same in every build.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The HTML renderer. The template is loaded on first use, since its path
/// comes from the plugin's settings.
#[derive(Debug, Default)]
pub struct Html {
    env: OnceLock<Result<Environment<'static>, String>>,
}

impl Renderer for Html {
    const RENDERS_ALL: bool = true;

    fn formats(&self) -> Vec<String> {
        vec!["html".into()]
    }

    async fn render_all(
        &self,
        params: RenderTransformAllParams,
        ctx: &Context,
    ) -> PluginResult<RenderTransformAllResult> {
        let wanted = ctx.settings().get("sitemap").and_then(|v| v.as_bool());
        if wanted == Some(false) {
            return Ok(RenderTransformAllResult::default());
        }
        let base = ctx
            .site()
            .base_url
            .as_deref()
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
        let Some(base) = base else {
            let mut result = RenderTransformAllResult::default();
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
        Ok(RenderTransformAllResult {
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
                    ctx.settings().get("template").and_then(|t| t.as_str()),
                    ctx.site(),
                )
            })
            .as_ref()
            .map_err(|e| PluginError::new(e.clone()))?;
        let options = Options::from_value(ctx.settings());
        Ok(RenderResult {
            output: render_page(env, &params.document, ctx.site(), &options)?.into_bytes(),
            extension: "html".into(),
            diagnostics: vec![],
        })
    }
}

/// Rendering options, from the plugin's settings.
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
    /// Reads the options from the plugin's settings; missing or
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
    // The URL of a document's output, by its source path, relative to the
    // page being rendered.
    env.add_filter("page_url", |state: &State, source: String| {
        let from = state
            .lookup("page")
            .and_then(|page| page.get_attr("source").ok())
            .and_then(|s| s.as_str().map(str::to_owned))
            .unwrap_or_default();
        Value::from_safe_string(escape(&relative_path(&from, &output_path(&source))))
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
pub fn page(doc: &Document, site: &SiteInfo) -> Result<String, String> {
    let env = environment(None, site)?;
    render_page(&env, doc, site, &Options::default()).map_err(|e| e.to_string())
}

/// A sitemap (sitemaps.org protocol) of the rendered pages, with URLs under
/// `base`. `index.html` pages are listed by their directory URL.
pub fn sitemap(base: &str, params: &RenderTransformAllParams) -> String {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use lsdocs_plugin_sdk::doctree::{Heading, Link, Node, Paragraph, text};

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
        let html = page(&doc("a.md"), &site()).unwrap();
        assert!(
            html.contains("<title>Tom &amp; Jerry | Site</title>"),
            "{html}"
        );
        assert!(
            html.contains("<h1 id=\"t\">Tom &amp; Jerry <a class=\"lsd-anchor\" href=\"#t\" aria-label=\"Link to this section\">#</a></h1>\n<p>&lt;script&gt; <a href=\"other.html#part\">other</a></p>\n"),
            "{html}"
        );
        assert!(html.contains("<a href=\"index.html\">Site</a>"), "{html}");
    }

    #[test]
    fn nested_pages_link_home_relatively() {
        let html = page(&doc("guide/deep/a.md"), &site()).unwrap();
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
        let params = RenderTransformAllParams {
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
    fn cache_key_follows_the_template() {
        use lsdocs_plugin_sdk::protocol::InitializeParams;
        let dir = std::env::temp_dir().join(format!("lsdocs-render-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let init = |options: serde_json::Value| {
            lsdocs_plugin_sdk::Setup::with_settings(
                InitializeParams {
                    site: SiteInfo {
                        root: Some(dir.to_string_lossy().into_owned()),
                        ..SiteInfo::default()
                    },
                    ..InitializeParams::default()
                },
                options,
            )
        };
        let custom = init(serde_json::json!({"template": "t.html"}));
        assert_eq!(cache_key(&custom), None, "unreadable: no caching");
        std::fs::write(dir.join("t.html"), "a").unwrap();
        let first = cache_key(&custom).unwrap();
        std::fs::write(dir.join("t.html"), "b").unwrap();
        assert_ne!(cache_key(&custom).unwrap(), first);
        assert_eq!(
            cache_key(&init(serde_json::Value::Null)).as_deref(),
            Some("built-in template")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
