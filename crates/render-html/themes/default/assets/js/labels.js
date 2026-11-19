// The theme's labels in the page's language: the page carries them as JSON
// (`<script type="application/json" id="lsd-labels">`, from the theme's
// `labels` table); English when it does not.
const english = {
    copy: "Copy to clipboard",
    copied: "Copied to clipboard",
    copy_failed: "Copy failed",
    scheme: "Color scheme",
    scheme_auto: "System",
    scheme_light: "Light",
    scheme_dark: "Dark",
};
let page = null;
/** The label `name`, from the page if it has it. */
export function label(name) {
    if (page === null) {
        page = {};
        try {
            const json = globalThis.document?.getElementById("lsd-labels")?.textContent;
            const value = json ? JSON.parse(json) : null;
            if (value && typeof value === "object")
                page = value;
        }
        catch {
            // Not JSON: English.
        }
    }
    const value = page[name];
    return typeof value === "string" ? value : english[name];
}
