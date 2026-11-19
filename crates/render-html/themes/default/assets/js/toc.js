// `<lsd-toc>`: around a table of contents, marks the link to the section
// being read (`aria-current="true"` and the `lsd-active` class). An
// IntersectionObserver tells when a section's top crosses the fold (the
// top of the page below the header: the page's `scroll-padding-top`).
/**
 * The section being read: the last one (in page order) whose top is at or
 * above `fold`; `null` when none is.
 */
export function currentSection(sections, fold) {
    let current = null;
    for (const section of sections) {
        if (section.top > fold)
            break;
        current = section.id;
    }
    return current;
}
/** Defines `<lsd-toc>`. */
export function define() {
    if (customElements.get("lsd-toc"))
        return;
    customElements.define("lsd-toc", class extends HTMLElement {
        #abort = null;
        connectedCallback() {
            this.#abort?.abort();
            const abort = new AbortController();
            this.#abort = abort;
            const links = new Map();
            const targets = [];
            for (const link of this.querySelectorAll('a[href^="#"]')) {
                let id;
                try {
                    id = decodeURIComponent(link.hash.slice(1));
                }
                catch {
                    continue;
                }
                const section = document.getElementById(id);
                if (!section || links.has(id))
                    continue;
                links.set(id, link);
                // A section's heading: the section's own box crosses the fold
                // only on its way out.
                const heading = section.querySelector(":scope > :is(h1, h2, h3, h4, h5, h6)");
                targets.push({ id, element: heading ?? section });
            }
            if (!targets.length)
                return;
            // A pixel of slack: a link scrolls its section to the fold, give
            // or take rounding.
            const fold = () => (parseFloat(getComputedStyle(document.documentElement).scrollPaddingTop) || 0) + 1;
            let active = null;
            const update = () => {
                // At the end of the page, the last sections may never reach the
                // fold: the last one is current.
                const end = scrollY > 0 && innerHeight + scrollY >= document.documentElement.scrollHeight - 1;
                const id = end
                    ? (targets.at(-1)?.id ?? null)
                    : currentSection(targets.map(({ id, element }) => ({ id, top: element.getBoundingClientRect().top })), fold());
                const link = id === null ? null : (links.get(id) ?? null);
                if (link === active)
                    return;
                active?.removeAttribute("aria-current");
                active?.classList.remove("lsd-active");
                link?.setAttribute("aria-current", "true");
                link?.classList.add("lsd-active");
                active = link;
            };
            // The observed area starts at the fold, so a heading crossing
            // it changes its intersection.
            const observer = new IntersectionObserver(update, {
                rootMargin: `-${Math.round(fold())}px 0px 0px 0px`,
                threshold: [0, 1],
            });
            for (const { element } of targets)
                observer.observe(element);
            // A jump past whole sections (a link to the middle of a long one)
            // may cross no observed edge.
            addEventListener("scrollend", update, { signal: abort.signal, passive: true });
            abort.signal.addEventListener("abort", () => {
                observer.disconnect();
                active?.removeAttribute("aria-current");
                active?.classList.remove("lsd-active");
            });
            update();
        }
        disconnectedCallback() {
            this.#abort?.abort();
            this.#abort = null;
        }
    });
}
