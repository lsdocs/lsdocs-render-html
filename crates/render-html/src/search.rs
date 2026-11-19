//! The search index: each page's sections with their text, without
//! markup. `render` puts a page's entry in its result's `data`, so the
//! host caches it with the page; `render/transform/all` gathers the
//! entries into `_lsdocs/search.json`:
//!
//! ```json
//! { "pages": [{ "url": "guide/setup.html", "title": "Setup",
//!               "sections": [{ "id": "install", "title": "Install", "text": "..." }] }] }
//! ```
//!
//! `url` is the page's output path from the site root. A page's text
//! before its first section is a section with an empty `id` and the
//! page's title. Pages whose front matter says `search: false` are left
//! out.

use lsdocs_plugin_sdk::doctree::{Document, Node, plain_text};
use lsdocs_plugin_sdk::protocol::RenderTransformAllParams;
use serde_json::{Value, json};

/// Where the index is written, from the site root. Not hashed: the
/// theme's script fetches it by this name.
pub const PATH: &str = "_lsdocs/search.json";

/// A section of a page in the index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// The section's anchor; empty for the text before the first section.
    pub id: String,
    /// The section's title.
    pub title: String,
    /// Its text (not its subsections'), without markup, whitespace
    /// collapsed.
    pub text: String,
}

/// The sections of `doc`, in page order, subsections after their
/// section. Sections without text or title are left out.
pub fn sections(doc: &Document) -> Vec<Section> {
    let title = doc.title().unwrap_or_else(|| doc.source.clone());
    let mut out = Vec::new();
    collect(String::new(), title, &doc.children, &mut out);
    out
}

/// Adds the section of `children` (with this id and title), then its
/// subsections.
fn collect(id: String, title: String, children: &[Node], out: &mut Vec<Section>) {
    let mut text = String::new();
    let mut title = title;
    let mut subsections = Vec::new();
    for (i, child) in children.iter().enumerate() {
        match child {
            Node::Section(section) => subsections.push(section),
            // A section's title is its first heading.
            Node::Heading(heading) if i == 0 && !id.is_empty() => {
                title = collapse(&plain_text(&heading.children));
            }
            other => add_text(other, &mut text),
        }
    }
    let text = collapse(&text);
    if !text.is_empty() || !id.is_empty() {
        out.push(Section { id, title, text });
    }
    for section in subsections {
        let id = section.id.clone().unwrap_or_default();
        collect(id, String::new(), &section.children, out);
    }
}

/// Appends the text of `node` to `out`, blocks separated by spaces.
/// Code is included (readers search for names in it); raw markup, math
/// and footnote references are not.
fn add_text(node: &Node, out: &mut String) {
    match node {
        Node::Text(t) => out.push_str(&t.value),
        Node::InlineCode(c) => out.push_str(&c.value),
        Node::CodeBlock(c) => {
            out.push(' ');
            out.push_str(&c.value);
            out.push(' ');
        }
        Node::Image(i) => out.push_str(&i.alt),
        Node::Raw(_) | Node::Math(_) | Node::FootnoteReference(_) => {}
        Node::LineBreak(_) => out.push(' '),
        other => {
            let block = !matches!(
                other,
                Node::Emphasis(_) | Node::Strong(_) | Node::Link(_) | Node::Custom(_)
            );
            if block {
                out.push(' ');
            }
            for child in other.children() {
                add_text(child, out);
            }
            if block {
                out.push(' ');
            }
        }
    }
}

/// `text` with runs of whitespace as one space, trimmed.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A page's entry, for its render result's `data`.
pub fn entry(doc: &Document) -> Value {
    let sections: Vec<Value> = sections(doc)
        .into_iter()
        .map(|s| json!({ "id": s.id, "title": s.title, "text": s.text }))
        .collect();
    json!({
        "title": doc.title().unwrap_or_else(|| doc.source.clone()),
        "sections": sections,
    })
}

/// The index (`search.json`) of the rendered pages, from the entries
/// in their `data`. Pages without one (rendered by a theme without
/// search) and pages with `search: false` are left out.
pub fn index(params: &RenderTransformAllParams) -> String {
    let pages: Vec<Value> = params
        .pages
        .iter()
        .filter(|page| page.metadata.get("search").and_then(Value::as_bool) != Some(false))
        .filter_map(|page| {
            let title = page.data.get("title")?.clone();
            let sections = page.data.get("sections")?.clone();
            Some(json!({ "url": page.path, "title": title, "sections": sections }))
        })
        .collect();
    json!({ "pages": pages }).to_string()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use lsdocs_plugin_sdk::doctree::{CodeBlock, Emphasis, Paragraph, Raw};
    use lsdocs_plugin_sdk::doctree::{Heading, Section as Sec, text};
    use lsdocs_plugin_sdk::protocol::RenderedPage;

    fn heading(level: u8, title: &str) -> Node {
        Node::Heading(Heading {
            level,
            children: vec![text(title)],
            ..Heading::default()
        })
    }

    fn para(children: Vec<Node>) -> Node {
        Node::Paragraph(Paragraph {
            children,
            ..Paragraph::default()
        })
    }

    fn section(id: &str, children: Vec<Node>) -> Node {
        Node::Section(Sec {
            id: Some(id.into()),
            children,
            ..Sec::default()
        })
    }

    #[test]
    fn search_index_lists_sections() {
        let mut doc = Document::new("guide.md");
        doc.children = vec![section(
            "guide",
            vec![
                heading(1, "Guide"),
                para(vec![text("Read  this\n first.")]),
                section(
                    "setup",
                    vec![
                        heading(2, "Setup"),
                        para(vec![
                            text("Run "),
                            Node::Emphasis(Emphasis {
                                children: vec![text("it")],
                                ..Emphasis::default()
                            }),
                            text("."),
                        ]),
                        Node::Raw(Raw {
                            format: "html".into(),
                            value: "<b>raw</b>".into(),
                            ..Raw::default()
                        }),
                        Node::CodeBlock(CodeBlock {
                            value: "lsdocs build".into(),
                            ..CodeBlock::default()
                        }),
                        para(vec![text("Done.")]),
                    ],
                ),
            ],
        )];
        assert_eq!(
            sections(&doc),
            [
                Section {
                    id: "guide".into(),
                    title: "Guide".into(),
                    text: "Read this first.".into(),
                },
                Section {
                    id: "setup".into(),
                    title: "Setup".into(),
                    text: "Run it. lsdocs build Done.".into(),
                },
            ]
        );

        // Text before the first section belongs to the page.
        let mut intro = Document::new("intro.md");
        intro.metadata.insert("title".into(), json!("Intro"));
        intro.children = vec![para(vec![text("Hello.")])];
        let params = RenderTransformAllParams {
            format: "html".into(),
            pages: vec![
                RenderedPage {
                    source: "guide.md".into(),
                    path: "guide.html".into(),
                    data: entry(&doc),
                    ..RenderedPage::default()
                },
                RenderedPage {
                    source: "hidden.md".into(),
                    path: "hidden.html".into(),
                    metadata: [("search".to_owned(), json!(false))].into(),
                    data: entry(&intro),
                },
                RenderedPage {
                    source: "intro.md".into(),
                    path: "intro.html".into(),
                    data: entry(&intro),
                    ..RenderedPage::default()
                },
                RenderedPage {
                    source: "plain.md".into(),
                    path: "plain.html".into(),
                    ..RenderedPage::default()
                },
            ],
        };
        let index: Value = serde_json::from_str(&index(&params)).unwrap();
        assert_eq!(
            index,
            json!({"pages": [
                {"url": "guide.html", "title": "Guide", "sections": [
                    {"id": "guide", "title": "Guide", "text": "Read this first."},
                    {"id": "setup", "title": "Setup", "text": "Run it. lsdocs build Done."},
                ]},
                {"url": "intro.html", "title": "Intro", "sections": [
                    {"id": "", "title": "Intro", "text": "Hello."},
                ]},
            ]})
        );
    }
}
