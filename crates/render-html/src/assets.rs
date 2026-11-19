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

/// A theme's assets, with their output paths.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Assets {
    /// Each asset's output path and contents, by its path under `assets/`.
    entries: BTreeMap<String, (String, Vec<u8>)>,
}

impl Assets {
    /// The assets of a theme (by path under `assets/`).
    pub fn new(assets: &BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            entries: assets
                .iter()
                .map(|(path, bytes)| (path.clone(), (hashed_path(path, bytes), bytes.clone())))
                .collect(),
        }
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
