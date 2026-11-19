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

/** A label's name. */
export type Label = keyof typeof english;

let page: Record<string, unknown> | null = null;

/** The label `name`, from the page if it has it. */
export function label(name: Label): string {
  if (page === null) {
    page = {};
    try {
      const json = globalThis.document?.getElementById("lsd-labels")?.textContent;
      const value: unknown = json ? JSON.parse(json) : null;
      if (value && typeof value === "object") page = value as Record<string, unknown>;
    } catch {
      // Not JSON: English.
    }
  }
  const value = page[name];
  return typeof value === "string" ? value : english[name];
}
