// Line numbers and emphasized lines of a code block (`figure.lsd-code`):
// the body writer leaves them as data (`data-linenos`, `data-line-start`,
// `data-lines`), as CSS cannot address a line of a `pre`. This adds a
// gutter of numbers beside the `pre` and paints the emphasized lines as
// bands of its background (`--lsd-code-lines`, a gradient in `lh`), so the
// code's own markup (and what the copy button copies) is left as it is.

/**
 * The lines a spec such as `1,3-4` names (1-based, within the block's
 * `count` lines), sorted and without repeats. Parts that are not a number
 * or a range of two are skipped.
 */
export function parseLines(spec: string, count: number): number[] {
  const lines = new Set<number>();
  for (const part of spec.split(",")) {
    const bounds = part.trim().split("-");
    if (bounds.length > 2 || bounds.some((b) => !/^\d+$/.test(b.trim()))) continue;
    const from = Number(bounds[0]);
    const to = Number(bounds[bounds.length - 1]);
    for (let n = Math.max(from, 1); n <= Math.min(to, count); n++) lines.add(n);
  }
  return [...lines].sort((a, b) => a - b);
}

/** Sorted lines as runs of consecutive ones: `[first, last]`, inclusive. */
export function runs(lines: number[]): Array<[number, number]> {
  const out: Array<[number, number]> = [];
  for (const n of lines) {
    const last = out[out.length - 1];
    if (last && n === last[1] + 1) last[1] = n;
    else out.push([n, n]);
  }
  return out;
}

/**
 * The background that paints `lines`: a gradient with a band of
 * `--lsd-code-hl` per run, from the top padding (`--lsd-code-pad`) down,
 * one `lh` per line.
 */
export function gradient(lines: number[]): string {
  const at = (n: number): string => `calc(var(--lsd-code-pad) + ${n}lh)`;
  const stops = runs(lines).flatMap(([first, last]) => [
    `transparent ${at(first - 1)}`,
    `var(--lsd-code-hl) ${at(first - 1)}`,
    `var(--lsd-code-hl) ${at(last)}`,
    `transparent ${at(last)}`,
  ]);
  return `linear-gradient(transparent, ${stops.join(", ")}, transparent)`;
}

/** The numbers of `count` lines from `start`, one per line. */
export function numbers(count: number, start: number): string {
  return Array.from({ length: count }, (_, i) => String(start + i)).join("\n");
}

/**
 * Draws the line numbers and emphasized lines `figure` asks for, around
 * its `pre`. Returns what undoes it.
 */
export function decorate(figure: HTMLElement, pre: HTMLElement): () => void {
  const linenos = figure.hasAttribute("data-linenos");
  const spec = figure.dataset["lines"];
  if (!linenos && spec === undefined) return () => {};
  const count = (pre.textContent ?? "").replace(/\n$/, "").split("\n").length;
  const lines = spec === undefined ? [] : parseLines(spec, count);
  if (lines.length > 0) figure.style.setProperty("--lsd-code-lines", gradient(lines));
  let gutter: HTMLElement | null = null;
  if (linenos) {
    const start = Number.parseInt(figure.dataset["lineStart"] ?? "", 10);
    gutter = document.createElement("span");
    gutter.className = "lsd-linenos";
    gutter.setAttribute("aria-hidden", "true");
    gutter.textContent = numbers(count, Number.isNaN(start) ? 1 : start);
    pre.before(gutter);
  }
  return () => {
    figure.style.removeProperty("--lsd-code-lines");
    gutter?.remove();
  };
}
