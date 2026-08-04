import { useEffect, useRef, useState } from 'react';
import type { SponsorMessage } from '../../types/electron';

const INITIAL_REQUEST_DELAY_MS = 900;
const SPONSOR_ROTATION_INTERVAL_MS = 15_000;

// Screen routes mount separate sponsor surfaces. Keep the current line outside
// React so navigating never blanks or recounts an already visible sponsor.
let persistentMessage: SponsorMessage | null = null;
let persistentDismissed = false;
let persistentEnabled: boolean | null = null;
let persistentAcknowledgedId: string | null = null;

export type SponsorPlacement = 'home-support' | 'catalog-support' | 'profile-selector-support';

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
    const [message, setMessage] = useState<SponsorMessage | null>(persistentMessage);
    const [dismissed, setDismissed] = useState(persistentDismissed);
    const [enabled, setEnabled] = useState<boolean | null>(persistentEnabled);
    const acknowledgedRef = useRef<string | null>(persistentAcknowledgedId);

    useEffect(() => {
        void window.ipcRenderer.getSettings()
            .then((settings) => {
                persistentEnabled = settings.sponsored_messages_enabled !== false;
                setEnabled(persistentEnabled);
            })
            .catch(() => {
                persistentEnabled = false;
                setEnabled(false);
            });

        const onPreferenceChange = (event: Event) => {
            const nextEnabled = (event as CustomEvent<{ enabled?: boolean }>).detail?.enabled === true;
            persistentEnabled = nextEnabled;
            setEnabled(nextEnabled);
            if (!nextEnabled) {
                persistentMessage = null;
                persistentDismissed = false;
                persistentAcknowledgedId = null;
                setMessage(null);
                setDismissed(false);
                acknowledgedRef.current = null;
            } else {
                persistentDismissed = false;
                setDismissed(false);
            }
        };
        window.addEventListener('r2modmac:sponsor-preferences', onPreferenceChange);
        return () => window.removeEventListener('r2modmac:sponsor-preferences', onPreferenceChange);
    }, []);

    useEffect(() => {
        if (enabled !== true || !visible || dismissed) return;

        let cancelled = false;
        let timeoutId: number | undefined;

        const requestNextSponsor = async () => {
            try {
                const candidate = await window.ipcRenderer.requestSponsor(placement);
                if (!cancelled && candidate) {
                    persistentMessage = candidate;
                    persistentDismissed = false;
                    setMessage((current) => current?.id === candidate.id ? current : candidate);
                    setDismissed(false);
                }
            } catch {
                // Sponsorship is optional; keep the current line and retry later.
            } finally {
                if (!cancelled) {
                    timeoutId = window.setTimeout(requestNextSponsor, SPONSOR_ROTATION_INTERVAL_MS);
                }
            }
        };

        timeoutId = window.setTimeout(requestNextSponsor, INITIAL_REQUEST_DELAY_MS);
        return () => {
            cancelled = true;
            if (timeoutId !== undefined) window.clearTimeout(timeoutId);
        };
    }, [dismissed, enabled, placement, visible]);

    useEffect(() => {
        if (!message || acknowledgedRef.current === message.id) return;
        const frame = window.requestAnimationFrame(() => {
            acknowledgedRef.current = message.id;
            persistentAcknowledgedId = message.id;
            void window.ipcRenderer.acknowledgeSponsorDisplay(message.id).catch(() => undefined);
        });
        return () => window.cancelAnimationFrame(frame);
    }, [message]);

    if (!message || dismissed || enabled !== true) return null;

    return (
        <div
            className={`pointer-events-none absolute inset-x-0 bottom-0 z-20 flex justify-center bg-gradient-to-t from-gray-900/90 via-gray-900/70 to-transparent px-4 pb-3 pt-8 transition-[opacity,transform] duration-300 ease-out ${visible ? 'translate-y-0 opacity-100' : 'translate-y-3 opacity-0'} ${className}`}
            aria-hidden={!visible}
        >
            <div className="pointer-events-auto flex w-auto max-w-[min(46rem,calc(100%-1rem))] min-w-0 items-center gap-2 rounded-lg border border-gray-700/90 bg-gray-800/80 px-3 py-2 backdrop-blur-[2px] shadow-[0_10px_28px_rgba(0,0,0,0.24)]">
                <span className="shrink-0 border-r border-gray-700 pr-2 text-[9px] font-semibold uppercase tracking-[0.14em] text-gray-500">Sponsored</span>
                <p className="min-w-0 max-w-[min(30rem,45vw)] truncate text-sm text-gray-200" title={message.message}>{message.message}</p>
                <div className="flex shrink-0 items-center gap-2">
                    {message.url ? (
                        <button
                            type="button"
                            onClick={() => {
                                void import('@tauri-apps/plugin-shell').then(({ open }) => open(message.url!));
                            }}
                            className="flex h-7 items-center rounded-md border border-blue-400/25 bg-blue-500/10 px-2.5 text-[11px] font-semibold text-blue-300 transition-colors hover:border-blue-400/40 hover:bg-blue-500/20 hover:text-blue-200"
                        >
                            Learn more ↗
                        </button>
                    ) : null}
                    <button
                        type="button"
                        onClick={() => {
                            persistentDismissed = true;
                            setDismissed(true);
                            void window.ipcRenderer.dismissSponsor(message.id).catch(() => undefined);
                        }}
                        className="flex h-7 w-7 items-center justify-center rounded-md text-gray-500 transition-colors hover:bg-gray-700 hover:text-gray-200"
                        aria-label="Dismiss sponsored message"
                        title="Dismiss"
                    >
                        <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="m6 6 12 12M18 6 6 18" />
                        </svg>
                    </button>
                </div>
            </div>
        </div>
    );
}
