//! Embeds the built-in themes (`themes/<name>/`) in the plugin: each
//! theme's `theme.toml` and the files under its `templates/` and `assets/`
//! become `$OUT_DIR/themes.rs`, a table of `include_bytes!`. Other files
//! (a theme's TypeScript sources, its tests) are left out.

use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let themes = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("themes");
    let mut out = String::from(
        "/// A theme's files: each one's path in the theme's directory (`/`\n\
         /// separators) and contents.\n\
         pub type Files = &'static [(&'static str, &'static [u8])];\n\n\
         /// The built-in themes: each one's name and files.\n\
         pub static BUILT_IN: &[(&str, Files)] = &[\n",
    );
    for (name, dir) in sorted(&themes)? {
        if !dir.is_dir() {
            continue;
        }
        // Not the whole directory: it holds the TypeScript sources and
        // their `node_modules`.
        for part in ["theme.toml", "templates", "assets"] {
            println!("cargo:rerun-if-changed={}", dir.join(part).display());
        }
        writeln!(out, "    ({name:?}, &[")?;
        let mut files = Vec::new();
        if dir.join("theme.toml").is_file() {
            files.push(("theme.toml".to_owned(), dir.join("theme.toml")));
        }
        for sub in ["templates", "assets"] {
            walk(&dir.join(sub), sub, &mut files)?;
        }
        for (rel, path) in files {
            let path = path.to_str().ok_or("theme paths must be UTF-8")?;
            writeln!(out, "        ({rel:?}, include_bytes!({path:?})),")?;
        }
        writeln!(out, "    ]),")?;
    }
    out.push_str("];\n");
    std::fs::write(
        PathBuf::from(std::env::var("OUT_DIR")?).join("themes.rs"),
        out,
    )?;
    Ok(())
}

/// A directory's entries, by name.
fn sorted(dir: &Path) -> Result<Vec<(String, PathBuf)>, Box<dyn Error>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "theme file names must be UTF-8")?;
        entries.push((name, entry.path()));
    }
    entries.sort();
    Ok(entries)
}

/// Every file under `dir` (if it exists), as `prefix/<path>`.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<(), Box<dyn Error>> {
    if !dir.is_dir() {
        return Ok(());
    }
    for (name, path) in sorted(dir)? {
        let rel = format!("{prefix}/{name}");
        if path.is_dir() {
            walk(&path, &rel, out)?;
        } else {
            out.push((rel, path));
        }
    }
    Ok(())
}
