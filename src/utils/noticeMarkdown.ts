import DOMPurify from 'dompurify';
import { Marked } from 'marked';
import { attachmentLinesToVideos, isAllowedMediaUrl, isSafeLinkUrl } from './noticeUrls';

// Own instances, so the hooks and options here don't leak into the other
// places that already render markdown with the shared ones.
const markdown = new Marked({ gfm: true, breaks: true });
let purify: ReturnType<typeof DOMPurify> | null = null;

function sanitizer(): ReturnType<typeof DOMPurify> {
    if (purify) return purify;
    const instance = DOMPurify(window);
    instance.addHook('afterSanitizeAttributes', (node) => {
        const tag = node.tagName;
        if (tag === 'A') {
            const href = node.getAttribute('href');
            if (href && !isSafeLinkUrl(href)) node.removeAttribute('href');
            node.setAttribute('rel', 'noopener noreferrer');
        } else if (tag === 'IMG' || tag === 'VIDEO' || tag === 'SOURCE') {
            const src = node.getAttribute('src');
            if (src && !isAllowedMediaUrl(src)) node.removeAttribute('src');
            const poster = node.getAttribute('poster');
            if (poster && !isAllowedMediaUrl(poster)) node.removeAttribute('poster');
            if (tag === 'IMG') node.setAttribute('loading', 'lazy');
            if (tag === 'VIDEO') {
                node.removeAttribute('autoplay');
                node.setAttribute('controls', '');
                node.setAttribute('preload', 'metadata');
            }
        }
    });
    purify = instance;
    return instance;
}

/** An announcement's markdown as HTML that is safe to put in the page. */
export function renderNoticeMarkdown(source: string): string {
    const html = markdown.parse(attachmentLinesToVideos(source)) as string;
    return sanitizer().sanitize(html, {
        ADD_TAGS: ['video', 'source'],
        ADD_ATTR: ['controls', 'preload', 'playsinline', 'muted', 'loop', 'poster'],
        FORBID_TAGS: ['style', 'form', 'input', 'button', 'textarea', 'select', 'iframe', 'object', 'embed', 'link', 'meta'],
        FORBID_ATTR: ['style'],
    });
}
