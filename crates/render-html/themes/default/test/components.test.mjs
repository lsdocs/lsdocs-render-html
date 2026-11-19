// The default theme's pure helpers, on the compiled modules (assets/js).
// Run with `nur test theme`.

import assert from "node:assert/strict";
import test from "node:test";

import { nextScheme } from "../assets/js/palette.js";
import { linkedTabs } from "../assets/js/tabs.js";
import { currentSection } from "../assets/js/toc.js";
import { consent, dismissed, query, scheme } from "../assets/js/state.js";

test("next_scheme_cycles", () => {
  assert.equal(nextScheme("auto"), "light");
  assert.equal(nextScheme("light"), "dark");
  assert.equal(nextScheme("dark"), "auto");
});

test("linked_tabs_switch_together", () => {
  const groups = [
    ["Rust", "Python", "Node.js"],
    ["Python", "Node.js"],
    ["C", "C++"],
  ];
  assert.deepEqual(linkedTabs(groups, "Python"), [1, 0, null]);
  assert.deepEqual(linkedTabs(groups, "C++"), [null, null, 1]);
  assert.deepEqual(linkedTabs(groups, "Go"), [null, null, null]);
  assert.deepEqual(linkedTabs([], "Rust"), []);
});

test("current_section_is_the_last_one_above_the_fold", () => {
  const sections = [
    { id: "a", top: -400 },
    { id: "b", top: -20 },
    { id: "c", top: 64 },
    { id: "d", top: 500 },
  ];
  assert.equal(currentSection(sections, 64), "c");
  assert.equal(currentSection(sections, 63), "b");
  assert.equal(currentSection(sections, 0), "b");
  assert.equal(currentSection(sections.slice(2), 10), null);
  assert.equal(currentSection([], 10), null);
});

test("state_works_without_storage_or_a_page", () => {
  assert.equal(scheme.value, "auto");
  assert.equal(query.value, "");
  assert.deepEqual(dismissed.value, []);
  assert.equal(consent.value, null);
  scheme.value = nextScheme(scheme.value);
  assert.equal(scheme.value, "light");
});
