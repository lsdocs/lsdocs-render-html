// The default theme's pure helpers, on the compiled modules (assets/js).
// Run with `nur test theme`.

import assert from "node:assert/strict";
import test from "node:test";

import { gradient, numbers, parseLines, runs } from "../assets/js/lines.js";
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

test("emphasized_lines_parse_within_the_block", () => {
  assert.deepEqual(parseLines("1,3-4", 10), [1, 3, 4]);
  assert.deepEqual(parseLines(" 4-3 , 2,2, 9-12", 10), [2, 9, 10]);
  assert.deepEqual(parseLines("0-1,x,1-2-3,", 5), [1]);
  assert.deepEqual(parseLines("", 5), []);
});

test("emphasized_lines_paint_one_band_per_run", () => {
  assert.deepEqual(runs([1, 3, 4, 5, 8]), [[1, 1], [3, 5], [8, 8]]);
  assert.deepEqual(runs([]), []);
  assert.equal(
    gradient([2, 3]),
    "linear-gradient(transparent, transparent calc(var(--lsd-code-pad) + 1lh), " +
      "var(--lsd-code-hl) calc(var(--lsd-code-pad) + 1lh), " +
      "var(--lsd-code-hl) calc(var(--lsd-code-pad) + 3lh), " +
      "transparent calc(var(--lsd-code-pad) + 3lh), transparent)",
  );
});

test("line_numbers_count_from_the_start", () => {
  assert.equal(numbers(3, 5), "5\n6\n7");
  assert.equal(numbers(1, 1), "1");
});
