import type { InstalledBuilds } from '../../types/electron';

interface PlatformPickerProps {
    value: 'windows' | 'mac';
    onChange: (platform: 'windows' | 'mac') => void;
    /** Builds of the game found on this machine, to tag the matching option. */
    installed?: InstalledBuilds;
}

function InstalledTag() {
    return (
        <span className="ml-1 rounded-full border border-emerald-400/30 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-semibold leading-none text-emerald-300">
            Installed
        </span>
    );
}

const WIN_ICON = (
    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
        <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.4H10.949M0 12.6h9.75v9.451L0 20.699M10.949 12.6H24V24l-12.9-1.801" />
    </svg>
);

const MAC_ICON = (
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="16" viewBox="0 0 384 512" fill="currentColor">
        <path d="M318.7 268.7c-.2-36.7 16.4-64.4 50-84.8-18.8-26.9-47.2-41.7-84.7-44.6-35.5-2.8-74.3 20.7-88.5 20.7-15 0-49.4-19.7-76.4-19.7C63.3 141.2 4 184.8 4 273.5q0 39.3 14.4 81.2c12.8 36.7 59 126.7 107.2 125.2 25.2-.6 43-17.9 75.8-17.9 31.8 0 48.3 17.9 76.4 17.9 48.6-.7 90.4-82.5 102.6-119.3-65.2-30.7-61.7-90-61.7-91.9zm-56.6-164.2c27.3-32.4 24.8-61.9 24-72.5-24.1 1.4-52 16.4-67.9 34.9-17.5 19.8-27.8 44.3-25.6 71.9 26.1 2 49.9-11.4 69.5-34.3z" />
    </svg>
);

/** Unified platform toggle — Windows vs MacOS. Use wherever platform selection is needed. */
export function PlatformPicker({ value, onChange, installed }: PlatformPickerProps) {
    const base = 'flex items-center justify-center gap-2 p-3 rounded-xl border-2 transition-all font-medium text-sm';
    const active = 'bg-blue-500/20 border-blue-500 text-white';
    const inactive = 'bg-gray-900/50 border-gray-700 text-gray-400 hover:border-gray-500 hover:text-gray-200 hover:bg-gray-800/50';

    return (
        <div className="space-y-2">
            <label className="text-sm font-medium text-gray-400">Platform Compatibility</label>
            <div className="grid grid-cols-2 gap-3">
                <button
                    type="button"
                    onClick={() => onChange('windows')}
                    className={`${base} ${value === 'windows' ? active : inactive}`}
                >
                    {WIN_ICON}
                    Windows
                    {installed?.windows && <InstalledTag />}
                </button>
                <button
                    type="button"
                    onClick={() => onChange('mac')}
                    className={`${base} ${value === 'mac' ? active : inactive}`}
                >
                    {MAC_ICON}
                    MacOS
                    {installed?.mac && <InstalledTag />}
                </button>
            </div>
        </div>
    );
}
