# lsdocs-render-html

Part of [lsdocs](https://github.com/lsdocs), a static-site generator whose
parsers, transformers and renderers are plugins.

`crates/render-html` (`lsdocs-render-html`, builtin `render-html`): the
HTML renderer. Page bodies follow the `lsd-` markup contract; themes are
templates (minijinja) plus assets: the built-in `default` (the flagship,
with TypeScript components and search) and `plain`, or a theme directory,
with overrides. `crates/render-html/README.md` covers theme development
(`nur gen theme`, `nur test theme`); the lsdocs documentation
(https://lsdocs.github.io, "Themes" and "HTML markup") covers using and
writing themes.

## Known limitations

Items marked `[ ]` under TODO are also limitations until done.

- The `default` theme draws code blocks' line numbers and emphasized
  lines with its script (the body writer leaves them as `data-`
  attributes), so they are not shown without JavaScript. The bands
  assume lines of one height (`pre` does not wrap) and use the CSS `lh`
  unit.
- A code block's title bar (`figcaption.lsd-code-title`, before the
  code) shows its caption (`:caption:`) or `title=` from its fence's info
  string (MkDocs' syntax, quotes optional); with both, the title is in
  `strong` and the caption in a paragraph under it. MkDocs' `linenums`
  and `hl_lines` are read from the info string too.
- A directive no plugin handles (`mermaid` and the like: a block `custom`
  node with text `content`) is drawn as its `div.lsd-KIND` holding a
  warning admonition, titled `Unknown directive "KIND"`, around its
  content as a plain code block. Its `argument` and `options` are not
  shown, and a `custom` node without `content` is still an empty `div`.

## TODO

- [ ] `render-html` loads its theme (and overrides) once per session, so
      `lsdocs serve` shows edits to a theme directory only after a
      restart (or a change under `.config/`). Report the theme's files as
      render dependencies, or reload the theme when they change.
- [ ] Old hashed theme assets accumulate in the output directory across
      separate builds and `serve` restarts (`crates/host/src/lib.rs`
      prunes only within one `Session`). Keep a manifest of written
      assets in `.lsdocs-cache/` and prune against it.
- [ ] The `default` theme's social icons are a few generic ones drawn for
      it (`link`, `mail`, `rss`, `chat`, `code`) or an image; no brand
      icons (GitHub, Mastodon, ...) yet, which need a licensed icon set.

From the flagship theme's plan (Phase 13):

- [~] Content components themes style. Done: content tabs (`tab-set` /
      `tab-item` directives, sphinx-design's names, in the `extras`
      plugin, for both parsers) and
      copy buttons, line numbers, highlighted lines and code block
      titles. Not done, and on the TODO list: buttons, icons and emoji,
      and MkDocs-style `===` tab blocks.
- [ ] Instant navigation, code annotations, navigation tabs.
