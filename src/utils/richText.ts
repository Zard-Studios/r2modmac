import DOMPurify from 'dompurify';
import { Marked } from 'marked';
import { attachmentLinesToVideos, isAllowedMediaUrl, isSafeLinkUrl, VIDEO_PREVIEW_SECONDS } from './richTextUrls';

/**
 * Turns markdown or HTML into HTML that is safe to put in the page. Every place
 * that shows formatted text goes through here, so they all follow the same
 * rules and none keeps a copy of them.
 *
 * Two modes, for two kinds of author:
 * - `announcement`: text the maintainer writes (announcements, release notes).
 *   Pictures and videos may only come from GitHub's own hosting, and inline
 *   styles are dropped. Line breaks follow GitHub's comments.
 * - `community`: text other people write (mod descriptions and changelogs).
 *   Pictures may come from any https site, since that is where authors host
 *   them, and inline styles stay because many descriptions centre their
 *   images with them. Videos still only play from GitHub's hosting, which is
 *   all the page's content security policy lets through.
 *
 * In both modes links only keep an address that can be opened safely, scripts,
 * frames, forms and style sheets are removed, and a video never starts alone.
 */
export type RichTextMode = 'announcement' | 'community';

/** The text to show: markdown, or HTML that is already made. */
export interface RichTextSource {
    markdown?: string;
    html?: string;
}

const parsers: Record<RichTextMode, Marked> = {
    announcement: new Marked({ gfm: true, breaks: true }),
    community: new Marked({ gfm: true, breaks: false }),
};

// One sanitizer per mode, each with its own hook, so neither leaks into the
// other nor into anything else that uses DOMPurify.
const sanitizers: Partial<Record<RichTextMode, ReturnType<typeof DOMPurify>>> = {};

function sanitizerFor(mode: RichTextMode): ReturnType<typeof DOMPurify> {
    const existing = sanitizers[mode];
    if (existing) return existing;
    const instance = DOMPurify(window);
    instance.addHook('afterSanitizeAttributes', (node) => {
        const tag = node.tagName;
        if (tag === 'A') {
            const href = node.getAttribute('href');
            if (href && !isSafeLinkUrl(href, mode === 'community')) node.removeAttribute('href');
            node.setAttribute('rel', 'noopener noreferrer');
        } else if (tag === 'IMG' || tag === 'VIDEO' || tag === 'SOURCE') {
            const src = node.getAttribute('src');
            // Pictures in a description come from wherever its author put them.
            // Anything that plays must come from GitHub.
            const checked = tag !== 'IMG' || mode === 'announcement';
            if (src && checked && !isAllowedMediaUrl(src)) node.removeAttribute('src');
            const poster = node.getAttribute('poster');
            if (poster && !isAllowedMediaUrl(poster)) node.removeAttribute('poster');
            if (tag === 'IMG') {
                node.setAttribute('loading', 'lazy');
                // Decode big pictures off the main thread so the panel stays smooth.
                node.setAttribute('decoding', 'async');
            }
            if (tag === 'VIDEO') {
                node.removeAttribute('autoplay');
                // Without this the box stays empty until play is pressed: the
                // web view then loads the length but not a first picture.
                const kept = node.getAttribute('src');
                if (kept && isAllowedMediaUrl(kept) && !kept.includes('#')) {
                    node.setAttribute('src', `${kept}#t=${VIDEO_PREVIEW_SECONDS}`);
                }
                node.setAttribute('controls', '');
                node.setAttribute('preload', 'metadata');
            }
        }
    });
    sanitizers[mode] = instance;
    return instance;
}

/** HTML made from a text that someone else wrote, made safe. */
export function sanitizeHtml(html: string, mode: RichTextMode): string {
    return sanitizerFor(mode).sanitize(html, {
        ADD_TAGS: ['video', 'source'],
        ADD_ATTR: ['controls', 'preload', 'playsinline', 'muted', 'loop', 'poster'],
        FORBID_TAGS: ['style', 'form', 'input', 'button', 'textarea', 'select', 'iframe', 'object', 'embed', 'link', 'meta'],
        FORBID_ATTR: mode === 'announcement' ? ['style'] : [],
    });
}

/** The checkboxes of a task list can't be kept as form controls; show their state. */
function taskBoxesAsSymbols(html: string): string {
    return html.replace(/<input\b[^>]*type="checkbox"[^>]*>/g, (box) => (/\bchecked\b/.test(box) ? '☑' : '☐'));
}

/** Markdown or HTML turned into HTML that is safe to put in the page. */
export function renderRichText(source: RichTextSource, mode: RichTextMode): string {
    if (source.html !== undefined) return sanitizeHtml(source.html, mode);
    const parsed = parsers[mode].parse(attachmentLinesToVideos(source.markdown ?? '')) as string;
    return sanitizeHtml(taskBoxesAsSymbols(parsed), mode);
}
