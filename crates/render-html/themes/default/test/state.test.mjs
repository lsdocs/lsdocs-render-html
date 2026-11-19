// The theme's state with a (fake) `localStorage`: stored values are read,
// changes written, and bad or refused storage ignored. Each test file runs
// in its own process, so the fake is in place before state.js loads.

import assert from "node:assert/strict";
import test from "node:test";

const items = new Map([
  ["lsd-scheme", "dark"],
  ["lsd-dismissed", "not json"],
  ["lsd-consent", "true"],
]);
const writes = [];
Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => {
      if (key === "lsd-query") throw new Error("QuotaExceededError");
      writes.push([key, value]);
      items.set(key, value);
    },
    removeItem: (key) => {
      writes.push([key, null]);
      items.delete(key);
    },
  },
});

const { consent, dismissed, query, scheme } = await import("../assets/js/state.js");

test("stored_values_are_read_and_bad_ones_ignored", () => {
  assert.equal(scheme.value, "dark");
  assert.deepEqual(dismissed.value, []);
  assert.equal(consent.value, true);
  assert.deepEqual(writes, [], "loading writes nothing");
});

test("changes_are_written", () => {
  scheme.value = "light";
  dismissed.value = ["v2"];
  consent.value = null;
  assert.equal(items.get("lsd-scheme"), "light");
  assert.equal(items.get("lsd-dismissed"), '["v2"]');
  assert.equal(items.has("lsd-consent"), false);
});

test("refused_writes_keep_the_value", () => {
  query.value = "signals";
  assert.equal(query.value, "signals");
  assert.equal(items.has("lsd-query"), false);
});
