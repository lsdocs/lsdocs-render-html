//! A theme's assets in the output: each written to `_lsdocs/`, its name
//! carrying a hash of its contents, so browsers may cache it for good.

use std::collections::BTreeMap;

use lsdocs_plugin_sdk::protocol::OutputFile;
use lsdocs_plugin_sdk::relative_path;

/// The output directory of theme assets.
pub const DIR: &str = "_lsdocs";

/// `_lsdocs/<path-stem>.<hash>.<ext>`: where the asset `path` (relative to
/// the theme's `assets/`) with these contents is written. The hash is the
/// first 8 hex digits of the contents' blake3 hash.
pub fn hashed_path(path: &str, bytes: &[u8]) -> String {
    let hash = blake3::hash(bytes).to_hex();
    let hash = &hash[..8];
    let (dir, file) = match path.rfind('/') {
        Some(at) => path.split_at(at + 1),
        None => ("", path),
    };
    let name = match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem}.{hash}.{ext}"),
        _ => format!("{file}.{hash}"),
    };
    format!("{DIR}/{dir}{name}")
}

/// Resolves the asset `path` into `done` (its output path and final
/// contents), resolving the assets a CSS file refers to first. `active`
/// holds the files being resolved, to stop cycles.
fn resolve(
    path: &str,
    assets: &BTreeMap<String, Vec<u8>>,
    done: &mut BTreeMap<String, (String, Vec<u8>)>,
    active: &mut Vec<String>,
) -> Option<String> {
    if let Some((output, _)) = done.get(path) {
        return Some(output.clone());
    }
    if active.iter().any(|a| a == path) {
        return None;
    }
    let bytes = assets.get(path)?;
    let bytes = match (path.ends_with(".css"), std::str::from_utf8(bytes)) {
        (true, Ok(text)) if text.contains("url(") => {
            active.push(path.to_owned());
            let text = rewrite_urls(text, path, assets, done, active);
            active.pop();
            text.into_bytes()
        }
        _ => bytes.clone(),
    };
    let output = hashed_path(path, &bytes);
    done.insert(path.to_owned(), (output.clone(), bytes));
    Some(output)
}

/// `text` (the CSS file `path`) with its relative `url(...)` references to
/// other assets pointing at their output paths.
fn rewrite_urls(
    text: &str,
    path: &str,
    assets: &BTreeMap<String, Vec<u8>>,
    done: &mut BTreeMap<String, (String, Vec<u8>)>,
    active: &mut Vec<String>,
) -> String {
    let dir = path.rfind('/').map_or("", |at| &path[..=at]);
    // Where the CSS file's output will be, for relative URLs: only its
    // directory matters, and it is `_lsdocs/` plus the source directory.
    let from = format!("{DIR}/{dir}x");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("url(") {
        let (before, after) = rest.split_at(at + 4);
        out.push_str(before);
        let Some(end) = after.find(')') else {
            rest = after;
            break;
        };
        let inner = &after[..end];
        let trimmed = inner.trim();
        let (quote, target) = match trimmed.chars().next() {
            Some(q @ ('"' | '\'')) if trimmed.len() >= 2 && trimmed.ends_with(q) => {
                (Some(q), &trimmed[1..trimmed.len() - 1])
            }
            _ => (None, trimmed),
        };
        match rewrite_target(target, dir, &from, assets, done, active) {
            Some(new) => match quote {
                Some(q) => out.push_str(&format!("{q}{new}{q}")),
                None => out.push_str(&new),
            },
            None => out.push_str(inner),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// The new text of one reference `target`, if it names an asset.
fn rewrite_target(
    target: &str,
    dir: &str,
    from: &str,
    assets: &BTreeMap<String, Vec<u8>>,
    done: &mut BTreeMap<String, (String, Vec<u8>)>,
    active: &mut Vec<String>,
) -> Option<String> {
    if target.is_empty()
        || target.starts_with(['/', '#'])
        || target.contains(':')
        || target.contains('\\')
    {
        return None;
    }
    let split = target.find(['?', '#']).unwrap_or(target.len());
    let (file, suffix) = target.split_at(split);
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in file.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    let output = resolve(&parts.join("/"), assets, done, active)?;
    Some(format!("{}{suffix}", relative_path(from, &output)))
}

/// A theme's assets, with their output paths.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Assets {
    /// Each asset's output path and contents, by its path under `assets/`.
    entries: BTreeMap<String, (String, Vec<u8>)>,
}

impl Assets {
    /// The assets of a theme (by path under `assets/`). Relative `url(...)`
    /// references in CSS files are rewritten to the hashed names of the
    /// assets they refer to (a CSS file's hash covers the rewritten text,
    /// so it changes when a referenced file does). References to files
    /// the theme does not have, and references that form a cycle, are left
    /// as they are.
    pub fn new(assets: &BTreeMap<String, Vec<u8>>) -> Self {
        let mut entries = BTreeMap::new();
        for path in assets.keys() {
            resolve(path, assets, &mut entries, &mut Vec::new());
        }
        Self { entries }
    }

    /// The URL of the asset `path` from the page whose source is `from`
    /// (relative, so the site works from any directory), if there is one.
    pub fn url(&self, from: &str, path: &str) -> Option<String> {
        let (output, _) = self.entries.get(path)?;
        Some(relative_path(from, output))
    }

    /// An import map (JSON) from each JavaScript module's unhashed URL to
    /// its hashed one, from the page `from`, so modules import each other
    /// by their plain names; `None` if the theme has no modules.
    pub fn importmap(&self, from: &str) -> Option<String> {
        // Import maps take URLs starting with `/`, `./` or `../`; others
        // would be bare module names.
        let url = |to: &str| {
            let url = relative_path(from, to);
            if url.starts_with("../") {
                url
            } else {
                format!("./{url}")
            }
        };
        let imports: serde_json::Map<String, serde_json::Value> = self
            .entries
            .iter()
            .filter(|(path, _)| path.ends_with(".js") || path.ends_with(".mjs"))
            .map(|(path, (output, _))| (url(&format!("{DIR}/{path}")), url(output).into()))
            .collect();
        if imports.is_empty() {
            return None;
        }
        let map = serde_json::json!({ "imports": imports }).to_string();
        // Inside `<script>`, a `<` could end the element.
        Some(map.replace('<', "\\u003c"))
    }

    /// The files to write.
    pub fn files(&self) -> Vec<OutputFile> {
        self.entries
            .values()
            .map(|(path, bytes)| OutputFile {
                path: path.clone(),
                output: bytes.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn hashed_names_change_with_contents() {
        let a = hashed_path("js/main.js", b"a");
        let b = hashed_path("js/main.js", b"b");
        assert_ne!(a, b);
        assert_eq!(a, hashed_path("js/main.js", b"a"));
        let hash = &blake3::hash(b"a").to_hex()[..8];
        assert_eq!(a, format!("_lsdocs/js/main.{hash}.js"));
        assert_eq!(
            hashed_path("a.min.css", b"a"),
            format!("_lsdocs/a.min.{hash}.css")
        );
        assert_eq!(
            hashed_path("LICENSE", b"a"),
            format!("_lsdocs/LICENSE.{hash}")
        );
        assert_eq!(hashed_path(".x", b"a"), format!("_lsdocs/.x.{hash}"));
    }

    #[test]
    fn assets_have_relative_urls_and_files() {
        let assets = Assets::new(&BTreeMap::from([
            ("main.css".to_owned(), b"body {}".to_vec()),
            ("js/a.js".to_owned(), b"import './b.js';".to_vec()),
            ("js/b.mjs".to_owned(), b"export {};".to_vec()),
        ]));
        let css = hashed_path("main.css", b"body {}");
        assert_eq!(assets.url("index.md", "main.css"), Some(css.clone()));
        assert_eq!(
            assets.url("guide/deep/a.md", "main.css"),
            Some(format!("../../{css}"))
        );
        assert_eq!(assets.url("a.md", "missing.css"), None);
        let files = assets.files();
        assert_eq!(files.len(), 3);
        assert!(
            files
                .iter()
                .any(|f| f.path == css && f.output == b"body {}")
        );
    }

    #[test]
    fn css_references_follow_the_hashed_names() {
        let theme = |font: &[u8]| {
            Assets::new(&BTreeMap::from([
                (
                    "css/a.css".to_owned(),
                    b"@font-face { src: url(../fonts/x.woff2) }
                      .a { background: url(\"img/b.svg\") }
                      .b { background: url('data:image/png;base64,AAAA') }
                      .c { background: url(https://x.org/y.png) url(#frag) url(/abs.png) url(missing.png) }"
                        .to_vec(),
                ),
                ("fonts/x.woff2".to_owned(), font.to_vec()),
                ("css/img/b.svg".to_owned(), b"<svg/>".to_vec()),
            ]))
        };
        let assets = theme(b"font");
        let font = hashed_path("fonts/x.woff2", b"font");
        let svg = hashed_path("css/img/b.svg", b"<svg/>");
        let files = assets.files();
        let css = files
            .iter()
            .find(|f| f.path.starts_with("_lsdocs/css/a."))
            .unwrap();
        let text = String::from_utf8(css.output.clone()).unwrap();
        assert!(
            text.contains(&format!(
                "url(../{})",
                font.strip_prefix("_lsdocs/").unwrap()
            )),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                "url(\"{}\")",
                svg.strip_prefix("_lsdocs/css/").unwrap()
            )),
            "{text}"
        );
        for kept in [
            "url('data:image/png;base64,AAAA')",
            "url(https://x.org/y.png)",
            "url(#frag)",
            "url(/abs.png)",
            "url(missing.png)",
        ] {
            assert!(text.contains(kept), "{text}");
        }
        assert!(files.iter().any(|f| f.path == font));
        assert!(files.iter().any(|f| f.path == svg));
        // The CSS's own hash follows what it references.
        let css_path = assets.url("a.md", "css/a.css").unwrap();
        assert_eq!(css_path, css.path);
        let other = theme(b"font2").url("a.md", "css/a.css").unwrap();
        assert_ne!(css_path, other);
    }

    #[test]
    fn css_cycles_are_left_alone() {
        let assets = Assets::new(&BTreeMap::from([
            ("a.css".to_owned(), b"@import url(b.css);".to_vec()),
            ("b.css".to_owned(), b"@import url(a.css);".to_vec()),
        ]));
        assert_eq!(assets.files().len(), 2);
    }

    #[test]
    fn the_import_map_maps_plain_module_urls() {
        let assets = Assets::new(&BTreeMap::from([
            ("main.css".to_owned(), b"body {}".to_vec()),
            ("js/a.js".to_owned(), b"import './b.mjs';".to_vec()),
            ("js/b.mjs".to_owned(), b"export {};".to_vec()),
        ]));
        let a = hashed_path("js/a.js", b"import './b.mjs';");
        let b = hashed_path("js/b.mjs", b"export {};");
        let map: serde_json::Value =
            serde_json::from_str(&assets.importmap("guide/x.md").unwrap()).unwrap();
        assert_eq!(
            map,
            serde_json::json!({"imports": {
                "../_lsdocs/js/a.js": format!("../{a}"),
                "../_lsdocs/js/b.mjs": format!("../{b}"),
            }})
        );
        let map: serde_json::Value =
            serde_json::from_str(&assets.importmap("index.md").unwrap()).unwrap();
        assert_eq!(map["imports"]["./_lsdocs/js/a.js"], format!("./{a}"));
        let none = Assets::new(&BTreeMap::from([("a.css".to_owned(), vec![])]));
        assert_eq!(none.importmap("index.md"), None);
    }
}
