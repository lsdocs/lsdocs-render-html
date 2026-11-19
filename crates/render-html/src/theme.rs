//! Themes: loading one (built in, or a directory), layering overrides on
//! it, and checking a site's settings for it against its `theme.toml`.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

/// A loaded theme.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Theme {
    /// The theme as named in the settings (a built-in name, or a directory).
    pub name: String,
    /// Templates, by path under `templates/` (`page.html`). Each of the
    /// theme's own templates is also there as `theme/<path>`, so an
    /// override can extend the template it replaces.
    pub templates: BTreeMap<String, String>,
    /// Assets, by path under `assets/` (`js/main.js`).
    pub assets: BTreeMap<String, Vec<u8>>,
    /// The `[settings]` of `theme.toml`: each setting's default.
    pub settings: toml::Table,
}

/// The built-in themes, embedded by `build.rs`.
mod built_in {
    include!(concat!(env!("OUT_DIR"), "/themes.rs"));
}

impl Theme {
    /// A digest of the theme (its files and its defaults), for cache keys.
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        let mut add = |bytes: &[u8]| {
            hasher.update(&(bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        };
        add(self.name.as_bytes());
        for (path, text) in &self.templates {
            add(path.as_bytes());
            add(text.as_bytes());
        }
        for (path, bytes) in &self.assets {
            add(path.as_bytes());
            add(bytes);
        }
        add(self.settings.to_string().as_bytes());
        hasher.finalize().to_hex()[..16].to_owned()
    }

    /// Adds a file of a theme's directory (`templates/...` or
    /// `assets/...`; others are left out). `original` says the file is the
    /// theme's own, not an override's.
    fn add(&mut self, path: &str, bytes: Vec<u8>, original: bool) -> Result<(), String> {
        if let Some(name) = path.strip_prefix("templates/") {
            let text =
                String::from_utf8(bytes).map_err(|_| format!("template {path} is not UTF-8"))?;
            if original {
                self.templates.insert(format!("theme/{name}"), text.clone());
            }
            self.templates.insert(name.to_owned(), text);
        } else if let Some(name) = path.strip_prefix("assets/") {
            self.assets.insert(name.to_owned(), bytes);
        }
        Ok(())
    }
}

/// The built-in themes' names.
pub fn built_in_names() -> Vec<&'static str> {
    built_in::BUILT_IN.iter().map(|(name, _)| *name).collect()
}

/// Loads `spec`, a built-in theme's name or else a directory relative to
/// `root`, then layers `overrides` (a directory relative to `root`, of
/// `templates/` and `assets/`) on it, file by file.
pub fn load(spec: &str, root: &Path, overrides: Option<&Path>) -> Result<Theme, String> {
    let mut theme = Theme {
        name: spec.to_owned(),
        ..Theme::default()
    };
    let files = match built_in::BUILT_IN.iter().find(|(name, _)| *name == spec) {
        Some((_, files)) => files
            .iter()
            .map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec()))
            .collect(),
        None => {
            let dir = root.join(spec);
            if !dir.is_dir() {
                return Err(format!(
                    "theme `{spec}` is neither a built-in theme ({}) nor a directory ({})",
                    built_in_names().join(", "),
                    dir.display()
                ));
            }
            let manifest = dir.join("theme.toml");
            if !manifest.is_file() {
                return Err(format!(
                    "theme `{spec}`: {} has no theme.toml",
                    dir.display()
                ));
            }
            let mut files = vec![(
                "theme.toml".to_owned(),
                std::fs::read(&manifest)
                    .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?,
            )];
            read_files(&dir, &mut files)?;
            files
        }
    };
    for (path, bytes) in files {
        if path == "theme.toml" {
            theme.settings = manifest(spec, &bytes)?;
        } else {
            theme.add(&path, bytes, true)?;
        }
    }
    if let Some(overrides) = overrides {
        let dir = root.join(overrides);
        if !dir.is_dir() {
            return Err(format!(
                "the theme overrides {} are not a directory",
                dir.display()
            ));
        }
        let mut files = Vec::new();
        read_files(&dir, &mut files)?;
        for (path, bytes) in files {
            theme.add(&path, bytes, false)?;
        }
    }
    if !theme.templates.contains_key("page.html") {
        return Err(format!("theme `{spec}` has no templates/page.html"));
    }
    Ok(theme)
}

/// The settings `theme.toml` declares: its `[settings]` table, the only
/// key it may have.
fn manifest(spec: &str, bytes: &[u8]) -> Result<toml::Table, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| format!("theme `{spec}`: theme.toml is not UTF-8"))?;
    let mut table: toml::Table =
        toml::from_str(text).map_err(|e| format!("theme `{spec}`: theme.toml: {e}"))?;
    let settings = match table.remove("settings") {
        None => toml::Table::new(),
        Some(toml::Value::Table(settings)) => settings,
        Some(_) => {
            return Err(format!(
                "theme `{spec}`: theme.toml: `settings` must be a table"
            ));
        }
    };
    if let Some(key) = table.keys().next() {
        return Err(format!(
            "theme `{spec}`: theme.toml: unknown key `{key}` (known: settings)"
        ));
    }
    if let Some((key, _)) = settings.iter().find(|(_, value)| value.is_datetime()) {
        return Err(format!(
            "theme `{spec}`: theme.toml: setting `{key}` is a date, which settings cannot be"
        ));
    }
    Ok(settings)
}

/// Every file under `dir`'s `templates/` and `assets/`, as
/// `templates/<path>` and `assets/<path>`.
fn read_files(dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
        let cannot = |e: std::io::Error| format!("cannot read {}: {e}", dir.display());
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(cannot)? {
            paths.push(entry.map_err(cannot)?.path());
        }
        paths.sort();
        for path in paths {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return Err(format!("{}: file names must be UTF-8", path.display()));
            };
            let rel = format!("{prefix}/{name}");
            if path.is_dir() {
                walk(&path, &rel, out)?;
            } else {
                let bytes = std::fs::read(&path)
                    .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
                out.push((rel, bytes));
            }
        }
        Ok(())
    }
    for sub in ["templates", "assets"] {
        let path = dir.join(sub);
        if path.is_dir() {
            walk(&path, sub, out)?;
        }
    }
    Ok(())
}

/// What a setting's default says its values must be.
fn kind(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "a string",
        toml::Value::Integer(_) => "an integer",
        toml::Value::Float(_) => "a number",
        toml::Value::Boolean(_) => "a boolean",
        toml::Value::Array(_) => "an array",
        toml::Value::Table(_) => "a table",
        toml::Value::Datetime(_) => "a date",
    }
}

/// Whether `value` is of the kind of the default `default`.
fn same_kind(default: &toml::Value, value: &Value) -> bool {
    match default {
        toml::Value::String(_) => value.is_string(),
        toml::Value::Integer(_) => value.is_i64() || value.is_u64(),
        toml::Value::Float(_) => value.is_number(),
        toml::Value::Boolean(_) => value.is_boolean(),
        toml::Value::Array(_) => value.is_array(),
        toml::Value::Table(_) => value.is_object(),
        toml::Value::Datetime(_) => false,
    }
}

/// The theme's settings for templates: its defaults, with `given` (the
/// site's `theme_settings`) on top. A setting the theme does not declare,
/// or one of another kind than its default, is an error naming `file`.
pub fn settings(
    defaults: &toml::Table,
    given: Option<&Value>,
    file: &str,
) -> Result<Value, String> {
    let mut merged = serde_json::to_value(defaults).map_err(|e| format!("{file}: {e}"))?;
    let Some(given) = given else {
        return Ok(merged);
    };
    let Some(given) = given.as_object() else {
        return Err(format!("{file}: `theme_settings` must be a table"));
    };
    for (key, value) in given {
        let Some(default) = defaults.get(key) else {
            let known: Vec<&str> = defaults.keys().map(String::as_str).collect();
            return Err(format!(
                "{file}: unknown theme setting `{key}` in `theme_settings` (the theme's: {})",
                if known.is_empty() {
                    "none".to_owned()
                } else {
                    known.join(", ")
                }
            ));
        };
        if !same_kind(default, value) {
            return Err(format!(
                "{file}: theme setting `{key}` must be {}",
                kind(default)
            ));
        }
        if let Some(merged) = merged.as_object_mut() {
            merged.insert(key.clone(), value.clone());
        }
    }
    Ok(merged)
}

/// Checks the insides of the built-in `default` theme's array settings,
/// whose defaults (empty) cannot say what their elements are:
/// `extra_css` and `extra_js` are strings, `social` tables of an `icon`
/// and a `link` string. An error names `file`.
pub fn check_default_theme(values: &Value, file: &str) -> Result<(), String> {
    let elements = |key: &str| {
        values
            .get(key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    for key in ["extra_css", "extra_js"] {
        if !elements(key).iter().all(Value::is_string) {
            return Err(format!(
                "{file}: theme setting `{key}` must be an array of strings"
            ));
        }
    }
    let link = |value: &Value| {
        value.as_object().is_some_and(|link| {
            link.len() == 2
                && ["icon", "link"]
                    .iter()
                    .all(|key| link.get(*key).is_some_and(Value::is_string))
        })
    };
    if !elements("social").iter().all(link) {
        return Err(format!(
            "{file}: theme setting `social` must be an array of tables of an `icon` \
             and a `link` (strings), e.g. {{ icon = \"mail\", link = \"mailto:...\" }}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn temp(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lsdocs-theme-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn loads_built_in_and_directory_themes() {
        let root = temp("load");
        let plain = load("plain", &root, None).unwrap();
        assert_eq!(plain.name, "plain");
        for name in [
            "base.html",
            "page.html",
            "theme/base.html",
            "theme/page.html",
        ] {
            assert!(plain.templates.contains_key(name), "{name}");
        }
        assert!(plain.assets.contains_key("plain.css"));
        assert!(load("default", &root, None).is_ok());

        write(
            &root.join("mine/theme.toml"),
            "[settings]\ncolor = \"red\"\n",
        );
        write(&root.join("mine/templates/page.html"), "{{ content }}");
        write(&root.join("mine/assets/js/main.js"), "export {};\n");
        write(&root.join("mine/src/main.ts"), "export {};\n");
        let mine = load("mine", &root, None).unwrap();
        assert_eq!(mine.name, "mine");
        assert_eq!(mine.templates["page.html"], "{{ content }}");
        assert_eq!(mine.assets.keys().collect::<Vec<_>>(), ["js/main.js"]);
        assert_eq!(mine.settings["color"].as_str(), Some("red"));

        let missing = load("nope", &root, None).unwrap_err();
        assert!(
            missing.contains("`nope`") && missing.contains("plain"),
            "{missing}"
        );
        write(&root.join("bare/templates/page.html"), "x");
        let bare = load("bare", &root, None).unwrap_err();
        assert!(bare.contains("theme.toml"), "{bare}");
        write(&root.join("odd/theme.toml"), "colour = 1\n");
        write(&root.join("odd/templates/page.html"), "x");
        let odd = load("odd", &root, None).unwrap_err();
        assert!(odd.contains("`colour`"), "{odd}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn overrides_are_layered_file_by_file() {
        let root = temp("layer");
        write(&root.join("over/templates/page.html"), "new");
        write(&root.join("over/templates/extra.html"), "extra");
        write(&root.join("over/assets/plain.css"), "body {}");
        let theme = load("plain", &root, Some(Path::new("over"))).unwrap();
        assert_eq!(theme.templates["page.html"], "new");
        assert_eq!(theme.templates["extra.html"], "extra");
        assert_ne!(theme.templates["theme/page.html"], "new");
        assert!(theme.templates["base.html"].contains("<!doctype html>"));
        assert_eq!(theme.assets["plain.css"], b"body {}");
        let missing = load("plain", &root, Some(Path::new("gone"))).unwrap_err();
        assert!(missing.contains("gone"), "{missing}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn theme_settings_are_validated() {
        let defaults: toml::Table = toml::from_str("color = \"red\"\nsize = 1\n").unwrap();
        let file = ".config/lsdocs/html/settings.toml";
        let unknown =
            settings(&defaults, Some(&serde_json::json!({"colour": "x"})), file).unwrap_err();
        assert!(
            unknown.contains("`colour`") && unknown.contains(file),
            "{unknown}"
        );
        let kind = settings(&defaults, Some(&serde_json::json!({"color": 1})), file).unwrap_err();
        assert!(
            kind.contains("`color`") && kind.contains("a string"),
            "{kind}"
        );
        let not_table = settings(&defaults, Some(&serde_json::json!(3)), file).unwrap_err();
        assert!(not_table.contains("theme_settings"), "{not_table}");
        assert_eq!(
            settings(&defaults, Some(&serde_json::json!({"color": "blue"})), file).unwrap(),
            serde_json::json!({"color": "blue", "size": 1})
        );
        assert_eq!(
            settings(&defaults, None, file).unwrap(),
            serde_json::json!({"color": "red", "size": 1})
        );
    }
}
