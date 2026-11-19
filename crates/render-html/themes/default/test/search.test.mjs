// The search engine (the worker's pure functions), on the compiled module.
// Not a worker here, so importing it starts nothing.

import assert from "node:assert/strict";
import test from "node:test";

import { buildIndex, excerpt, search, tokenize } from "../assets/js/search-worker.js";
import { isShortcut, moveActive } from "../assets/js/search.js";

const index = buildIndex({
  pages: [
    {
      url: "guide/setup.html",
      title: "Setup",
      sections: [
        { id: "", title: "Setup", text: "How to set lsdocs up." },
        {
          id: "plugins",
          title: "Plugins",
          text: "Plugins have settings. Settings, settings, settings: every plugin reads its settings file.",
        },
        { id: "settings", title: "Settings", text: "Where they live." },
      ],
    },
    {
      url: "reference/html.html",
      title: "HTML markup",
      sections: [
        {
          id: "escaping",
          title: "Escaping",
          text: "Text such as <script>alert(1)</script> is escaped.",
        },
      ],
    },
  ],
});

/** Results as `url` strings. */
const urls = (query) => search(index, query).map((item) => item.url);

test("tokenize_splits_words_and_lowercases", () => {
  assert.deepEqual(tokenize("Theme_settings, <b>Bold</b> .*("), ["theme", "settings", "b", "bold", "b"]);
  assert.deepEqual(tokenize(""), []);
});

test("ranks_titles_above_text", () => {
  // The "Plugins" section says "settings" five times; "Settings" is the
  // other's title.
  assert.deepEqual(urls("settings"), ["guide/setup.html#settings", "guide/setup.html#plugins"]);
  const [first] = search(index, "settings");
  assert.equal(first.title, "Setup");
  assert.equal(first.section, "Settings");
});

test("matches_prefixes", () => {
  assert.deepEqual(urls("sett"), urls("settings"));
  // Case is ignored; titles still come first ("Setup" also has "set" in its text).
  assert.deepEqual(urls("SET"), [
    "guide/setup.html",
    "guide/setup.html#settings",
    "guide/setup.html#plugins",
  ]);
  // Every word must match.
  assert.deepEqual(urls("plug sett"), ["guide/setup.html#plugins"]);
  assert.deepEqual(urls("plug nothing"), []);
  // A one-letter word matches only itself.
  assert.deepEqual(urls("s"), []);
});

test("search_escapes_and_handles_no_matches", () => {
  for (const query of ["<b>", ".*(", "", "   ", "zzz"]) {
    assert.deepEqual(search(index, query), [], query);
  }
  // Markup in a query is words and punctuation, never markup: `<script>`
  // finds the text that says it, as plain text segments.
  const [item] = search(index, "<script>");
  assert.equal(item.url, "reference/html.html#escaping");
  for (const segment of item.excerpt) {
    assert.deepEqual(Object.keys(segment).sort(), ["match", "text"]);
    assert.equal(typeof segment.text, "string");
    assert.equal(typeof segment.match, "boolean");
  }
  assert.equal(item.excerpt.map((s) => s.text).join(""), "Text such as <script>alert(1)</script> is escaped.");
  assert.deepEqual(
    item.excerpt.filter((s) => s.match).map((s) => s.text),
    ["script", "script"],
  );
});

test("excerpts_are_cut_around_the_first_match", () => {
  const long = `${"word ".repeat(100)}needle ${"more ".repeat(100)}`.trim();
  const segments = excerpt(long, ["needle"]);
  assert.equal(segments[0].text.startsWith("..."), true);
  assert.equal(segments.at(-1).text.endsWith("..."), true);
  assert.deepEqual(
    segments.filter((s) => s.match).map((s) => s.text),
    ["needle"],
  );
  const text = segments.map((s) => s.text).join("");
  assert.ok(text.length < 200, text);
  assert.deepEqual(excerpt("Short.", ["x"]), [{ text: "Short.", match: false }]);
});

test("the_shortcut_is_slash_or_ctrl_k", () => {
  const key = (key, mods = {}) => ({ key, ctrlKey: false, metaKey: false, altKey: false, ...mods });
  assert.equal(isShortcut(key("/")), true);
  assert.equal(isShortcut(key("k", { ctrlKey: true })), true);
  assert.equal(isShortcut(key("K", { metaKey: true })), true);
  assert.equal(isShortcut(key("k")), false);
  assert.equal(isShortcut(key("/", { ctrlKey: true })), false);
  assert.equal(isShortcut(key("k", { ctrlKey: true, altKey: true })), false);
});

test("arrow_keys_move_through_results_and_wrap", () => {
  assert.equal(moveActive(-1, 3, 1), 0);
  assert.equal(moveActive(-1, 3, -1), 2);
  assert.equal(moveActive(0, 3, 1), 1);
  assert.equal(moveActive(2, 3, 1), 0);
  assert.equal(moveActive(0, 3, -1), 2);
  assert.equal(moveActive(-1, 0, 1), -1);
});
