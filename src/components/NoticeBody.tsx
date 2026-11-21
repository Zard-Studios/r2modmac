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
 * Make a paused video show its first picture instead of an empty box. Some web
 * views load the length of a video but no picture until it plays, so if there
 * is still none a moment after the length is known, play it for an instant
 * without sound and put it back at the start.
 */
function showFirstPicture(video: HTMLVideoElement) {
    if (video.dataset.primed) return;
    video.dataset.primed = 'true';
    if (video.currentTime === 0) video.currentTime = 0.001;
    window.setTimeout(() => {
        if (!video.isConnected || !video.paused || video.readyState >= 2) return;
        const wasMuted = video.muted;
        video.muted = true;
        video
            .play()
            .then(() => {
                video.pause();
                video.currentTime = 0;
            })
            .catch(() => {})
            .finally(() => {
                video.muted = wasMuted;
            });
    }, 800);
}

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
            const original = video.getAttribute('src')?.split('#')[0];
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

        // `loadedmetadata` doesn't bubble either. A video that already has its
        // length by now missed the event, so it is handled here.
        const onMetadata = (event: Event) => {
            if (event.target instanceof HTMLVideoElement) showFirstPicture(event.target);
        };
        container.addEventListener('loadedmetadata', onMetadata, true);
        for (const video of container.querySelectorAll('video')) {
            if (video.readyState >= 1) showFirstPicture(video);
        }

        return () => {
            container.removeEventListener('error', onError, true);
            container.removeEventListener('loadedmetadata', onMetadata, true);
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
