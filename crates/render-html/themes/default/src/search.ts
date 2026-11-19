// `<lsd-search>`: a search button in the header, opening a dialog that
// searches the site. The page gives the index's URL (`data-index`) and the
// worker's (`data-worker`); the worker (search-worker.ts) loads the index
// on first use and answers queries. `/` or Ctrl+K (Cmd+K) opens the dialog,
// unless the reader is typing in a field; Escape closes it and focus goes
// back where it was. Results are built as DOM nodes from text segments,
// never from HTML. The query is the `query` signal (state.ts).

import { label } from "./labels.js";
import { query } from "./state.js";
import type { Item } from "./search-worker.js";

const SVG = "http://www.w3.org/2000/svg";

/** Whether `target` takes typing (so `/` there is a character). */
function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || target.closest("input, textarea, select, [contenteditable]") !== null;
}

/** Whether `event` is the shortcut that opens search. */
export function isShortcut(event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey">): boolean {
  if (event.altKey) return false;
  if (event.key === "/") return !event.ctrlKey && !event.metaKey;
  return event.key.toLowerCase() === "k" && (event.ctrlKey || event.metaKey);
}

/**
 * The result made active by an arrow key (`step`: 1 down, -1 up) from
 * `active` (-1: none) among `count`: wrapping around, the first (down) or
 * the last (up) from none; -1 when there are none.
 */
export function moveActive(active: number, count: number, step: 1 | -1): number {
  if (count <= 0) return -1;
  if (active < 0) return step > 0 ? 0 : count - 1;
  return (active + step + count) % count;
}

/** The status line for `count` results. */
function status(count: number): string {
  if (count === 0) return label("search_none");
  if (count === 1) return label("search_one");
  return label("search_many").replace("{count}", String(count));
}

let next = 0;

/** Defines `<lsd-search>`. */
export function define(): void {
  if (customElements.get("lsd-search")) return;
  customElements.define(
    "lsd-search",
    class extends HTMLElement {
      #abort: AbortController | null = null;
      #worker: Worker | null = null;

      connectedCallback(): void {
        this.#abort?.abort();
        const index = this.dataset["index"];
        const workerUrl = this.dataset["worker"];
        if (!index || !workerUrl || typeof HTMLDialogElement === "undefined") return;
        const abort = new AbortController();
        this.#abort = abort;
        const signal = abort.signal;
        const id = `lsd-search-${next++}`;

        // The header's button.
        const button = document.createElement("button");
        button.type = "button";
        button.className = "lsd-search-button";
        button.setAttribute("aria-haspopup", "dialog");
        button.setAttribute("aria-keyshortcuts", "/ Control+K");
        const name = label("search");
        button.setAttribute("aria-label", name);
        button.title = `${name} (/)`;
        const svg = document.createElementNS(SVG, "svg");
        svg.setAttribute("class", "lsd-icon");
        svg.setAttribute("viewBox", "0 0 24 24");
        svg.setAttribute("aria-hidden", "true");
        const path = document.createElementNS(SVG, "path");
        path.setAttribute("d", "M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM15.5 15.5 20 20");
        svg.append(path);
        const text = document.createElement("span");
        text.className = "lsd-search-label";
        text.textContent = name;
        button.append(svg, text);

        // The dialog: a combobox and its listbox of results.
        const dialog = document.createElement("dialog");
        dialog.className = "lsd-search-dialog";
        dialog.setAttribute("aria-label", name);
        const form = document.createElement("form");
        form.method = "dialog";
        form.setAttribute("role", "search");
        const input = document.createElement("input");
        input.type = "search";
        input.className = "lsd-search-input";
        input.placeholder = label("search_placeholder");
        input.setAttribute("aria-label", name);
        input.setAttribute("role", "combobox");
        input.setAttribute("aria-autocomplete", "list");
        input.setAttribute("aria-expanded", "false");
        input.setAttribute("aria-controls", `${id}-results`);
        input.autocomplete = "off";
        input.spellcheck = false;
        const close = document.createElement("button");
        close.type = "button";
        close.className = "lsd-search-close";
        close.textContent = label("search_close");
        form.append(input, close);
        const live = document.createElement("p");
        live.className = "lsd-search-status";
        live.setAttribute("role", "status");
        const list = document.createElement("ul");
        list.className = "lsd-search-results";
        list.id = `${id}-results`;
        list.setAttribute("role", "listbox");
        list.setAttribute("aria-label", name);
        dialog.append(form, live, list);
        this.replaceChildren(button, dialog);

        let options: HTMLLIElement[] = [];
        let active = -1;
        const activate = (i: number): void => {
          options[active]?.classList.remove("lsd-active");
          options[active]?.setAttribute("aria-selected", "false");
          active = i;
          const option = options[active];
          if (option) {
            option.classList.add("lsd-active");
            option.setAttribute("aria-selected", "true");
            option.scrollIntoView({ block: "nearest" });
            input.setAttribute("aria-activedescendant", option.id);
          } else {
            input.removeAttribute("aria-activedescendant");
          }
        };

        // Result URLs are from the site root: the index is in `_lsdocs/`.
        const root = new URL("..", new URL(index, location.href));
        const show = (items: Item[]): void => {
          options = items.map((item, i) => {
            const option = document.createElement("li");
            option.id = `${id}-${i}`;
            option.setAttribute("role", "option");
            option.setAttribute("aria-selected", "false");
            const link = document.createElement("a");
            link.href = new URL(item.url, root).href;
            link.tabIndex = -1;
            const title = document.createElement("span");
            title.className = "lsd-search-title";
            title.textContent = item.section || item.title;
            link.append(title);
            if (item.section && item.section !== item.title) {
              const page = document.createElement("span");
              page.className = "lsd-search-page";
              page.textContent = item.title;
              link.append(page);
            }
            const excerpt = document.createElement("span");
            excerpt.className = "lsd-search-excerpt";
            for (const segment of item.excerpt) {
              if (segment.match) {
                const mark = document.createElement("mark");
                mark.textContent = segment.text;
                excerpt.append(mark);
              } else {
                excerpt.append(document.createTextNode(segment.text));
              }
            }
            link.append(excerpt);
            option.append(link);
            return option;
          });
          list.replaceChildren(...options);
          list.scrollTop = 0;
          active = -1;
          input.removeAttribute("aria-activedescendant");
          input.setAttribute("aria-expanded", String(options.length > 0));
          live.textContent = status(items.length);
        };
        const clear = (): void => {
          options = [];
          list.replaceChildren();
          input.removeAttribute("aria-activedescendant");
          input.setAttribute("aria-expanded", "false");
          live.textContent = "";
        };

        const worker = (): Worker | null => {
          if (this.#worker) return this.#worker;
          try {
            const created = new Worker(new URL(workerUrl, location.href), { type: "module" });
            created.addEventListener("message", (event: MessageEvent) => {
              const data = event.data as { type?: string; text?: string; items?: Item[] };
              if (data.type === "error") {
                clear();
                live.textContent = label("search_failed");
              } else if (data.type === "results" && data.text === input.value) {
                show(data.items ?? []);
              }
            });
            created.addEventListener("error", () => {
              live.textContent = label("search_failed");
            });
            created.postMessage({ type: "load", url: new URL(index, location.href).href });
            this.#worker = created;
          } catch {
            live.textContent = label("search_failed");
          }
          return this.#worker;
        };
        const ask = (): void => {
          const text = input.value;
          query.value = text;
          if (!text.trim()) {
            clear();
            return;
          }
          worker()?.postMessage({ type: "query", text });
        };

        let opener: Element | null = null;
        const open = (): void => {
          if (dialog.open) return;
          opener = document.activeElement;
          input.value = query.value;
          dialog.showModal();
          input.focus();
          input.select();
          ask();
        };

        button.addEventListener("click", open, { signal });
        close.addEventListener("click", () => dialog.close(), { signal });
        input.addEventListener("input", ask, { signal });
        input.addEventListener(
          "keydown",
          (event) => {
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              activate(moveActive(active, options.length, event.key === "ArrowDown" ? 1 : -1));
            } else if (event.key === "Escape") {
              // A search field would clear itself first: close at once.
              event.preventDefault();
              dialog.close();
            } else if (event.key === "Enter") {
              event.preventDefault();
              const link = (options[active] ?? options[0])?.querySelector("a");
              if (link) {
                dialog.close();
                link.click();
              }
            }
          },
          { signal },
        );
        // A click on a result follows its link; the dialog closes (the
        // link may only move within this page).
        list.addEventListener(
          "click",
          (event) => {
            if (event.target instanceof Element && event.target.closest("a")) dialog.close();
          },
          { signal },
        );
        // A click on the backdrop closes the dialog.
        dialog.addEventListener(
          "click",
          (event) => {
            if (event.target === dialog) dialog.close();
          },
          { signal },
        );
        dialog.addEventListener(
          "close",
          () => {
            if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
            opener = null;
          },
          { signal },
        );
        document.addEventListener(
          "keydown",
          (event) => {
            if (dialog.open || event.defaultPrevented || !isShortcut(event) || typing(event.target)) return;
            event.preventDefault();
            open();
          },
          { signal },
        );
        signal.addEventListener("abort", () => {
          if (dialog.open) dialog.close();
          this.#worker?.terminate();
          this.#worker = null;
        });
      }

      disconnectedCallback(): void {
        this.#abort?.abort();
        this.#abort = null;
        this.replaceChildren();
      }
    },
  );
}
