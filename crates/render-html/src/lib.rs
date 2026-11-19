//! First-party HTML renderer plugin.
//!
//! Renders each document's body to HTML, then places it in a theme's page
//! template (minijinja). Two themes are built in: `default`, the
//! flagship, and `plain`, one stylesheet with the page's table of
//! contents as a drop-down menu. A site picks one in the plugin's
//! settings: for
//!
//! ```toml
//! [[plugins]]
//! name = "html"
//! builtin = "render-html"
//! ```
//!
//! in `lsdocs.toml`, `.config/lsdocs/html/settings.toml` holds
//!
//! ```toml
//! theme = "plain"                     # a built-in theme, or a directory
//! overrides = "theme-overrides"       # templates and assets replacing the theme's
//! theme_settings = { footer = "..." } # the theme's settings (its `theme.toml`)
//! ```
//!
//! | Setting | Default | Meaning |
//! |---|---|---|
//! | `theme` | `"default"` | A built-in theme's name, else a theme directory, relative to the site directory. |
//! | `theme_settings` | none | Settings for the theme, each declared (with its default) in the theme's `theme.toml`. |
//! | `overrides` | none | A directory (relative to the site directory) whose `templates/` and `assets/` are layered on the theme's, file by file. |
//! | `heading_anchors` | `true` | Add a `#` link to each heading with an id. |
//! | `sitemap` | `true` | Write `sitemap.xml`. Without an absolute `base_url`, `true` given explicitly is a warning. |
//!
//! A theme is a directory of `theme.toml` (its `[settings]`, with their
//! defaults), `templates/` (with `page.html`, the template each page is
//! rendered with) and `assets/` (copied to the output). An override's
//! template can extend the one it replaces as `theme/<path>`
//! (`{% extends "theme/page.html" %}`) and replace one block.
//!
//! Templates see:
//!
//! | Variable | Value |
//! |---|---|
//! | `content` | The rendered body (already HTML). |
//! | `page.title` | `title` front matter, else the first heading, else the source path. |
//! | `page.source` | The source path, e.g. `guide/intro.md`. |
//! | `page.url` | The output path, from the site root, e.g. `guide/intro.html`. |
//! | `page.metadata` | All metadata (front matter, and what transformers added). |
//! | `page.root` | Relative path from this page to the site root, e.g. `../`. |
//! | `page.nav` | Site navigation from `transform-nav`, if it ran (`items`, `prev`, `next`). |
//! | `page.toc` | The page's sections from `transform-toc`, if any (`[{id, title, level, children}]`). |
//! | `page.previous`, `page.next` | The neighboring pages in the navigation (`title`, `source`), if any. |
//! | `site.title`, `site.base_url` | From the site config. |
//! | `theme` | The theme's settings: its defaults, with `theme_settings` on top. |
//! | `lsdocs.version` | lsdocs' version. |
//!
//! They have the filters `link` (a link to a source file becomes a link to
//! its output: `{{ item.url | link }}`) and `page_url` (a source path
//! becomes the relative URL of its output: `{{ item.source | page_url }}`)
//! and `url` (a path from the site root becomes a URL relative to the
//! page, and URLs stay as they are: `{{ theme.logo | url }}`), and the
//! functions `asset(path)`, the relative URL of the theme's asset
//! `path` (under `assets/`), and `asset_importmap()`, an import map from
//! each JavaScript module's unhashed URL to its hashed one, so that a
//! theme's modules import each other by their plain relative names.
//!
//! Assets are written to `_lsdocs/`, each name carrying a hash of the
//! file's contents (`_lsdocs/js/main.1a2b3c4d.js`), at the
//! `render/transform/all` request; templates refer to them through
//! `asset()` only.
//!
//! The body's markup is a contract with themes: classes and data
//! attributes prefixed `lsd-`, listed in `docs/content/reference/html-markup.md`.
//! `custom:highlight` nodes from `transform-highlight` are written as the
//! highlighted HTML they carry, in an `lsd-code` figure; the themes color
//! their tokens. Other `custom` nodes are a `div` (or the block
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
//! `link` filter in templates.
//!
//! After every page is rendered, the plugin writes `sitemap.xml` (via the
//! `render/transform/all` request) if the site has an absolute `base_url`. Pages
//! whose front matter says `sitemap: false` are left out.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use lsdocs_plugin_sdk::config::{Kind, recommended, settings_toml_keys};
use lsdocs_plugin_sdk::doctree::Document;
use lsdocs_plugin_sdk::protocol::{
    ConfigInfo, Diagnostic, OutputFile, RenderParams, RenderResult, RenderTransformAllParams,
    RenderTransformAllResult, Severity, SiteInfo,
};
use lsdocs_plugin_sdk::{Context, Plugin, PluginError, PluginResult, Renderer, relative_path};
use minijinja::value::Serde;
use minijinja::{Environment, ErrorKind, State, Value, context};

pub mod assets;
mod body;
pub mod theme;

pub use assets::Assets;
pub use body::{body, escape, link_href, output_path, rewrite_link};
pub use theme::Theme;

/// The theme used unless the settings name another.
pub const DEFAULT_THEME: &str = "default";

/// The template each page is rendered with.
const TEMPLATE_NAME: &str = "page.html";

/// A settings value (JSON-like).
type Json = lsdocs_plugin_sdk::doctree::Value;

/// The plugin, as registered with the host.
pub fn plugin() -> Plugin {
    Plugin::new("render-html", env!("CARGO_PKG_VERSION"))
        .settings(|c| {
            settings_toml_keys(
                c,
                &[
                    ("theme", Kind::String),
                    ("theme_settings", Kind::Table),
                    ("overrides", Kind::String),
                    ("heading_anchors", Kind::Bool),
                    ("sitemap", Kind::Bool),
                ],
            )
        })
        .renderer(Html::default())
        .cache_key(cache_key)
}

/// The cache key: a digest of the theme with its overrides, since pages
/// depend on them. If they cannot be loaded, nothing is cached (rendering
/// will report the error).
fn cache_key(setup: &lsdocs_plugin_sdk::Setup) -> Option<String> {
    let theme = load_theme(&setup.settings, &setup.params.site).ok()?;
    Some(format!("theme {}", theme.digest()))
}

/// Loads the theme the settings name, with their overrides.
fn load_theme(settings: &Json, site: &SiteInfo) -> Result<Theme, String> {
    let root = Path::new(site.root.as_deref().unwrap_or_default());
    let spec = settings
        .get("theme")
        .and_then(|t| t.as_str())
        .unwrap_or(DEFAULT_THEME);
    let overrides = settings
        .get("overrides")
        .and_then(|o| o.as_str())
        .map(Path::new);
    theme::load(spec, root, overrides)
}

/// The plugin's settings file, for messages.
fn settings_file(config: &ConfigInfo) -> String {
    Path::new(&config.dir)
        .join(recommended(config, "settings.toml"))
        .display()
        .to_string()
}

/// The HTML renderer. The theme is loaded on first use, since it comes
/// from the plugin's settings.
#[derive(Debug, Default)]
pub struct Html {
    prepared: OnceLock<Result<Prepared, String>>,
}

impl Html {
    /// The theme, ready to render with.
    fn prepared(&self, ctx: &Context) -> PluginResult<&Prepared> {
        self.prepared
            .get_or_init(|| {
                let file = settings_file(ctx.config());
                let theme =
                    load_theme(ctx.settings(), ctx.site()).map_err(|e| format!("{file}: {e}"))?;
                Prepared::new(&theme, ctx.settings().get("theme_settings"), &file)
            })
            .as_ref()
            .map_err(|e| PluginError::new(e.clone()))
    }
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
        let mut result = RenderTransformAllResult {
            files: self.prepared(ctx)?.files(),
            diagnostics: vec![],
        };
        let wanted = ctx.settings().get("sitemap").and_then(|v| v.as_bool());
        if wanted == Some(false) {
            return Ok(result);
        }
        let base = ctx
            .site()
            .base_url
            .as_deref()
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
        match base {
            Some(base) => result.files.push(OutputFile {
                path: "sitemap.xml".into(),
                output: sitemap(base, &params).into_bytes(),
            }),
            None if wanted == Some(true) => result.diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                message: "the sitemap needs an absolute `base_url` (http:// or https://); \
                          not writing sitemap.xml"
                    .into(),
                code: Some("sitemap".into()),
                ..Diagnostic::default()
            }),
            None => {}
        }
        Ok(result)
    }

    async fn render(&self, params: RenderParams, ctx: &Context) -> PluginResult<RenderResult> {
        if params.format != "html" {
            return Err(PluginError::new(format!(
                "unsupported format `{}`",
                params.format
            )));
        }
        let prepared = self.prepared(ctx)?;
        let options = Options::from_value(ctx.settings());
        Ok(RenderResult {
            output: render_page(prepared.env(), &params.document, ctx.site(), &options)?
                .into_bytes(),
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
    pub fn from_value(value: &Json) -> Self {
        let defaults = Self::default();
        Self {
            heading_anchors: value
                .get("heading_anchors")
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.heading_anchors),
        }
    }
}

/// A theme ready to render with: its templates in an environment that
/// holds its settings, and its assets.
#[derive(Debug)]
pub struct Prepared {
    env: Environment<'static>,
    assets: Arc<Assets>,
}

impl Prepared {
    /// Prepares `theme` with the site's settings for it (`theme_settings`,
    /// checked against the theme's; they are in `file`, for messages).
    pub fn new(theme: &Theme, theme_settings: Option<&Json>, file: &str) -> Result<Self, String> {
        let values = theme::settings(&theme.settings, theme_settings, file)?;
        if theme.name == DEFAULT_THEME {
            theme::check_default_theme(&values, file)?;
        }
        let assets = Arc::new(Assets::new(&theme.assets));
        let mut env = Environment::new();
        // Escaped here, not by minijinja, which would also escape every `/`.
        env.add_filter("link", |url: String| {
            Value::from_safe_string(escape(&rewrite_link(&url)))
        });
        // The URL of a document's output, by its source path, relative to the
        // page being rendered.
        env.add_filter("page_url", |state: &State, source: String| {
            Value::from_safe_string(escape(&relative_path(
                &page_source(state),
                &output_path(&source),
            )))
        });
        // A path from the site root becomes a URL relative to the page being
        // rendered; URLs (with a scheme, or starting with `/` or `#`) are
        // kept.
        env.add_filter("url", |state: &State, url: String| {
            Value::from_safe_string(escape(&site_url(&page_source(state), &url)))
        });
        let urls = Arc::clone(&assets);
        env.add_function(
            "asset",
            move |state: &State, path: String| -> Result<Value, minijinja::Error> {
                match urls.url(&page_source(state), &path) {
                    Some(url) => Ok(Value::from_safe_string(escape(&url))),
                    None => Err(minijinja::Error::new(
                        ErrorKind::InvalidOperation,
                        format!("the theme has no asset `{path}`"),
                    )),
                }
            },
        );
        let maps = Arc::clone(&assets);
        env.add_function("asset_importmap", move |state: &State| {
            Value::from_safe_string(
                maps.importmap(&page_source(state))
                    .map(|map| format!("<script type=\"importmap\">{map}</script>"))
                    .unwrap_or_default(),
            )
        });
        env.add_global("theme", Value::from(Serde(&values)));
        env.add_global("lsdocs", context! { version => env!("CARGO_PKG_VERSION") });
        for (name, source) in &theme.templates {
            env.add_template_owned(name.clone(), source.clone())
                .map_err(|e| format!("theme `{}`: invalid template {name}: {e}", theme.name))?;
        }
        Ok(Self { env, assets })
    }

    /// The template environment.
    pub fn env(&self) -> &Environment<'static> {
        &self.env
    }

    /// The asset files to write.
    pub fn files(&self) -> Vec<OutputFile> {
        self.assets.files()
    }
}

/// The source path of the page being rendered (empty outside a page).
fn page_source(state: &State) -> String {
    state
        .lookup("page")
        .and_then(|page| page.get_attr("source").ok())
        .and_then(|s| s.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// `url` as a link from the page whose source is `from`: a path from the
/// site root (`images/logo.svg`) becomes relative to the page; a URL with
/// a scheme (`https:`, `mailto:`), or one starting with `/`, `#` or `?`,
/// is kept.
fn site_url(from: &str, url: &str) -> String {
    let scheme = url.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    if url.is_empty() || scheme || url.starts_with(['/', '#', '?']) {
        url.to_owned()
    } else {
        relative_path(from, url)
    }
}

/// Renders a full page with the given template environment.
pub fn render_page(
    env: &Environment<'_>,
    doc: &Document,
    site: &SiteInfo,
    options: &Options,
) -> Result<String, minijinja::Error> {
    let depth = doc.source.matches('/').count();
    let nav = doc.metadata.get("nav");
    let neighbor = |key: &str| {
        nav.and_then(|nav| nav.get(key))
            .filter(|page| page.is_object())
            .map(|page| Value::from(Serde(page)))
    };
    env.get_template(TEMPLATE_NAME)?.render(context! {
        content => Value::from_safe_string(body(doc, options)),
        page => context! {
            title => doc.title().unwrap_or_else(|| doc.source.clone()),
            source => &doc.source,
            // Escaped here: minijinja would also escape every `/`.
            url => Value::from_safe_string(escape(&output_path(&doc.source))),
            metadata => Value::from(Serde(&doc.metadata)),
            nav => nav.map(|nav| Value::from(Serde(nav))),
            toc => doc.metadata.get("toc").map(|toc| Value::from(Serde(toc))),
            previous => neighbor("prev"),
            next => neighbor("next"),
            // Only ever `../` repeated, so it needs no escaping.
            root => Value::from_safe_string("../".repeat(depth)),
        },
        site => context! {
            title => &site.title,
            base_url => &site.base_url,
        },
    })
}

/// Renders a page with the built-in default theme and default options.
pub fn page(doc: &Document, site: &SiteInfo) -> Result<String, String> {
    let theme = theme::load(DEFAULT_THEME, Path::new(""), None)?;
    let prepared = Prepared::new(&theme, None, "settings")?;
    render_page(prepared.env(), doc, site, &Options::default()).map_err(|e| e.to_string())
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
    use lsdocs_plugin_sdk::protocol::InitializeParams;
    use std::path::PathBuf;

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

    fn temp(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lsdocs-render-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Renders `doc` with the theme `spec` (and overrides) under `root`.
    fn render_with(
        spec: &str,
        root: &Path,
        overrides: Option<&str>,
        doc: &Document,
    ) -> Result<String, String> {
        let theme = theme::load(spec, root, overrides.map(Path::new))?;
        let prepared = Prepared::new(&theme, None, "settings.toml")?;
        render_page(prepared.env(), doc, &site(), &Options::default()).map_err(|e| e.to_string())
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
        assert!(
            html.contains("<a class=\"lsd-title\" href=\"index.html\"><span>Site</span></a>"),
            "{html}"
        );
    }

    #[test]
    fn nested_pages_link_home_relatively() {
        let html = page(&doc("guide/deep/a.md"), &site()).unwrap();
        assert!(
            html.contains("href=\"../../index.html\"><span>Site</span></a>"),
            "{html}"
        );
    }

    #[test]
    fn renders_a_bare_page() {
        let mut bare = Document::new("a.md");
        bare.children = vec![Node::Paragraph(Paragraph {
            children: vec![text("Just text.")],
            ..Paragraph::default()
        })];
        let root = temp("bare");
        for spec in ["default", "plain"] {
            let html = render_with(spec, &root, None, &bare).unwrap();
            assert!(html.contains("<p>Just text.</p>"), "{spec}: {html}");
            assert!(!html.contains("<nav"), "{spec}: {html}");
            assert!(
                !html.contains("page-toc") && !html.contains("lsd-toc-sidebar"),
                "{spec}: {html}"
            );
            assert!(!html.contains("<details"), "{spec}: {html}");
            assert!(!html.contains("importmap"), "{spec}: {html}");
            assert!(!html.contains("lsd-drawer"), "{spec}: {html}");
        }
        // With a toc and nav, the same templates show them.
        let mut full = bare.clone();
        full.metadata.insert(
            "toc".into(),
            serde_json::json!([{"id": "a", "title": "A", "level": 2, "children": [
                {"id": "b", "title": "B <i>", "level": 3, "children": []}
            ]}]),
        );
        full.metadata.insert(
            "nav".into(),
            serde_json::json!({
                "items": [{"title": "Home", "url": "a.md", "source": "a.md", "current": true}],
                "prev": null,
                "next": {"title": "Next", "url": "b.md", "source": "b.md"},
            }),
        );
        let plain = render_with("plain", &root, None, &full).unwrap();
        assert!(plain.contains("<details class=\"page-toc\">"), "{plain}");
        assert!(plain.contains("<a href=\"#b\">B &lt;i&gt;</a>"), "{plain}");
        assert!(
            plain.contains("<a class=\"next\" href=\"b.html\">Next &rarr;</a>"),
            "{plain}"
        );
        assert!(!plain.contains("class=\"prev\""), "{plain}");
        let default = render_with("default", &root, None, &full).unwrap();
        assert!(
            default.contains("<nav class=\"lsd-toc-sidebar\" aria-label=\"On this page\">"),
            "{default}"
        );
        assert!(default.contains("aria-current=\"page\""), "{default}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn overrides_replace_one_block() {
        let root = temp("override");
        write(
            &root.join("over/templates/page.html"),
            "{% extends \"theme/page.html\" %}\
             {% block footer %}<footer>Mine</footer>{% endblock %}",
        );
        for spec in ["default", "plain"] {
            let html = render_with(spec, &root, Some("over"), &doc("a.md")).unwrap();
            assert!(html.contains("<footer>Mine</footer>"), "{spec}: {html}");
            assert!(!html.contains("Built with lsdocs"), "{spec}: {html}");
            let home = match spec {
                "plain" => "<a href=\"index.html\">Site</a></header>",
                _ => "<a class=\"lsd-title\" href=\"index.html\"><span>Site</span></a>",
            };
            assert!(html.contains(home), "{spec}: {html}");
            assert!(html.contains("<p>&lt;script&gt; "), "{spec}: {html}");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn theme_settings_are_validated() {
        let root = temp("settings");
        let config = root.join(".config");
        write(
            &config.join("lsdocs/html/settings.toml"),
            "theme = \"mine\"\ntheme_settings = { colour = \"x\" }\n",
        );
        write(
            &root.join("mine/theme.toml"),
            "[settings]\ncolor = \"red\"\n",
        );
        write(
            &root.join("mine/templates/page.html"),
            "{{ theme.color }} {{ lsdocs.version }} {{ page.url }}",
        );
        let plugin = plugin();
        let setup = lsdocs_plugin_sdk::testing::setup(
            &plugin,
            InitializeParams {
                site: SiteInfo {
                    root: Some(root.to_string_lossy().into_owned()),
                    ..SiteInfo::default()
                },
                config: ConfigInfo {
                    dir: config.to_string_lossy().into_owned(),
                    name: "html".into(),
                },
                ..InitializeParams::default()
            },
        );
        let ctx = lsdocs_plugin_sdk::testing::context(&setup);
        let params = RenderParams {
            document: doc("a.md"),
            format: "html".into(),
        };
        let err = Html::default()
            .render(params, &ctx)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("`colour`"), "{err}");
        assert!(err.contains("settings.toml"), "{err}");

        let theme = theme::load("mine", &root, None).unwrap();
        let good = Prepared::new(&theme, Some(&serde_json::json!({"color": "blue"})), "f").unwrap();
        let out =
            render_page(good.env(), &doc("guide/a.md"), &site(), &Options::default()).unwrap();
        assert_eq!(
            out,
            format!("blue {} guide/a.html", env!("CARGO_PKG_VERSION"))
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn asset_urls_are_relative() {
        let root = temp("assets");
        write(&root.join("t/theme.toml"), "");
        write(
            &root.join("t/templates/page.html"),
            "{{ asset('main.css') }}|{{ asset_importmap() }}",
        );
        write(&root.join("t/assets/main.css"), "body {}");
        let hash = &blake3::hash(b"body {}").to_hex()[..8];
        let nested = render_with("t", &root, None, &doc("guide/a.md")).unwrap();
        assert_eq!(nested, format!("../_lsdocs/main.{hash}.css|"));
        let top = render_with("t", &root, None, &doc("a.md")).unwrap();
        assert_eq!(top, format!("_lsdocs/main.{hash}.css|"));

        write(
            &root.join("t/templates/page.html"),
            "{{ asset('nope.css') }}",
        );
        let err = render_with("t", &root, None, &doc("a.md")).unwrap_err();
        assert!(err.contains("`nope.css`"), "{err}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn modules_get_an_import_map() {
        let root = temp("importmap");
        write(&root.join("t/theme.toml"), "");
        write(
            &root.join("t/templates/page.html"),
            "{{ asset_importmap() }}",
        );
        write(&root.join("t/assets/js/main.js"), "import './state.js';");
        write(&root.join("t/assets/js/state.js"), "export {};");
        let main = assets::hashed_path("js/main.js", b"import './state.js';");
        let state = assets::hashed_path("js/state.js", b"export {};");
        let html = render_with("t", &root, None, &doc("guide/a.md")).unwrap();
        let json = html
            .strip_prefix("<script type=\"importmap\">")
            .and_then(|rest| rest.strip_suffix("</script>"))
            .unwrap();
        let map: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            map["imports"]["../_lsdocs/js/main.js"],
            format!("../{main}")
        );
        assert_eq!(
            map["imports"]["../_lsdocs/js/state.js"],
            format!("../{state}")
        );
        std::fs::remove_dir_all(&root).unwrap();
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
        let theme = theme::load(DEFAULT_THEME, Path::new(""), None).unwrap();
        let prepared = Prepared::new(&theme, None, "f").unwrap();
        let out = prepared
            .env()
            .render_str(
                "{{ 'a/b.md#x' | link }} {{ 'https://x.org/y.md?a&b' | link }}",
                (),
            )
            .unwrap();
        assert_eq!(out, "a/b.html#x https://x.org/y.md?a&amp;b");
    }

    #[test]
    fn cache_key_follows_the_theme() {
        let dir = temp("key");
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
        let custom = init(serde_json::json!({"theme": "t"}));
        assert_eq!(cache_key(&custom), None, "unreadable: no caching");
        write(&dir.join("t/theme.toml"), "");
        write(&dir.join("t/templates/page.html"), "a");
        let first = cache_key(&custom).unwrap();
        write(&dir.join("t/templates/page.html"), "b");
        let second = cache_key(&custom).unwrap();
        assert_ne!(second, first);
        write(&dir.join("t/assets/x.css"), "b");
        assert_ne!(cache_key(&custom).unwrap(), second);
        let overridden = init(serde_json::json!({"theme": "t", "overrides": "o"}));
        assert_eq!(cache_key(&overridden), None, "missing overrides");
        let built_in = cache_key(&init(serde_json::Value::Null)).unwrap();
        assert_eq!(cache_key(&init(serde_json::Value::Null)).unwrap(), built_in);
        assert_ne!(
            cache_key(&init(serde_json::json!({"theme": "plain"}))).unwrap(),
            built_in
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A page with every part of the default theme's chrome: navigation,
    /// neighbors, a table of contents, and a link in its content.
    fn full_doc(source: &str) -> Document {
        let mut full = doc(source);
        full.children.push(Node::Paragraph(Paragraph {
            children: vec![Node::Link(Link {
                url: "https://example.org/in-content".into(),
                children: vec![text("out")],
                ..Link::default()
            })],
            ..Paragraph::default()
        }));
        full.metadata.insert(
            "toc".into(),
            serde_json::json!([{"id": "a", "title": "A", "level": 2, "children": []}]),
        );
        full.metadata.insert(
            "nav".into(),
            serde_json::json!({
                "items": [
                    {"title": "Home", "url": "index.md", "source": "index.md"},
                    {"title": "Guide", "children": [
                        {"title": "A", "url": "guide/a.md", "source": "guide/a.md", "current": true},
                    ]},
                ],
                "prev": {"title": "Home", "url": "index.md", "source": "index.md"},
                "next": {"title": "B", "url": "guide/b.md", "source": "guide/b.md"},
            }),
        );
        full
    }

    /// Renders `doc` with the built-in default theme and `settings`.
    fn render_default(settings: serde_json::Value, doc: &Document) -> Result<String, String> {
        let theme = theme::load(DEFAULT_THEME, Path::new(""), None)?;
        let prepared = Prepared::new(&theme, Some(&settings), "html/settings.toml")?;
        render_page(prepared.env(), doc, &site(), &Options::default()).map_err(|e| e.to_string())
    }

    #[test]
    fn default_theme_settings_reach_the_page() {
        let html = render_default(
            serde_json::json!({
                "primary": "teal",
                "accent": "#123456",
                "scheme": "dark",
                "language": "de",
                "font_text": "Inter",
                "logo": "images/logo.svg",
                "favicon": "images/icon.png",
                "extra_css": ["css/extra.css"],
                "extra_js": ["https://example.com/x.js"],
                "social": [{"icon": "mail", "link": "mailto:a@example.com"}],
                "footer": "Copyright &copy; Us",
                "edit_url": "https://example.com/edit/main/{path}",
            }),
            &full_doc("guide/a.md"),
        )
        .unwrap();
        assert!(html.contains("--lsd-primary: #009485;"), "{html}");
        assert!(html.contains("--lsd-accent: #123456;"), "{html}");
        assert!(
            html.contains("<html lang=\"de\" data-lsd-scheme=\"dark\""),
            "{html}"
        );
        assert!(
            html.contains("localStorage.getItem(\"lsd-scheme\")"),
            "{html}"
        );
        assert!(
            html.contains("--lsd-font-text: &quot;Inter&quot;, var(--lsd-font-system);\">"),
            "{html}"
        );
        assert!(html.contains("src=\"../images/logo.svg\""), "{html}");
        assert!(
            html.contains("<link rel=\"icon\" href=\"../images/icon.png\">"),
            "{html}"
        );
        assert!(
            html.contains("<link rel=\"stylesheet\" href=\"../css/extra.css\">"),
            "{html}"
        );
        assert!(
            html.contains("<script src=\"https://example.com/x.js\" defer></script>"),
            "{html}"
        );
        assert!(html.contains("href=\"mailto:a@example.com\""), "{html}");
        assert!(html.contains("Copyright &copy; Us"), "{html}");
        assert!(
            html.contains("href=\"https://example.com/edit/main/guide/a.md\""),
            "{html}"
        );
        // Without settings: no edit link, logo or extra files.
        let html = render_default(serde_json::json!({}), &full_doc("guide/a.md")).unwrap();
        assert!(html.contains("--lsd-primary: #4051b5;"), "{html}");
        assert!(!html.contains("lsd-edit"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(!html.contains("rel=\"icon\""), "{html}");
    }

    #[test]
    fn default_theme_settings_are_checked_inside() {
        let file = "html/settings.toml";
        for (settings, key) in [
            (serde_json::json!({"social": [{"icon": "mail"}]}), "social"),
            (
                serde_json::json!({"social": [{"icon": "mail", "link": "x", "url": "y"}]}),
                "social",
            ),
            (serde_json::json!({"social": ["mail"]}), "social"),
            (serde_json::json!({"extra_css": [1]}), "extra_css"),
            (serde_json::json!({"extra_js": [{}]}), "extra_js"),
        ] {
            let err = render_default(settings.clone(), &doc("a.md")).unwrap_err();
            assert!(
                err.contains(&format!("`{key}`")) && err.contains(file),
                "{settings}: {err}"
            );
        }
    }

    #[test]
    fn no_third_party_requests() {
        let full = full_doc("guide/a.md");
        let html = render_default(serde_json::json!({}), &full).unwrap();
        let content = body(&full, &Options::default());
        assert!(content.contains("https://example.org/in-content"));
        let chrome = html.replace(&content, "");
        for scheme in ["http:", "https:", "//"] {
            let hits: Vec<_> = chrome
                .match_indices(scheme)
                .map(|(at, _)| &chrome[at.saturating_sub(40)..(at + 40).min(chrome.len())])
                .collect();
            assert!(hits.is_empty(), "{scheme}: {hits:#?}");
        }
        let theme = theme::load(DEFAULT_THEME, Path::new(""), None).unwrap();
        for (path, bytes) in &theme.assets {
            let text = String::from_utf8_lossy(bytes).replace(['"', '\''], "");
            for request in ["url(http", "url(//", "@import"] {
                assert!(!text.contains(request), "{path}: {request}");
            }
        }
    }

    #[test]
    fn the_drawer_switch_takes_focus_only_where_it_shows() {
        let theme = theme::load(DEFAULT_THEME, Path::new(""), None).unwrap();
        let css = String::from_utf8(theme.assets["theme.css"].clone()).unwrap();
        let (wide, narrow) = css.split_once("@media (width < 76.25em)").unwrap();
        // Hidden (so out of the tab order) by default, ...
        let rule = &wide[wide.find(".lsd-drawer-toggle {").unwrap()..];
        let rule = &rule[..rule.find('}').unwrap()];
        assert!(rule.contains("visibility: hidden;"), "{rule}");
        // ... shown with its button below 76.25em, and hidden again
        // where the button is (no navigation, 60em to 76.25em).
        let (middle, small) = narrow.split_once("@media (width < 60em)").unwrap();
        assert!(middle.contains(".lsd-drawer-toggle { visibility: visible; }"));
        assert!(
            middle.contains("body:not(:has(.lsd-nav)) .lsd-drawer-toggle { visibility: hidden; }")
        );
        assert!(
            small.contains("body:not(:has(.lsd-nav)) .lsd-drawer-toggle { visibility: visible; }")
        );
    }

    #[tokio::test]
    async fn render_all_writes_the_assets() {
        let setup = lsdocs_plugin_sdk::Setup::with_settings(
            InitializeParams::default(),
            serde_json::json!({"theme": "plain", "sitemap": false}),
        );
        let ctx = lsdocs_plugin_sdk::testing::context(&setup);
        let result = Html::default()
            .render_all(
                RenderTransformAllParams {
                    format: "html".into(),
                    pages: vec![],
                },
                &ctx,
            )
            .await
            .unwrap();
        let paths: Vec<_> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths.len(), 1, "{paths:?}");
        assert!(
            paths[0].starts_with("_lsdocs/plain.") && paths[0].ends_with(".css"),
            "{paths:?}"
        );
    }
}
