// `<lsd-copy>`: around a code block (`figure.lsd-code`), adds a button that
// copies the code, and announces the result in a live region.
import { label } from "./labels.js";
const SVG = "http://www.w3.org/2000/svg";
const ICON = "M9 9h11v11H9zM5 15H4V4h11v1";
const DONE = "M5 12l5 5L20 7";
/** How long the button shows that it copied, in milliseconds. */
const SHOWN = 2000;
/**
 * Puts `text` on the clipboard. Where the Clipboard API is missing or
 * refused (pages served over plain HTTP, embedded browsers), copies the
 * selected contents of `element` instead.
 */
async function write(text, element) {
    try {
        if (!navigator.clipboard)
            throw new Error("no Clipboard API");
        await navigator.clipboard.writeText(text);
    }
    catch {
        const selection = getSelection();
        if (!selection)
            throw new Error("no selection");
        const saved = [...Array(selection.rangeCount).keys()].map((i) => selection.getRangeAt(i));
        const range = document.createRange();
        range.selectNodeContents(element);
        selection.removeAllRanges();
        selection.addRange(range);
        try {
            // Deprecated, but the only way left.
            if (!document.execCommand("copy"))
                throw new Error("copy refused");
        }
        finally {
            selection.removeAllRanges();
            for (const r of saved)
                selection.addRange(r);
        }
    }
}
/** Defines `<lsd-copy>`. */
export function define() {
    if (customElements.get("lsd-copy"))
        return;
    customElements.define("lsd-copy", class extends HTMLElement {
        #abort = null;
        #added = [];
        connectedCallback() {
            this.#abort?.abort();
            const pre = this.querySelector("pre");
            if (!pre)
                return;
            const abort = new AbortController();
            this.#abort = abort;
            const button = document.createElement("button");
            button.type = "button";
            button.className = "lsd-copy-button";
            button.setAttribute("aria-label", label("copy"));
            button.title = label("copy");
            const svg = document.createElementNS(SVG, "svg");
            svg.setAttribute("class", "lsd-icon");
            svg.setAttribute("viewBox", "0 0 24 24");
            svg.setAttribute("aria-hidden", "true");
            const path = document.createElementNS(SVG, "path");
            path.setAttribute("d", ICON);
            svg.append(path);
            button.append(svg);
            // Announced by screen readers when its text changes.
            const status = document.createElement("span");
            status.className = "lsd-copy-status";
            status.setAttribute("role", "status");
            this.append(button, status);
            this.#added = [button, status];
            let timer;
            const show = (message, done) => {
                status.textContent = message;
                path.setAttribute("d", done ? DONE : ICON);
                button.classList.toggle("lsd-copied", done);
                clearTimeout(timer);
                timer = setTimeout(() => {
                    status.textContent = "";
                    path.setAttribute("d", ICON);
                    button.classList.remove("lsd-copied");
                }, SHOWN);
            };
            abort.signal.addEventListener("abort", () => clearTimeout(timer));
            button.addEventListener("click", () => {
                const text = (pre.textContent ?? "").replace(/\n$/, "");
                write(text, pre).then(() => abort.signal.aborted || show(label("copied"), true), () => abort.signal.aborted || show(label("copy_failed"), false));
            }, { signal: abort.signal });
        }
        disconnectedCallback() {
            this.#abort?.abort();
            this.#abort = null;
            for (const element of this.#added)
                element.remove();
            this.#added = [];
        }
    });
}
