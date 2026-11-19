// The theme's shared state: signals, each persisted in `localStorage`
// under `lsd-<name>`. Storage can be missing or refuse access (private
// modes, blocked cookies, Node.js), so every read and write is guarded and
// the state then lives for the page only.

import { effect, signal, type Signal } from "./vendor/signals-core.mjs";

/** A color scheme: the system's ("auto"), light or dark. */
export type Scheme = "auto" | "light" | "dark";

/** How a signal's value is stored: as a string, and back (`null`: invalid). */
interface Codec<T> {
  parse(raw: string): T | null;
  /** `null` removes the key. */
  format(value: T): string | null;
}

function storage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

function read(key: string): string | null {
  try {
    return storage()?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function write(key: string, raw: string | null): void {
  try {
    const store = storage();
    if (raw === null) {
      store?.removeItem(key);
    } else {
      store?.setItem(key, raw);
    }
  } catch {
    // Full or refused: the value lasts for this page.
  }
}

/**
 * A signal persisted under `lsd-<name>`: its stored value, else `initial`.
 * Changes are written back (not the initial value, so a site's default can
 * change later), and changes made in other tabs are taken up.
 */
function persisted<T>(name: string, initial: T, codec: Codec<T>): Signal<T> {
  const key = `lsd-${name}`;
  const raw = read(key);
  const stored = raw === null ? null : codec.parse(raw);
  const state = signal<T>(stored ?? initial);
  // What storage holds, as far as the signal is concerned: loading writes
  // nothing, not even over an invalid value.
  let last = stored === null ? codec.format(initial) : raw;
  effect(() => {
    const next = codec.format(state.value);
    if (next !== last) {
      last = next;
      write(key, next);
    }
  });
  globalThis.addEventListener?.("storage", (event: StorageEvent) => {
    if (event.key !== key) return;
    last = event.newValue;
    const value = event.newValue === null ? null : codec.parse(event.newValue);
    state.value = value ?? initial;
  });
  return state;
}

const text: Codec<string> = { parse: (raw) => raw, format: (value) => value };

/**
 * The page's scheme before the reader picks one: the theme's setting,
 * which the page's `data-lsd-scheme` holds unless a stored one replaced it
 * (and then the stored one is used anyway).
 */
function defaultScheme(): Scheme {
  const value = globalThis.document?.documentElement.dataset["lsdScheme"];
  return isScheme(value) ? value : "auto";
}

function isScheme(value: unknown): value is Scheme {
  return value === "auto" || value === "light" || value === "dark";
}

/** The color scheme the reader picked (`lsd-palette`). */
export const scheme: Signal<Scheme> = persisted<Scheme>("scheme", defaultScheme(), {
  parse: (raw) => (isScheme(raw) ? raw : null),
  format: (value) => value,
});

/** The search query (`lsd-search`). */
export const query: Signal<string> = persisted("query", "", text);

/** The ids of the banners the reader dismissed. */
export const dismissed: Signal<string[]> = persisted<string[]>("dismissed", [], {
  parse: (raw) => {
    try {
      const value: unknown = JSON.parse(raw);
      return Array.isArray(value) && value.every((v) => typeof v === "string") ? value : null;
    } catch {
      return null;
    }
  },
  format: (value) => (value.length ? JSON.stringify(value) : null),
});

/** The reader's answer to a consent question: `null` until they give one. */
export const consent: Signal<boolean | null> = persisted<boolean | null>("consent", null, {
  parse: (raw) => (raw === "true" ? true : raw === "false" ? false : null),
  format: (value) => (value === null ? null : String(value)),
});

// The page follows the scheme (the inline script in the page's head set
// it before first paint, from the same key).
if (globalThis.document) {
  effect(() => {
    document.documentElement.dataset["lsdScheme"] = scheme.value;
  });
}
