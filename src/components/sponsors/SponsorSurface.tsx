import { useEffect, useRef, useState } from 'react';
import type { SponsorMessage } from '../../types/electron';

const INITIAL_REQUEST_DELAY_MS = 900;
const SPONSOR_ROTATION_INTERVAL_MS = 20_000;

// Screen routes mount separate sponsor surfaces. Keep the current line outside
// React so navigating never blanks or recounts an already visible sponsor.
let persistentMessage: SponsorMessage | null = null;
let persistentDismissed = false;
let persistentEnabled: boolean | null = null;
let persistentAcknowledgedId: string | null = null;
let persistentScale = 80;
let persistentOpacity = 80;

let persistentDismissCount = 0;
let persistentIsFakeAd = false;
let persistentDismissTimer: ReturnType<typeof setTimeout> | null = null;
let persistentFakeAdTimer: ReturnType<typeof setTimeout> | null = null;

const FAKE_SPONSOR_MESSAGE: SponsorMessage = {
    id: 'fake-sponsor-tip',
    sponsorName: 'r2modmac',
    message: 'Did you know? You can disable sponsored messages in Preferences.',
    url: undefined,
};

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
    const [scale, setScale] = useState(persistentScale);
    const [backgroundOpacity, setBackgroundOpacity] = useState(persistentOpacity);
    const acknowledgedRef = useRef<string | null>(persistentAcknowledgedId);

    useEffect(() => {
        void window.ipcRenderer.getSettings()
            .then((settings) => {
                persistentEnabled = settings.sponsored_messages_enabled !== false;
                persistentScale = settings.sponsored_messages_scale ?? 80;
                persistentOpacity = settings.sponsored_messages_background_opacity ?? 80;
                setEnabled(persistentEnabled);
                setScale(persistentScale);
                setBackgroundOpacity(persistentOpacity);
            })
            .catch(() => {
                persistentEnabled = false;
                setEnabled(false);
            });

        const onPreferenceChange = (event: Event) => {
            const detail = (event as CustomEvent<{ enabled?: boolean; scale?: number; opacity?: number }>).detail;
            const nextEnabled = detail?.enabled === true;
            if (typeof detail?.scale === 'number') { persistentScale = Math.min(100, Math.max(70, detail.scale)); setScale(persistentScale); }
            if (typeof detail?.opacity === 'number') { persistentOpacity = Math.min(100, Math.max(0, detail.opacity)); setBackgroundOpacity(persistentOpacity); }
            persistentEnabled = nextEnabled;
            setEnabled(nextEnabled);
        };
        window.addEventListener('r2modmac:sponsor-preferences', onPreferenceChange);
        return () => window.removeEventListener('r2modmac:sponsor-preferences', onPreferenceChange);
    }, []);

    useEffect(() => {
        if (!visible || dismissed || persistentIsFakeAd) return;

        let cancelled = false;
        let timeoutId: number | undefined;

        const requestNextSponsor = async () => {
            try {
                const candidate = await window.ipcRenderer.requestSponsor(placement);
                if (!cancelled && candidate && !persistentIsFakeAd) {
                    persistentMessage = candidate;
                    persistentDismissed = false;
                    setMessage((current) => current?.id === candidate.id ? current : candidate);
                    setDismissed(false);
                }
            } catch {
                // Sponsorship is optional; keep the current line and retry later.
            } finally {
                if (!cancelled && !persistentIsFakeAd) {
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
        if (!message || message.id === FAKE_SPONSOR_MESSAGE.id || acknowledgedRef.current === message.id) return;
        acknowledgedRef.current = message.id;
        persistentAcknowledgedId = message.id;
        void window.ipcRenderer.acknowledgeSponsorDisplay(message.id).catch(() => undefined);
    }, [message]);

    const handleDismiss = () => {
        if (!message) return;

        if (persistentIsFakeAd) {
            persistentIsFakeAd = false;
            persistentDismissCount = 0;
            persistentMessage = null;
            persistentDismissed = true;
            setDismissed(true);
            setMessage(null);

            if (persistentFakeAdTimer) {
                clearTimeout(persistentFakeAdTimer);
                persistentFakeAdTimer = null;
            }

            if (persistentDismissTimer) clearTimeout(persistentDismissTimer);
            persistentDismissTimer = setTimeout(() => {
                persistentDismissed = false;
                setDismissed(false);
            }, SPONSOR_ROTATION_INTERVAL_MS);
            return;
        }

        void window.ipcRenderer.dismissSponsor(message.id).catch(() => undefined);

        const nextCount = persistentDismissCount + 1;
        persistentDismissCount = nextCount;

        if (nextCount >= 3) {
            persistentIsFakeAd = true;
            persistentMessage = FAKE_SPONSOR_MESSAGE;
            persistentDismissed = false;
            setMessage(FAKE_SPONSOR_MESSAGE);
            setDismissed(false);

            if (persistentDismissTimer) {
                clearTimeout(persistentDismissTimer);
                persistentDismissTimer = null;
            }

            if (persistentFakeAdTimer) clearTimeout(persistentFakeAdTimer);
            persistentFakeAdTimer = setTimeout(() => {
                persistentIsFakeAd = false;
                persistentDismissCount = 0;
                persistentMessage = null;
                persistentDismissed = false;
                setMessage(null);
                setDismissed(false);
            }, 15_000);
        } else {
            persistentDismissed = true;
            persistentMessage = null;
            setDismissed(true);
            setMessage(null);

            const delayMs = nextCount * 15_000;
            if (persistentDismissTimer) clearTimeout(persistentDismissTimer);
            persistentDismissTimer = setTimeout(() => {
                persistentDismissed = false;
                setDismissed(false);
            }, delayMs);
        }
    };

    if (!message || dismissed) return null;

    const isVisible = visible && enabled === true;

    return (
        <div
            className={`pointer-events-none absolute inset-x-0 bottom-0 z-20 flex justify-center bg-gradient-to-t from-gray-900/90 via-gray-900/70 to-transparent px-4 pb-3 pt-8 transition-[opacity,transform] duration-300 ease-out ${isVisible ? 'translate-y-0 opacity-100' : 'translate-y-3 opacity-0'} ${className}`}
            style={!enabled ? { opacity: 0, pointerEvents: 'none' } : undefined}
            aria-hidden={!isVisible}
        >
            <div className={`pointer-events-auto flex w-auto max-w-[min(46rem,calc(100%-1rem))] min-w-0 origin-bottom items-center gap-2 rounded-lg border px-3 py-2 transition-[transform,background-color,border-color,box-shadow] duration-300 ease-out ${backgroundOpacity === 0 ? '' : 'backdrop-blur-[2px]'}`} style={{ transform: `scale(${scale / 100})`, backgroundColor: `rgb(var(--r2-gray-800) / calc(var(--r2-gray-800-alpha, 1) * ${backgroundOpacity / 100}))`, borderColor: `rgb(var(--r2-gray-700) / calc(var(--r2-gray-700-alpha, 1) * ${backgroundOpacity / 100}))`, boxShadow: backgroundOpacity === 0 ? 'none' : '0 10px 28px rgb(0 0 0 / 0.24)' }}>
                <span className="shrink-0 border-r border-gray-700 pr-2 text-[9px] font-semibold uppercase tracking-[0.14em] text-gray-500">Sponsored</span>
                <p className="min-w-0 max-w-[min(36rem,55vw)] truncate text-sm text-gray-200" title={message.message}>{message.message}</p>
                <div className="flex shrink-0 items-center gap-2">
                    {persistentIsFakeAd ? (
                        <button
                            type="button"
                            onClick={() => {
                                window.dispatchEvent(new CustomEvent('r2modmac:open-preferences'));
                            }}
                            className="flex h-7 items-center rounded-md border border-blue-500/30 bg-blue-500/10 px-2.5 text-[11px] font-semibold text-fg-accent transition-colors hover:border-blue-500/50 hover:bg-blue-500/20"
                        >
                            Preferences ↗
                        </button>
                    ) : message.url ? (
                        <button
                            type="button"
                            onClick={() => {
                                void import('@tauri-apps/plugin-shell').then(({ open }) => open(message.url!));
                            }}
                            className="flex h-7 items-center rounded-md border border-blue-500/30 bg-blue-500/10 px-2.5 text-[11px] font-semibold text-fg-accent transition-colors hover:border-blue-500/50 hover:bg-blue-500/20"
                        >
                            Learn more ↗
                        </button>
                    ) : null}
                    <button
                        type="button"
                        onClick={handleDismiss}
                        className="flex h-7 w-7 items-center justify-center rounded-md text-gray-400 transition-colors hover:bg-gray-700 hover:text-white"
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
