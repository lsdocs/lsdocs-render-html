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
- A block `custom` node with text `content` that no plugin handled (a
  directive nobody declared, say) is drawn as its `div.lsd-KIND` holding
  a warning admonition, titled `Unknown doc tree node "KIND"`, with code
  blocks titled `argument` and `options` (their values as JSON, if set)
  and `content` (plain). A `custom` node without `content` is still an
  empty `div`.
- The theme (and overrides) are loaded once per session. A theme
  directory's files and directories, and the overrides', are reported as
  settings files (`config_files`, absolute paths), so the host's cache
  keys cover them and `lsdocs serve` restarts the plugins, reloading the
  theme, when one changes. Built-in themes report none.

## TODO

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
