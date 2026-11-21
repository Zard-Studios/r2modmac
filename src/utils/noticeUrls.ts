/**
 * What an announcement may link to or show. The text comes from the
 * maintainer's own GitHub issue, but it still goes through these rules, so a
 * stolen account or a careless paste cannot send the app anywhere unexpected.
 */

/**
 * The instant of a video that is shown as its preview. Not zero: a video can
 * start a little after zero (the one this was found with has its first picture
 * at 0.066 s), and a web view shows nothing at an instant with no picture.
 */
export const VIDEO_PREVIEW_SECONDS = 0.1;

/** Pictures and videos come from GitHub's own hosting, nowhere else. */
export function isAllowedMediaUrl(raw: string): boolean {
    let url: URL;
    try {
        url = new URL(raw);
    } catch {
        return false;
    }
    if (url.protocol !== 'https:') return false;
    const host = url.hostname.toLowerCase();
    if (host === 'github.com') {
        return (
            url.pathname.startsWith('/user-attachments/') ||
            url.pathname.startsWith('/Zard-Studios/r2modmac/raw/')
        );
    }
    return host === 'githubusercontent.com' || host.endsWith('.githubusercontent.com');
}

/** Links open in the browser, and only over https (or as a mail address). */
export function isSafeLinkUrl(raw: string): boolean {
    try {
        const protocol = new URL(raw).protocol;
        return protocol === 'https:' || protocol === 'mailto:';
    } catch {
        return false;
    }
}

const ATTACHMENT_LINE =
    /^[ \t]*(https:\/\/github\.com\/user-attachments\/assets\/[0-9a-f-]{36})[ \t]*$/gim;

/**
 * GitHub writes an uploaded video into the issue as a bare link on its own
 * line, and shows it as a player. Do the same here.
 */
export function attachmentLinesToVideos(markdown: string): string {
    return markdown.replace(
        ATTACHMENT_LINE,
        (_match, url: string) =>
            `<video controls preload="metadata" src="${url}"><a href="${url}">Open the video</a></video>`,
    );
}
