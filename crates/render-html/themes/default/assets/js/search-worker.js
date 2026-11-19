// The search worker: loads the site's index (`_lsdocs/search.json`) and
// answers queries. It is self-contained (no imports): a worker gets no
// import map, and the theme's modules have hashed names, so it could not
// import them. It is a module worker, so its pure functions are also
// exported, for the tests.
//
// Messages: `{ type: "load", url }` (the index's absolute URL; sent once,
// first), then `{ type: "query", text }`, each answered by
// `{ type: "results", text, items }` (or `{ type: "error" }` when the
// index cannot be loaded). Items carry their excerpt as segments, so the
// page builds text nodes from them, never HTML.
/** Words: runs of letters and digits. */
const WORD = /[\p{L}\p{N}]+/gu;
/** The words of `text`, lower-cased. Anything else (markup, regex characters) separates them. */
export function tokenize(text) {
    return Array.from(text.toLowerCase().matchAll(WORD), (m) => m[0]);
}
/** Indexes the sections of `file`. */
export function buildIndex(file) {
    const entries = [];
    const postings = new Map();
    const count = (tokens) => {
        const counts = new Map();
        for (const token of tokens)
            counts.set(token, (counts.get(token) ?? 0) + 1);
        return counts;
    };
    for (const page of file.pages ?? []) {
        for (const section of page.sections ?? []) {
            const entry = entries.length;
            entries.push({
                url: section.id ? `${page.url}#${section.id}` : page.url,
                title: page.title,
                section: section.title,
                text: section.text,
            });
            const titles = count(tokenize(section.title));
            const texts = count(tokenize(section.text));
            for (const token of new Set([...titles.keys(), ...texts.keys()])) {
                let list = postings.get(token);
                if (!list)
                    postings.set(token, (list = []));
                list.push({ entry, title: titles.get(token) ?? 0, text: texts.get(token) ?? 0 });
            }
        }
    }
    return { entries, tokens: [...postings.keys()].sort(), postings };
}
/** The first index in sorted `tokens` not below `term`. */
function lowerBound(tokens, term) {
    let low = 0;
    let high = tokens.length;
    while (low < high) {
        const mid = (low + high) >>> 1;
        if ((tokens[mid] ?? "") < term)
            low = mid + 1;
        else
            high = mid;
    }
    return low;
}
/**
 * The index's tokens a query term matches: those starting with it. A
 * one-character term matches only itself (a prefix that short matches
 * everything).
 */
function matching(index, term) {
    if (term.length < 2)
        return index.postings.has(term) ? [term] : [];
    const out = [];
    for (let i = lowerBound(index.tokens, term); i < index.tokens.length; i++) {
        const token = index.tokens[i] ?? "";
        if (!token.startsWith(term))
            break;
        out.push(token);
    }
    return out;
}
/** A section's score for one occurrence count: a title match outweighs any text. */
function score(posting, exact) {
    const weight = exact ? 1 : 0.5;
    return weight * ((posting.title > 0 ? 100 : 0) + (posting.text > 0 ? 1 + Math.log(posting.text) : 0));
}
/** How many characters an excerpt shows, about. */
const EXCERPT = 160;
/**
 * The sections matching every word of `query` (by prefix), best first:
 * matches in titles rank above matches in text. At most `limit`.
 */
export function search(index, query, limit = 20) {
    const terms = [...new Set(tokenize(query))];
    if (!terms.length)
        return [];
    let scores = null;
    for (const term of terms) {
        const next = new Map();
        for (const token of matching(index, term)) {
            for (const posting of index.postings.get(token) ?? []) {
                if (scores && !scores.has(posting.entry))
                    continue;
                const before = next.get(posting.entry) ?? scores?.get(posting.entry) ?? 0;
                next.set(posting.entry, before + score(posting, token === term));
            }
        }
        scores = next;
        if (!scores.size)
            return [];
    }
    return [...(scores ?? new Map())]
        .sort((a, b) => b[1] - a[1] || a[0] - b[0])
        .slice(0, limit)
        .flatMap(([i]) => {
        const entry = index.entries[i];
        if (!entry)
            return [];
        return [
            {
                url: entry.url,
                title: entry.title,
                section: entry.section,
                excerpt: excerpt(entry.text || entry.section, terms),
            },
        ];
    });
}
/**
 * About `EXCERPT` characters of `text` around the first word matching a
 * term (by prefix), as segments: the matching words, and the text
 * between them. "..." marks cut ends.
 */
export function excerpt(text, terms) {
    const words = Array.from(text.matchAll(WORD), (m) => ({
        start: m.index ?? 0,
        end: (m.index ?? 0) + m[0].length,
        match: terms.some((term) => {
            const word = m[0].toLowerCase();
            return term.length < 2 ? word === term : word.startsWith(term);
        }),
    }));
    const first = words.find((w) => w.match);
    let start = 0;
    if (first && first.start > EXCERPT / 3) {
        // Start at a word, a little before the match.
        const from = first.start - EXCERPT / 3;
        start = words.find((w) => w.start >= from)?.start ?? first.start;
    }
    let end = Math.min(text.length, start + EXCERPT);
    if (end < text.length) {
        // End after a word.
        end = words.filter((w) => w.end <= end && w.start >= start).at(-1)?.end ?? end;
    }
    const segments = [];
    const push = (part, match) => {
        if (!part)
            return;
        const last = segments.at(-1);
        if (last && last.match === match)
            last.text += part;
        else
            segments.push({ text: part, match });
    };
    if (start > 0)
        push("...", false);
    let at = start;
    for (const word of words) {
        if (!word.match || word.start < start || word.end > end)
            continue;
        push(text.slice(at, word.start), false);
        push(text.slice(word.start, word.end), true);
        at = word.end;
    }
    push(text.slice(at, end), false);
    if (end < text.length)
        push("...", false);
    return segments;
}
const scope = globalThis;
if (typeof scope.WorkerGlobalScope === "function" && scope.postMessage && scope.addEventListener) {
    const post = scope.postMessage.bind(scope);
    let loading = null;
    scope.addEventListener("message", (event) => {
        const message = event.data;
        if (!message || typeof message !== "object")
            return;
        const { type, url, text } = message;
        if (type === "load" && typeof url === "string" && !loading) {
            loading = fetch(url)
                .then((response) => {
                if (!response.ok)
                    throw new Error(`${response.status}`);
                return response.json();
            })
                .then(buildIndex);
            // Each query is answered with the error, too: the page shows it
            // whenever the user types, not only the first time.
            loading.catch(() => post({ type: "error" }));
        }
        else if (type === "query" && typeof text === "string" && loading) {
            loading.then((index) => post({ type: "results", text, items: search(index, text) }), () => post({ type: "error" }));
        }
    });
}
