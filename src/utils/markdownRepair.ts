/**
 * Some mod descriptions were exported from a word processor, which puts a
 * backslash in front of every character that markdown could read as
 * formatting. The description then says `\- first item` and `\[!\[image](...)`,
 * and shows those characters as they are, instead of a list and a picture.
 * These helpers undo that, only where it can't be what the author meant.
 */

const FENCE = /^\s*(```|~~~)/;

/**
 * Take the backslash away from the start of a line that was meant to be a list
 * item, a heading or a quote, and from an image that is also a link. Lines
 * inside a fenced code block are left alone, since a backslash there is code.
 */
export function repairEscapedMarkdown(markdown: string): string {
    let inFence = false;
    return markdown
        .split('\n')
        .map((line) => {
            if (FENCE.test(line)) {
                inFence = !inFence;
                return line;
            }
            if (inFence) return line;
            return line
                .replace(/^(\s*)\\([-*+>])(\s)/, '$1$2$3')
                .replace(/^(\s*)\\(#{1,6})(\s)/, '$1$2$3')
                .replace(/^(\s*)(\d{1,3})\\\.(\s)/, '$1$2.$3')
                .replace(/\\\[!\\\[([^\]\n]*)\]\(([^)\s]+)\)\]\(([^)\s]+)\)/g, '[![$1]($2)]($3)');
        })
        .join('\n');
}

/**
 * Whether HTML that a site already made from markdown still shows markdown
 * symbols: paragraphs that start with a list marker, or an image written out.
 * That means the site's own converter gave up, and the original text is worth
 * fetching to show it properly.
 */
export function looksLikeUnrenderedMarkdown(html: string): boolean {
    const markerParagraphs = html.match(/<p>\s*(?:[-*+]|\d{1,3}\.)\s/g) ?? [];
    return markerParagraphs.length >= 2 || /<p>\s*\[!\[/.test(html);
}
