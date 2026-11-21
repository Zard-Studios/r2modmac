import { useEffect, useMemo, useState } from 'react';
import { Bell, X } from 'lucide-react';
import type { Notice } from '../types/electron';
import {
    cachedNotices,
    markNoticesSeen,
    noticesEnabled,
    refreshNotices,
    unreadCount,
} from '../utils/notices';

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
 * The bell on the home screen. Notices are messages from the maintainer; the
 * "From Zard Studios" label is written here, not taken from the notice, and
 * the backend only passes on issues opened by the maintainer's own account.
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

    const store = storage();
    const enabled = useMemo(() => (store ? noticesEnabled(store) : true), [store, version]);
    const unread = useMemo(() => (store ? unreadCount(notices, store) : 0), [store, notices, version]);

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
            if (event.key === 'Escape') setOpen(false);
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [open]);

    if (!enabled) return null;

    const openPanel = () => {
        if (store) markNoticesSeen(notices, store);
        setVersion((value) => value + 1);
        setOpen(true);
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
                    <span className="absolute top-2.5 right-2.5 h-2.5 w-2.5 rounded-full bg-red-500 ring-2 ring-gray-800" />
                )}
            </button>

            {open && (
                <div
                    className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 p-6"
                    onClick={() => setOpen(false)}
                >
                    <div
                        role="dialog"
                        aria-label="Announcements"
                        className="flex max-h-[80vh] w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-gray-700 bg-gray-800 shadow-2xl"
                        onClick={(event) => event.stopPropagation()}
                    >
                        <div className="flex items-center justify-between gap-4 border-b border-gray-700/60 px-5 py-4">
                            <h2 className="text-lg font-semibold text-white">Announcements</h2>
                            <button
                                type="button"
                                onClick={() => setOpen(false)}
                                className="rounded-lg p-1.5 text-gray-400 transition-colors hover:bg-gray-700 hover:text-white"
                                aria-label="Close"
                            >
                                <X className="h-5 w-5" aria-hidden="true" />
                            </button>
                        </div>

                        <div className="overflow-y-auto">
                            {notices.length === 0 ? (
                                <p className="px-5 py-10 text-center text-sm text-gray-400">
                                    Nothing new :)
                                </p>
                            ) : (
                                <ul className="divide-y divide-gray-700/50">
                                    {notices.map((notice) => (
                                        <li key={notice.id} className="px-5 py-4">
                                            <p className="text-[11px] font-semibold uppercase tracking-widest text-gray-500">
                                                From Zard Studios
                                                {formatDate(notice.created_at) && ` · ${formatDate(notice.created_at)}`}
                                            </p>
                                            <h3 className="mt-1 text-[15px] font-medium text-white">{notice.title}</h3>
                                            {notice.body && (
                                                <p className="mt-2 whitespace-pre-wrap break-words text-[13px] leading-relaxed text-gray-300">
                                                    {notice.body}
                                                </p>
                                            )}
                                            {notice.url && (
                                                <button
                                                    type="button"
                                                    onClick={() => openIssue(notice.url as string)}
                                                    className="mt-3 text-[13px] font-medium text-blue-400 transition-colors hover:text-blue-300"
                                                >
                                                    Open on GitHub
                                                </button>
                                            )}
                                        </li>
                                    ))}
                                </ul>
                            )}
                        </div>
                    </div>
                </div>
            )}
        </>
    );
}
