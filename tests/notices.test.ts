import assert from 'node:assert/strict';
import test from 'node:test';

import {
    NOTICE_CHECK_INTERVAL_MS,
    NOTICE_OPEN_INTERVAL_MS,
    cachedNotices,
    markNoticesSeen,
    refreshNotices,
    setNoticesEnabled,
    unreadCount,
    type NoticeStorage,
} from '../src/utils/notices.ts';

const issue = 'https://github.com/Zard-Studios/r2modmac/issues/63';

function notice(id: number, url: string | null = issue) {
    return { id, title: `Notice ${id}`, body: 'Text', url, created_at: '2026-11-21T10:00:00Z' };
}

function memory(): NoticeStorage {
    const data = new Map<string, string>();
    return {
        getItem: (key) => data.get(key) ?? null,
        setItem: (key, value) => void data.set(key, value),
    };
}

test('a fresh install asks GitHub and keeps the answer', async () => {
    const storage = memory();
    let calls = 0;
    const fetcher = async () => (calls++, [notice(1)]);

    const first = await refreshNotices(storage, fetcher, 1_000_000);
    const second = await refreshNotices(storage, fetcher, 1_000_000 + 60_000);

    assert.equal(calls, 1, 'the second launch reuses the stored notices');
    assert.deepEqual(first, second);
});

test('it asks again once the interval has passed', async () => {
    const storage = memory();
    let calls = 0;
    const fetcher = async () => (calls++, [notice(calls)]);

    await refreshNotices(storage, fetcher, 1_000_000);
    const later = await refreshNotices(storage, fetcher, 1_000_000 + NOTICE_CHECK_INTERVAL_MS);

    assert.equal(calls, 2);
    assert.equal(later[0].id, 2);
});

test('a failed request keeps the stored notices and is tried again next time', async () => {
    const storage = memory();
    await refreshNotices(storage, async () => [notice(1)], 1_000_000);
    let calls = 0;
    const failing = async (): Promise<ReturnType<typeof notice>[]> => {
        calls++;
        throw new Error('rate limited');
    };

    const afterFailure = await refreshNotices(storage, failing, 1_000_000 + NOTICE_CHECK_INTERVAL_MS);
    await refreshNotices(storage, failing, 1_000_000 + NOTICE_CHECK_INTERVAL_MS + 1000);

    assert.equal(afterFailure[0].id, 1);
    assert.equal(calls, 2, 'a failure does not count as a check');
});

test('with announcements off nothing is requested and nothing is shown', async () => {
    const storage = memory();
    setNoticesEnabled(storage, false);
    let calls = 0;

    const shown = await refreshNotices(storage, async () => (calls++, [notice(1)]), 1_000_000);

    assert.equal(calls, 0);
    assert.deepEqual(shown, []);
});

test('read notices stop counting, new ones count', async () => {
    const storage = memory();
    const notices = [notice(1), notice(2)];
    assert.equal(unreadCount(notices, storage), 2);

    markNoticesSeen(notices, storage);
    assert.equal(unreadCount(notices, storage), 0);
    assert.equal(unreadCount([...notices, notice(3)], storage), 1);
});

test('stored data that is not a list of notices is ignored', () => {
    const storage = memory();
    storage.setItem('r2modmac:notices', '{"not":"a list"}');
    assert.deepEqual(cachedNotices(storage), []);

    storage.setItem('r2modmac:notices', 'not json at all');
    assert.deepEqual(cachedNotices(storage), []);

    storage.setItem('r2modmac:notices', JSON.stringify([{ id: 'x', title: 'bad' }, null, 5, notice(9)]));
    assert.deepEqual(cachedNotices(storage).map((n) => n.id), [9]);
});

test('a link that is not an issue of this repository is dropped', async () => {
    const storage = memory();
    const [shown] = await refreshNotices(storage, async () => [notice(1, 'https://example.com/phish')], 1_000_000);

    assert.equal(shown.url, null);
});

test('opening the bell looks again after a short wait, not the long one', async () => {
    const storage = memory();
    let calls = 0;
    const fetcher = async () => (calls++, calls === 1 ? [] : [notice(63)]);

    await refreshNotices(storage, fetcher, 1_000_000);
    const tooSoon = await refreshNotices(storage, fetcher, 1_000_000 + 60_000, NOTICE_OPEN_INTERVAL_MS);
    const later = await refreshNotices(storage, fetcher, 1_000_000 + NOTICE_OPEN_INTERVAL_MS, NOTICE_OPEN_INTERVAL_MS);

    assert.deepEqual(tooSoon, []);
    assert.equal(calls, 2);
    assert.equal(later[0].id, 63);
});
