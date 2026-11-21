import type { Notice } from '../types/electron';

/**
 * Announcements from the maintainer: what is kept on this machine, which ones
 * were read, and when to ask GitHub again. The notices themselves come from
 * labelled issues (see `src-tauri/src/commands/notices.rs`).
 */

export type NoticeStorage = Pick<Storage, 'getItem' | 'setItem'>;

const CACHE_KEY = 'r2modmac:notices';
const SEEN_KEY = 'r2modmac:noticesSeen';
const CHECKED_KEY = 'r2modmac:noticesChecked';
const OFF_KEY = 'r2modmac:noticesOff';

/** One request every few hours at most, however often the app is opened. */
export const NOTICE_CHECK_INTERVAL_MS = 3 * 60 * 60 * 1000;

/** Opening the bell looks again sooner, but never more than this often. */
export const NOTICE_OPEN_INTERVAL_MS = 2 * 60 * 1000;

/** Notices link to their own issue and nowhere else. */
const ISSUE_URL_PREFIX = 'https://github.com/Zard-Studios/r2modmac/issues/';

function read(storage: NoticeStorage, key: string): string | null {
    try {
        return storage.getItem(key);
    } catch {
        return null;
    }
}

function write(storage: NoticeStorage, key: string, value: string): void {
    try {
        storage.setItem(key, value);
    } catch {
        // Storage can be blocked or full. The bell then works without memory.
    }
}

function parseList(raw: string | null): unknown[] {
    if (!raw) return [];
    try {
        const parsed: unknown = JSON.parse(raw);
        return Array.isArray(parsed) ? parsed : [];
    } catch {
        return [];
    }
}

/** Keep only well-formed notices, whatever the stored text looks like. */
function sanitize(items: unknown[]): Notice[] {
    const notices: Notice[] = [];
    for (const item of items) {
        if (typeof item !== 'object' || item === null) continue;
        const { id, title, body, url, created_at } = item as Record<string, unknown>;
        if (typeof id !== 'number' || typeof title !== 'string' || title.trim() === '') continue;
        notices.push({
            id,
            title,
            body: typeof body === 'string' ? body : '',
            url: typeof url === 'string' && url.startsWith(ISSUE_URL_PREFIX) ? url : null,
            created_at: typeof created_at === 'string' ? created_at : '',
        });
    }
    return notices;
}

export function noticesEnabled(storage: NoticeStorage): boolean {
    return read(storage, OFF_KEY) !== 'true';
}

export function setNoticesEnabled(storage: NoticeStorage, enabled: boolean): void {
    write(storage, OFF_KEY, enabled ? 'false' : 'true');
}

export function cachedNotices(storage: NoticeStorage): Notice[] {
    return sanitize(parseList(read(storage, CACHE_KEY)));
}

export function seenNoticeIds(storage: NoticeStorage): Set<number> {
    return new Set(parseList(read(storage, SEEN_KEY)).filter((id): id is number => typeof id === 'number'));
}

export function unreadCount(notices: Notice[], storage: NoticeStorage): number {
    const seen = seenNoticeIds(storage);
    return notices.filter((notice) => !seen.has(notice.id)).length;
}

/** An announcement counts as read once its text has been opened. */
export function markNoticeSeen(notice: Notice, storage: NoticeStorage): void {
    const seen = [...seenNoticeIds(storage), notice.id];
    // Old announcements get closed on GitHub, so the list needs no more room.
    write(storage, SEEN_KEY, JSON.stringify([...new Set(seen)].slice(-200)));
}

function checkIsDue(storage: NoticeStorage, now: number, interval: number): boolean {
    const last = Number(read(storage, CHECKED_KEY));
    return !Number.isFinite(last) || last <= 0 || now - last >= interval || last > now;
}

/**
 * The notices to show. Asks GitHub only when notices are on and the last check
 * is old enough. When the request fails, for instance offline or rate limited,
 * the notices already stored are kept and the next launch tries again.
 */
export async function refreshNotices(
    storage: NoticeStorage,
    fetchNotices: () => Promise<Notice[]>,
    now: number = Date.now(),
    interval: number = NOTICE_CHECK_INTERVAL_MS,
): Promise<Notice[]> {
    if (!noticesEnabled(storage)) return [];
    const cached = cachedNotices(storage);
    if (!checkIsDue(storage, now, interval)) return cached;
    try {
        const fresh = sanitize(await fetchNotices());
        write(storage, CACHE_KEY, JSON.stringify(fresh));
        write(storage, CHECKED_KEY, String(now));
        return fresh;
    } catch {
        return cached;
    }
}
