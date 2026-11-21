import { useEffect, useMemo, useRef, type MouseEvent } from 'react';
import { renderRichText, type RichTextMode } from '../utils/richText';
import { isSafeLinkUrl, VIDEO_PREVIEW_SECONDS } from '../utils/richTextUrls';

/** What the browser's own error code for a media element means. */
const MEDIA_ERRORS: Record<number, string> = {
    1: 'stopped',
    2: 'network error',
    3: 'could not be decoded',
    4: 'format or address not accepted',
};

// Tailwind needs the class names written out in full, so they live here as
// plain strings rather than being put together from pieces.
const SHARED_CLASSES =
    'rich-text prose prose-invert prose-sm max-w-none break-words ' +
    'prose-a:text-fg-accent hover:prose-a:text-fg-accent ' +
    'prose-code:rounded prose-code:bg-black/25 prose-code:px-1 prose-code:py-0.5 prose-code:font-normal ' +
    'prose-code:before:content-none prose-code:after:content-none ' +
    'prose-img:rounded-lg [&_video]:w-full [&_video]:rounded-lg [&_video]:my-3';

/** Compact spacing for text in a dialog: no margin outside the first and last block. */
const ANNOUNCEMENT_CLASSES =
    '[&>:first-child]:mt-0 [&>:last-child]:mb-0 ' +
    'prose-headings:text-white prose-headings:mt-4 prose-headings:mb-2 ' +
    'prose-p:my-2 prose-ul:my-2 prose-li:my-0.5 prose-img:my-3';

/** A mod description keeps the usual spacing of a document. */
const COMMUNITY_CLASSES = 'overflow-hidden prose-headings:text-white';

/**
 * Make a paused video show its first picture instead of an empty box. Some web
 * views load the length of a video but no picture until it plays, so if there
 * is still none a moment after the length is known, play it for an instant
 * without sound and put it back at the start.
 */
function showFirstPicture(video: HTMLVideoElement) {
    if (video.dataset.primed) return;
    video.dataset.primed = 'true';
    if (video.currentTime < VIDEO_PREVIEW_SECONDS) video.currentTime = VIDEO_PREVIEW_SECONDS;
    window.setTimeout(() => {
        if (!video.isConnected || !video.paused || video.readyState >= 2) return;
        const wasMuted = video.muted;
        video.muted = true;
        video.dataset.priming = 'true';
        video
            .play()
            .then(() => {
                video.pause();
                video.currentTime = VIDEO_PREVIEW_SECONDS;
            })
            .catch(() => {})
            .finally(() => {
                video.muted = wasMuted;
                delete video.dataset.priming;
            });
    }, 800);
}

interface RichTextProps {
    /** Markdown to show. */
    markdown?: string;
    /** HTML that is already made, to show instead. */
    html?: string;
    mode: RichTextMode;
    /** Where relative pictures and links point to, for a README written for GitHub. */
    imageBase?: string;
    linkBase?: string;
}

/**
 * Formatted text, shown the same way everywhere: bold, strikethrough, lists,
 * tables, code, images and videos, with links that open in the browser. The
 * announcements, the update notes and the mod descriptions all use this, so a
 * change here changes them all. See `renderRichText` for what each mode allows.
 */
export function RichText({ markdown, html, mode, imageBase, linkBase }: RichTextProps) {
    const rendered = useMemo(
        () => renderRichText({ markdown, html }, mode, imageBase && linkBase ? { image: imageBase, link: linkBase } : undefined),
        [markdown, html, mode, imageBase, linkBase],
    );
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
            note.className = 'my-3 rounded-lg bg-black/25 px-3 py-2 text-[13px] text-gray-300';
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

        // The preview sits a moment after the start. When the reader presses
        // play for the first time, start from the very beginning.
        const onPlay = (event: Event) => {
            const video = event.target;
            if (!(video instanceof HTMLVideoElement) || video.dataset.priming || video.dataset.started) return;
            video.dataset.started = 'true';
            if (video.currentTime <= VIDEO_PREVIEW_SECONDS + 0.05) video.currentTime = 0;
        };
        container.addEventListener('play', onPlay, true);
        for (const video of container.querySelectorAll('video')) {
            if (video.readyState >= 1) showFirstPicture(video);
        }

        return () => {
            container.removeEventListener('error', onError, true);
            container.removeEventListener('loadedmetadata', onMetadata, true);
            container.removeEventListener('play', onPlay, true);
        };
    }, [rendered]);

    // A link must never replace the app's own page: open it in the browser.
    const onClick = (event: MouseEvent<HTMLDivElement>) => {
        const link = (event.target as HTMLElement).closest('a');
        if (!link) return;
        event.preventDefault();
        const href = link.getAttribute('href');
        if (href && isSafeLinkUrl(href, mode === 'community')) {
            void import('@tauri-apps/plugin-shell').then(({ open }) => open(href));
        }
    };

    return (
        <div
            ref={root}
            onClick={onClick}
            className={`${SHARED_CLASSES} ${mode === 'announcement' ? ANNOUNCEMENT_CLASSES : COMMUNITY_CLASSES}`}
            dangerouslySetInnerHTML={{ __html: rendered }}
        />
    );
}
