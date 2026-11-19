// `<lsd-tabs>`: a tab set from its `.lsd-tab[data-label]` panels, following
// the WAI-ARIA tabs pattern (arrow keys, Home and End move between the
// tabs and show their panel). Tab sets are linked: choosing a label shows
// it in every tab set of the page that has one.
/**
 * For each group of labels (a tab set's), the index of `label` in it, or
 * `null` when it has none (and keeps the tab it shows).
 */
export function linkedTabs(groups, label) {
    return groups.map((labels) => {
        const index = labels.indexOf(label);
        return index < 0 ? null : index;
    });
}
let sets = 0;
/** Shows `label` in every tab set of the page, keeping `from` in place. */
function choose(from, label) {
    const all = [...document.querySelectorAll("lsd-tabs")].filter((set) => "show" in set);
    const before = from.getBoundingClientRect().top;
    linkedTabs(all.map((set) => set.labels()), label).forEach((index, i) => {
        if (index !== null)
            all[i]?.show(index);
    });
    // Panels above may have changed height: scroll so that the tab set the
    // reader used stays where it was.
    const moved = from.getBoundingClientRect().top - before;
    if (moved)
        window.scrollBy({ top: moved, behavior: "instant" });
}
/** Defines `<lsd-tabs>`. */
export function define() {
    if (customElements.get("lsd-tabs"))
        return;
    customElements.define("lsd-tabs", class extends HTMLElement {
        #abort = null;
        #tabs = [];
        #panels = [];
        /** Each panel's attributes before, to restore them. */
        #saved = [];
        #list = null;
        labels() {
            return this.#panels.map((panel) => panel.dataset["label"] ?? "");
        }
        show(index, focus = false) {
            this.#tabs.forEach((tab, i) => {
                const selected = i === index;
                tab.setAttribute("aria-selected", String(selected));
                tab.tabIndex = selected ? 0 : -1;
                const panel = this.#panels[i];
                if (panel)
                    panel.hidden = !selected;
            });
            if (focus)
                this.#tabs[index]?.focus();
        }
        connectedCallback() {
            this.disconnectedCallback();
            const panels = [...this.children].filter((child) => child instanceof HTMLElement && child.matches(".lsd-tab[data-label]"));
            if (!panels.length)
                return;
            const abort = new AbortController();
            this.#abort = abort;
            const id = `lsd-tabs-${++sets}`;
            const list = document.createElement("div");
            list.className = "lsd-tablist";
            list.setAttribute("role", "tablist");
            this.#panels = panels;
            this.#tabs = panels.map((panel, i) => {
                for (const name of ["id", "role", "aria-label", "aria-labelledby", "tabindex", "hidden"]) {
                    this.#saved.push([panel, name, panel.getAttribute(name)]);
                }
                const tab = document.createElement("button");
                tab.type = "button";
                tab.className = "lsd-tab-button";
                tab.id = `${id}-tab-${i}`;
                tab.setAttribute("role", "tab");
                tab.textContent = panel.dataset["label"] ?? "";
                panel.id ||= `${id}-panel-${i}`;
                tab.setAttribute("aria-controls", panel.id);
                panel.setAttribute("role", "tabpanel");
                panel.removeAttribute("aria-label");
                panel.setAttribute("aria-labelledby", tab.id);
                panel.tabIndex = 0;
                list.append(tab);
                return tab;
            });
            this.prepend(list);
            this.#list = list;
            this.show(0);
            list.addEventListener("click", (event) => {
                const tab = event.target.closest("[role=tab]");
                const index = this.#tabs.indexOf(tab);
                const name = this.#panels[index]?.dataset["label"];
                if (index < 0 || name === undefined)
                    return;
                choose(this, name);
                // The tab itself, should the set have its label twice.
                this.show(index);
            }, { signal: abort.signal });
            list.addEventListener("keydown", (event) => {
                const current = this.#tabs.indexOf(event.target);
                if (current < 0)
                    return;
                const last = this.#tabs.length - 1;
                const next = event.key === "ArrowRight" ? (current === last ? 0 : current + 1)
                    : event.key === "ArrowLeft" ? (current === 0 ? last : current - 1)
                        : event.key === "Home" ? 0
                            : event.key === "End" ? last
                                : null;
                if (next === null)
                    return;
                event.preventDefault();
                const name = this.#panels[next]?.dataset["label"];
                if (name !== undefined)
                    choose(this, name);
                this.show(next, true);
            }, { signal: abort.signal });
        }
        disconnectedCallback() {
            this.#abort?.abort();
            this.#abort = null;
            this.#list?.remove();
            this.#list = null;
            for (const [panel, name, value] of this.#saved) {
                if (value === null)
                    panel.removeAttribute(name);
                else
                    panel.setAttribute(name, value);
            }
            this.#saved = [];
            this.#tabs = [];
            this.#panels = [];
        }
    });
}
