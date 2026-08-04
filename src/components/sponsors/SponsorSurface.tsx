import { useEffect, useRef, useState } from 'react';
import type { SponsorMessage } from '../../types/electron';

export type SponsorPlacement = 'catalog-support' | 'profile-selector-support';

interface SponsorSurfaceProps {
    placement: SponsorPlacement;
    visible: boolean;
    className?: string;
}

/**
 * A single, product-owned sponsor line. It requests only after the surface has
 * stayed visible briefly, then keeps that exact line while it is faded out at a
 * scroll boundary. This prevents prefetching or counting an unseen placement.
 */
export function SponsorSurface({ placement, visible, className = '' }: SponsorSurfaceProps) {
    const [message, setMessage] = useState<SponsorMessage | null>(null);
    const [dismissed, setDismissed] = useState(false);
    const [enabled, setEnabled] = useState<boolean | null>(null);
    const requestedRef = useRef(false);
    const acknowledgedRef = useRef<string | null>(null);

    useEffect(() => {
        void window.ipcRenderer.getSettings()
            .then((settings) => setEnabled(settings.sponsored_messages_enabled !== false))
            .catch(() => setEnabled(false));

        const onPreferenceChange = (event: Event) => {
            const nextEnabled = (event as CustomEvent<{ enabled?: boolean }>).detail?.enabled === true;
            setEnabled(nextEnabled);
            if (!nextEnabled) {
                setMessage(null);
                setDismissed(false);
                requestedRef.current = false;
                acknowledgedRef.current = null;
            } else {
                requestedRef.current = false;
                setDismissed(false);
            }
        };
        window.addEventListener('r2modmac:sponsor-preferences', onPreferenceChange);
        return () => window.removeEventListener('r2modmac:sponsor-preferences', onPreferenceChange);
    }, []);

    useEffect(() => {
        if (enabled !== true || !visible || dismissed || message || requestedRef.current) return;

        const timeoutId = window.setTimeout(() => {
            requestedRef.current = true;
            void window.ipcRenderer.requestSponsor(placement)
                .then((candidate) => setMessage(candidate))
                .catch(() => undefined);
        }, 900);

        return () => window.clearTimeout(timeoutId);
    }, [dismissed, enabled, message, placement, visible]);

    useEffect(() => {
        if (!message || acknowledgedRef.current === message.id) return;
        const frame = window.requestAnimationFrame(() => {
            acknowledgedRef.current = message.id;
            void window.ipcRenderer.acknowledgeSponsorDisplay(message.id).catch(() => undefined);
        });
        return () => window.cancelAnimationFrame(frame);
    }, [message]);

    if (!message || dismissed || enabled !== true) return null;

    return (
        <div
            className={`pointer-events-none absolute inset-x-0 bottom-0 z-20 flex justify-center bg-gradient-to-t from-gray-900 via-gray-900/95 to-transparent px-[clamp(1rem,3vw,3.125rem)] pb-4 pt-12 transition-[opacity,transform] duration-300 ease-out ${visible ? 'translate-y-0 opacity-100' : 'translate-y-3 opacity-0'} ${className}`}
            aria-hidden={!visible}
        >
            <div className="pointer-events-auto flex w-full items-center gap-3 rounded-xl border border-gray-700/90 bg-gray-800/95 px-4 py-3 shadow-[0_12px_35px_rgba(0,0,0,0.28)]">
                <span className="shrink-0 border-r border-gray-700 pr-3 text-[10px] font-semibold uppercase tracking-[0.14em] text-gray-500">Sponsored</span>
                <p className="min-w-0 flex-1 truncate text-[15px] text-gray-200" title={message.message}>{message.message}</p>
                {message.url ? (
                    <button
                        type="button"
                        onClick={() => {
                            void import('@tauri-apps/plugin-shell').then(({ open }) => open(message.url!));
                        }}
                        className="shrink-0 rounded-lg border border-blue-400/25 bg-blue-500/10 px-3 py-1.5 text-xs font-semibold text-blue-300 transition-colors hover:border-blue-400/40 hover:bg-blue-500/20 hover:text-blue-200"
                    >
                        Learn more ↗
                    </button>
                ) : null}
                <button
                    type="button"
                    onClick={() => {
                        setDismissed(true);
                        void window.ipcRenderer.dismissSponsor(message.id).catch(() => undefined);
                    }}
                    className="-mr-1 -mt-1 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-gray-500 transition-colors hover:bg-gray-700 hover:text-gray-200"
                    aria-label="Dismiss sponsored message"
                    title="Dismiss"
                >
                    <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="m6 6 12 12M18 6 6 18" />
                    </svg>
                </button>
            </div>
        </div>
    );
}
