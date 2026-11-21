import { useMemo, type MouseEvent } from 'react';
import { renderNoticeMarkdown } from '../utils/noticeMarkdown';
import { isSafeLinkUrl } from '../utils/noticeUrls';

/**
 * An announcement's text, with its markdown shown: bold, strikethrough, lists,
 * tables, code, images and videos. Kept apart from the bell so the markdown
 * code is only loaded when someone opens an announcement.
 */
export function NoticeBody({ markdown }: { markdown: string }) {
    const html = useMemo(() => renderNoticeMarkdown(markdown), [markdown]);

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
