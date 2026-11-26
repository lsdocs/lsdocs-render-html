# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/lsdocs/lsdocs-render-html/compare/lsdocs-render-html/v0.1.0...lsdocs-render-html/v0.1.1) - 2026-11-26

### Added

- buttons ([#29](https://github.com/lsdocs/lsdocs-render-html/pull/29))
- report a theme directory's files as settings files ([#28](https://github.com/lsdocs/lsdocs-render-html/pull/28))
- *(body)* show unknown doc tree nodes in a warning ([#27](https://github.com/lsdocs/lsdocs-render-html/pull/27))
- *(body)* let block custom nodes ask for a pre ([#26](https://github.com/lsdocs/lsdocs-render-html/pull/26))
- code block line numbers, highlighted lines and titles ([#25](https://github.com/lsdocs/lsdocs-render-html/pull/25))

### Other

- parse with lsdocs-parser-markdown's myst dialect ([#23](https://github.com/lsdocs/lsdocs-render-html/pull/23))

## [0.1.0](https://github.com/lsdocs/lsdocs-render-html/releases/tag/lsdocs-render-html/v0.1.0) - 2026-11-23

### Added

- release automation ([#3](https://github.com/lsdocs/lsdocs-render-html/pull/3))
- snapshot test on tests/site, theme tasks, README, AGENTS.md
- *(render-html)* search
- *(render-html)* the default theme's components
- *(host)* mark generated documents
- *(render-html)* the default theme
- *(render-html)* themes, their settings and assets
- *(render-html)* the lsd- markup contract
- *(host)* plugin settings in .config/ (Phase 13)
- *(sphinx-addons)* index page, toctree navigation and numbering, rubric (Phase 11)
- *(sphinx-addons)* syntax extensions and Sphinx's directives and roles (Phase 11)
- content-hash build cache (Phase 7b)
- adopt the shared document shape in plugins (Phase 6d-3 to 6d-5)
- *(doctree)* add the shared document shape (Phase 6d-1)
- highlight code blocks with transform-highlight (Phase 6c)
- add site navigation and a sitemap (Phase 6b)
- check links, fill cross-references and add backlinks (Phase 6a)
- render a real site from CommonMark with templates (Phase 4)
- add plugin SDK, host pipeline and CLI (Phase 3)

### Fixed

- *(render-html)* rewrite url() references in theme CSS
- *(render-html)* no hidden tab stop for the drawer switch
- *(render-html)* theme fixes from review
- *(render-html)* group footnotes inside sections
- settings of the wrong kind are errors; stale options wording

### Other

- *(deps)* update dependency typescript to v7 ([#8](https://github.com/lsdocs/lsdocs-render-html/pull/8))
- *(README)* link the markup contract on the site
- *(README)* links after the split
- themes
- plugin settings in .config/
- *(protocol)* rename pipeline methods (Phase 13)
- deny unwrap, expect and panic outside tests
- License lsdocs under Apache-2.0 OR MIT
- Scaffold the lsdocs workspace (roadmap Phase 0)
