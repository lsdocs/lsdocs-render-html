# render-html

lsdocs' HTML renderer: it writes each page with a theme, the theme's
assets, a search index for themes that ask for one, and `sitemap.xml`.
Using themes is covered by "Themes" in the lsdocs documentation
(https://lsdocs.github.io); this file is about working on the
built-in themes.

## Layout

- `src/`: the renderer, the theme loader (`theme.rs`), assets and search.
- `themes/default/`: the flagship theme. `templates/`, `assets/` (CSS,
  and the compiled JavaScript in `assets/js/`), `theme.toml`, and the
  TypeScript sources in `src/` with their tests in `test/`.
- `themes/plain/`: the simple theme, one stylesheet.
- `build.rs` embeds both into the plugin.

## The default theme's scripts

The components (palette, copy button, tabs, table-of-contents
highlighting, search) are TypeScript in `themes/default/src/`, compiled
by `tsc` alone, with no bundler, to `themes/default/assets/js/`. The
compiled output and the vendored `@preact/signals-core`
(`src/vendor/`) are checked in, so building lsdocs or a site needs no
Node.js. Node.js is only needed to change the theme's scripts:

```sh
nur gen theme    # compile src/*.ts into assets/js/, vendor the signals library
nur test theme   # fail if the checked-in output is stale; run node --test
```

Never edit `assets/js/` by hand. `nur ci` runs `nur test theme`.

## The look

The `default` theme's look follows
[zensical-ui](https://github.com/zensical/ui) (MIT licensed): its colors,
layout and components. We took the look, not the code: the templates,
CSS and scripts are our own. Its markup targets the `lsd-` contract that
every parser's output is rendered to (`docs/content/reference/html-markup.md`).

## Browser baseline

The `default` theme uses CSS nesting, `light-dark()`, relative color
syntax, `:has()`, import maps and module workers, with no fallbacks:
2024-era evergreen browsers. Search needs http(s), not `file://`.
