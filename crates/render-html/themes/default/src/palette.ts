// `<lsd-palette>`: a button in the header cycling the color scheme through
// the system's, light and dark. The choice is the `scheme` signal (state.ts),
// which sets the page's `data-lsd-scheme` and is remembered.

import { effect } from "./vendor/signals-core.mjs";
import { label } from "./labels.js";
import { scheme, type Scheme } from "./state.js";

/** The scheme after `current`: auto, light, dark, auto, ... */
export function nextScheme(current: Scheme): Scheme {
  switch (current) {
    case "auto":
      return "light";
    case "light":
      return "dark";
    case "dark":
      return "auto";
  }
}

const icons: Record<Scheme, string> = {
  // Half a disc: the system's.
  auto: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 3v18",
  // A sun.
  light:
    "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4",
  // A moon.
  dark: "M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5z",
};

const SVG = "http://www.w3.org/2000/svg";

/** Defines `<lsd-palette>`. */
export function define(): void {
  if (customElements.get("lsd-palette")) return;
  customElements.define(
    "lsd-palette",
    class extends HTMLElement {
      #abort: AbortController | null = null;

      connectedCallback(): void {
        this.#abort?.abort();
        const abort = new AbortController();
        this.#abort = abort;

        const button = document.createElement("button");
        button.type = "button";
        button.className = "lsd-palette-button";
        const svg = document.createElementNS(SVG, "svg");
        svg.setAttribute("class", "lsd-icon");
        svg.setAttribute("viewBox", "0 0 24 24");
        svg.setAttribute("aria-hidden", "true");
        const path = document.createElementNS(SVG, "path");
        svg.append(path);
        button.append(svg);
        this.replaceChildren(button);

        button.addEventListener(
          "click",
          () => {
            scheme.value = nextScheme(scheme.value);
          },
          { signal: abort.signal },
        );
        const stop = effect(() => {
          const current = scheme.value;
          const name = `${label("scheme")}: ${label(`scheme_${current}`)}`;
          button.setAttribute("aria-label", name);
          button.title = name;
          button.dataset["scheme"] = current;
          path.setAttribute("d", icons[current]);
        });
        abort.signal.addEventListener("abort", stop);
      }

      disconnectedCallback(): void {
        this.#abort?.abort();
        this.#abort = null;
        this.replaceChildren();
      }
    },
  );
}
