//! Renders `tests/site` with both built-in themes and snapshots every page,
//! so markup and template changes show up in review. The pages are parsed
//! with lsdocs-parser-markdown; navigation, toc and a tab set are added by
//! hand (they come from other plugins).
//!
//! After an intended change, update the snapshots with
//! `INSTA_UPDATE=always cargo test --test site` and review the diff.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use lsdocs_plugin_sdk::doctree::{Custom, Document, Node, Paragraph, text};
use lsdocs_plugin_sdk::protocol::SiteInfo;
use lsdocs_render_html::{Options, Prepared, render_page, theme};
use serde_json::json;

fn site_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/site")
}

fn parse(source: &str) -> Document {
    let text = std::fs::read_to_string(site_dir().join(source)).unwrap();
    let (doc, diagnostics) = lsdocs_parser_markdown::parse(source, &text);
    assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
    doc
}

/// The pages, with what other plugins would add.
fn pages() -> Vec<Document> {
    // transform-nav's shape: links by `url` (relative to the page) and
    // `source`.
    let nav = |current: &str, prev: Option<&str>, next: Option<&str>| {
        let rel = |source: &str| {
            if current.contains('/') {
                format!("../{source}")
            } else {
                source.to_owned()
            }
        };
        let title = |source: &str| {
            if source == "index.md" {
                "Test site"
            } else {
                "Setup"
            }
        };
        let link =
            |p: Option<&str>| p.map(|s| json!({"title": title(s), "url": rel(s), "source": s}));
        json!({
            "items": [
                {"title": "Test site", "url": rel("index.md"), "source": "index.md", "current": current == "index.md"},
                {"title": "Guide", "open": true, "children": [
                    {"title": "Setup", "url": rel("guide/setup.md"), "source": "guide/setup.md", "current": current == "guide/setup.md"},
                ]},
            ],
            "prev": link(prev),
            "next": link(next),
        })
    };
    let mut index = parse("index.md");
    index
        .metadata
        .insert("nav".into(), nav("index.md", None, Some("guide/setup.md")));
    index.metadata.insert(
        "toc".into(),
        json!([{"id": "code", "title": "Code", "level": 2, "children": [
            {"id": "deeper", "title": "Deeper", "level": 3, "children": []}
        ]}]),
    );
    let tab = |label: &str, body: &str| {
        let mut item = Custom {
            kind: "tab-item".into(),
            children: vec![Node::Paragraph(Paragraph {
                children: vec![text(body)],
                ..Paragraph::default()
            })],
            ..Custom::default()
        };
        item.attrs.insert("label".into(), json!(label));
        Node::Custom(item)
    };
    index.children.push(Node::Custom(Custom {
        kind: "tab-set".into(),
        children: vec![tab("Rust", "cargo build"), tab("Python", "uv sync")],
        ..Custom::default()
    }));
    let mut setup = parse("guide/setup.md");
    setup
        .metadata
        .insert("nav".into(), nav("guide/setup.md", Some("index.md"), None));
    vec![index, setup]
}

fn render(theme_name: &str) -> Vec<(String, String)> {
    let theme = theme::load(theme_name, Path::new(""), None).unwrap();
    let prepared = Prepared::new(&theme, None, "settings.toml").unwrap();
    let site = SiteInfo {
        title: Some("Test site".into()),
        ..SiteInfo::default()
    };
    pages()
        .iter()
        .map(|doc| {
            let html = render_page(prepared.env(), doc, &site, &Options::default()).unwrap();
            (doc.source.replace('/', "__"), html)
        })
        .collect()
}

#[test]
fn renders_the_test_site() {
    for theme_name in ["default", "plain"] {
        for (name, html) in render(theme_name) {
            insta::assert_snapshot!(format!("{theme_name}__{name}"), html);
        }
    }
}
