import { useEffect, useMemo, useRef, type MouseEvent } from 'react';
import { renderNoticeMarkdown } from '../utils/noticeMarkdown';
import { isSafeLinkUrl } from '../utils/noticeUrls';

/** What the browser's own error code for a media element means. */
const MEDIA_ERRORS: Record<number, string> = {
    1: 'stopped',
    2: 'network error',
    3: 'could not be decoded',
    4: 'format or address not accepted',
};

/**
 * An announcement's text, with its markdown shown: bold, strikethrough, lists,
 * tables, code, images and videos. Kept apart from the bell so the markdown
 * code is only loaded when someone opens an announcement.
 */
export function NoticeBody({ markdown }: { markdown: string }) {
    const html = useMemo(() => renderNoticeMarkdown(markdown), [markdown]);
    const root = useRef<HTMLDivElement>(null);

    useEffect(() => {
        const container = root.current;
        if (!container) return;

        // `error` doesn't bubble, so listen on the way down. A video that can't
        // play is replaced by a line that says why, and a link to open it.
        const onError = (event: Event) => {
            const video = event.target;
            if (!(video instanceof HTMLVideoElement)) return;
            const original = video.getAttribute('src');
            const note = document.createElement('p');
            note.className = 'my-3 rounded-lg bg-gray-900/60 px-3 py-2 text-[13px] text-gray-300';
            const code = video.error?.code;
            note.textContent = `This video can't be played here${code ? ` (${MEDIA_ERRORS[code] ?? `error ${code}`})` : ''}. `;
            if (original && isSafeLinkUrl(original)) {
                const link = document.createElement('a');
                link.href = original;
                link.textContent = 'Open it in your browser';
                note.appendChild(link);
            }
            video.replaceWith(note);
        };
        container.addEventListener('error', onError, true);

        return () => {
            container.removeEventListener('error', onError, true);
        };
    }, [html]);

    // A link must never replace the app's own page: open it in the browser.
    const onClick = (event: MouseEvent<HTMLDivElement>) => {
        const link = (event.target as HTMLElement).closest('a');
        if (!link) return;
        event.preventDefault();
        const href = link.getAttribute('href');
        if (href && isSafeLinkUrl(href)) {
            void import('@tauri-apps/plugin-shell').then(({ open }) => open(href));
        }
    };

    return (
        <div
            ref={root}
            onClick={onClick}
            className="prose prose-invert prose-sm max-w-none break-words
                       prose-headings:text-white prose-headings:mt-4 prose-headings:mb-2
                       prose-p:my-2 prose-ul:my-2 prose-li:my-0.5
                       prose-a:text-blue-400 hover:prose-a:text-blue-300
                       prose-code:rounded prose-code:bg-gray-900/60 prose-code:px-1 prose-code:py-0.5 prose-code:font-normal
                       prose-code:before:content-none prose-code:after:content-none
                       prose-img:rounded-lg prose-img:my-3 [&_video]:w-full [&_video]:rounded-lg [&_video]:my-3"
            dangerouslySetInnerHTML={{ __html: html }}
        />
    );
}
