import { lazy, Suspense, useEffect, useMemo, useState } from 'react';
import { ArrowLeft, Bell, ChevronRight, X } from 'lucide-react';
import type { Notice } from '../types/electron';
import {
    cachedNotices,
    markNoticeSeen,
    NOTICE_OPEN_INTERVAL_MS,
    noticesEnabled,
    refreshNotices,
    seenNoticeIds,
    unreadCount,
} from '../utils/notices';

// The markdown code is only fetched when an announcement is opened.
const NoticeBody = lazy(() => import('./NoticeBody').then((module) => ({ default: module.NoticeBody })));

/** Preferences fires this when the announcements switch changes. */
export const NOTICES_SETTING_EVENT = 'r2modmac:notices-setting';

function storage(): Storage | null {
    try {
        return window.localStorage;
    } catch {
        return null;
    }
}

function formatDate(iso: string): string {
    const date = new Date(iso);
    return Number.isNaN(date.getTime()) ? '' : date.toLocaleDateString();
}

/**
 * The bell on the home screen. It lists the titles of the maintainer's
 * announcements; opening one shows its text. The "From Zard Studios" label is
 * written here, not taken from the announcement, and the backend only passes
 * on issues opened by the maintainer's own account.
 */
export function NoticesBell() {
    // `version` changes whenever the switch or the read list does, so the
    // values below are read again from storage.
    const [version, setVersion] = useState(0);
    const [notices, setNotices] = useState<Notice[]>(() => {
        const store = storage();
        return store && noticesEnabled(store) ? cachedNotices(store) : [];
    });
    const [open, setOpen] = useState(false);
    const [selectedId, setSelectedId] = useState<number | null>(null);

    const store = storage();
    const enabled = useMemo(() => (store ? noticesEnabled(store) : true), [store, version]);
    const unread = useMemo(() => (store ? unreadCount(notices, store) : 0), [store, notices, version]);
    const seen = useMemo(() => (store ? seenNoticeIds(store) : new Set<number>()), [store, version]);
    const selected = notices.find((notice) => notice.id === selectedId) ?? null;

    useEffect(() => {
        let cancelled = false;
        const load = async () => {
            const current = storage();
            if (!current) return;
            const shown = await refreshNotices(current, () => window.ipcRenderer.fetchNotices());
            if (cancelled) return;
            setNotices(shown);
            setVersion((value) => value + 1);
        };
        void load();
        const onSetting = () => void load();
        window.addEventListener(NOTICES_SETTING_EVENT, onSetting);
        return () => {
            cancelled = true;
            window.removeEventListener(NOTICES_SETTING_EVENT, onSetting);
        };
    }, []);

    useEffect(() => {
        if (!open) return;
        const onKey = (event: KeyboardEvent) => {
            if (event.key !== 'Escape') return;
            // One step back: from an announcement to the list, then out.
            if (selectedId !== null) setSelectedId(null);
            else setOpen(false);
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [open, selectedId]);

    if (!enabled) return null;

    const openPanel = () => {
        setSelectedId(null);
        setOpen(true);
        // Whoever opens the bell wants the latest, so look again if the last
        // check is a couple of minutes old, without waiting for the long interval.
        if (!store) return;
        void refreshNotices(store, () => window.ipcRenderer.fetchNotices(), Date.now(), NOTICE_OPEN_INTERVAL_MS).then((shown) => {
            setNotices(shown);
            setVersion((value) => value + 1);
        });
    };

    const closePanel = () => {
        setOpen(false);
        setSelectedId(null);
    };

    const openNotice = (notice: Notice) => {
        if (store) markNoticeSeen(notice, store);
        setVersion((value) => value + 1);
        setSelectedId(notice.id);
    };

    const openIssue = (url: string) => {
        void import('@tauri-apps/plugin-shell').then(({ open: openExternal }) => openExternal(url));
    };

    return (
        <>
            <button
                type="button"
                onClick={openPanel}
                className="relative h-[52px] w-[52px] bg-gray-800 border border-gray-700 rounded-xl hover:bg-gray-700 hover:border-gray-600 active:scale-95 transition-all text-gray-400 hover:text-white shadow-lg flex-none flex items-center justify-center"
                title={unread > 0 ? `${unread} new announcement${unread === 1 ? '' : 's'}` : 'Announcements'}
                aria-label="Announcements"
            >
                <Bell className="w-5 h-5" strokeWidth={2} aria-hidden="true" />
                {unread > 0 && (
                    <span className="absolute top-[13px] right-[13px] h-2 w-2 rounded-full bg-white ring-2 ring-gray-800" />
                )}
            </button>

            {open && (
                <div
                    className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 p-6"
                    onClick={closePanel}
                >
                    <div
                        role="dialog"
                        aria-label="Announcements"
                        className="flex max-h-[80vh] w-full max-w-xl flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-800 shadow-2xl"
                        onClick={(event) => event.stopPropagation()}
                    >
                        <div className="flex items-center justify-between gap-3 border-b border-gray-700/60 px-5 py-4">
                            <div className="flex min-w-0 items-center gap-2">
                                {selected && (
                                    <button
                                        type="button"
                                        onClick={() => setSelectedId(null)}
                                        className="-ml-1.5 rounded-lg p-1.5 text-gray-400 transition-colors hover:bg-gray-700 hover:text-white"
                                        aria-label="Back to the list"
                                    >
                                        <ArrowLeft className="h-5 w-5" aria-hidden="true" />
                                    </button>
                                )}
                                <h2 className="text-lg font-semibold text-white">Announcements</h2>
                            </div>
                            <button
                                type="button"
                                onClick={closePanel}
                                className="rounded-lg p-1.5 text-gray-400 transition-colors hover:bg-gray-700 hover:text-white"
                                aria-label="Close"
                            >
                                <X className="h-5 w-5" aria-hidden="true" />
                            </button>
                        </div>

                        <div className="overflow-y-auto">
                            {selected ? (
                                <article className="px-5 py-5">
                                    <p className="text-[11px] font-semibold uppercase tracking-widest text-gray-500">
                                        From Zard Studios
                                        {formatDate(selected.created_at) && ` · ${formatDate(selected.created_at)}`}
                                    </p>
                                    <h3 className="mt-1.5 text-xl font-semibold leading-snug text-white">{selected.title}</h3>
                                    <div className="mt-4">
                                        {selected.body ? (
                                            <Suspense fallback={null}>
                                                <NoticeBody markdown={selected.body} />
                                            </Suspense>
                                        ) : (
                                            <p className="text-sm text-gray-400">No more details.</p>
                                        )}
                                    </div>
                                    {selected.url && (
                                        <button
                                            type="button"
                                            onClick={() => openIssue(selected.url as string)}
                                            className="mt-5 text-[13px] font-medium text-blue-400 transition-colors hover:text-blue-300"
                                        >
                                            Open on GitHub
                                        </button>
                                    )}
                                </article>
                            ) : notices.length === 0 ? (
                                <p className="px-5 py-10 text-center text-sm text-gray-400">Nothing new :)</p>
                            ) : (
                                <ul className="divide-y divide-gray-700/50">
                                    {notices.map((notice) => {
                                        const isUnread = !seen.has(notice.id);
                                        return (
                                            <li key={notice.id}>
                                                <button
                                                    type="button"
                                                    onClick={() => openNotice(notice)}
                                                    className="flex w-full items-center gap-3 px-5 py-4 text-left transition-colors hover:bg-gray-700/40"
                                                >
                                                    <span
                                                        className={`h-2 w-2 flex-none rounded-full ${isUnread ? 'bg-white' : 'bg-transparent'}`}
                                                        aria-label={isUnread ? 'New' : undefined}
                                                    />
                                                    <span className="min-w-0 flex-1">
                                                        <span
                                                            className={`line-clamp-2 block text-[15px] font-medium leading-snug ${isUnread ? 'text-white' : 'text-gray-300'}`}
                                                        >
                                                            {notice.title}
                                                        </span>
                                                        {formatDate(notice.created_at) && (
                                                            <span className="mt-0.5 block text-xs text-gray-500">
                                                                {formatDate(notice.created_at)}
                                                            </span>
                                                        )}
                                                    </span>
                                                    <ChevronRight className="h-4 w-4 flex-none text-gray-500" aria-hidden="true" />
                                                </button>
                                            </li>
                                        );
                                    })}
                                </ul>
                            )}
                        </div>
                    </div>
                </div>
            )}
        </>
    );
}
