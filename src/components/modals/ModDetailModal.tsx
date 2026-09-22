import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ModSource, PackageVersion, Package } from '../../types/thunderstore';
import type { InstalledMod } from '../../types/profile';
import DOMPurify from 'dompurify';
import { LikeStat } from '../LikeStat';
import { marked } from 'marked';
import { packageIdentityKey } from '../../utils/modVersioning';

const getGithubRepo = (url: string | undefined): { owner: string; repo: string } | null => {
    if (!url) return null;
    const match = url.match(/github\.com\/([^/]+)\/([^/]+)/);
    if (!match) return null;
    const owner = match[1];
    let repo = match[2];
    if (repo.endsWith('.git')) {
        repo = repo.slice(0, -4);
    }
    repo = repo.split(/[?#]/)[0].replace(/\/$/, '');
    return { owner, repo };
};

interface ModDetailModalProps {
    pkg: Package;
    isOpen: boolean;
    onClose: () => void;
    onInstall: (version: PackageVersion) => void;
    onUpdate?: (version: PackageVersion) => void;
    onUninstall?: () => void;
    isInstalled: boolean;
    hasUpdate?: boolean;
    gameId: string;
    installedMods?: InstalledMod[];
    isBrowsing?: boolean;
    legacyInstallMode?: boolean;
    showDeprecatedWarning?: boolean;
}

type Tab = 'description' | 'changelog' | 'dependencies';

const sourceName = (source?: ModSource) => {
    if (source === 'hexium') return 'Hexium';
    if (source === 'outerwilds') return 'Outer Wilds Mods';
    return 'Thunderstore';
};

const versionChoiceKey = (version: PackageVersion) =>
    `${version.source || 'thunderstore'}:${version.uuid4}:${version.version_number}`;

function StoreLogo({ source, className = 'h-4 w-4' }: { source?: ModSource; className?: string }) {
    const name = sourceName(source);
    if (source === 'hexium') {
        return (
            <svg className={className} viewBox="0 0 32 32" role="img" aria-label={name}>
                <title>{name}</title>
                <path d="M16 2.5 28 9.25v13.5L16 29.5 4 22.75V9.25Z" fill="#6d28d9" stroke="#a78bfa" strokeWidth="1.5" />
                <path d="M10 9v14h4.2v-5.2h3.6V23H22V9h-4.2v5.1h-3.6V9Z" fill="white" />
            </svg>
        );
    }
    if (source === 'outerwilds') {
        return (
            <svg className={className} viewBox="0 0 32 32" role="img" aria-label={name}>
                <title>{name}</title>
                <circle cx="16" cy="16" r="13" fill="#d97706" />
                <path d="M16 7a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm0 4a5 5 0 1 1 0 10 5 5 0 0 1 0-10Z" fill="white" />
            </svg>
        );
    }
    return (
        <svg className={`${className} text-teal-400`} viewBox="0 0 1000 896" role="img" aria-label={name}>
            <title>{name}</title>
            <path
                d="M13.4223 496.845L209.485 838.17L300 650.202L200.99 477.966C189.992 458.897 189.992 436.945 200.99 417.779L324.555 202.755C335.561 183.611 354.447 172.666 376.421 172.675H442.857L314.286 462.366H473.143L257.143 881.384L690.941 361.014H557.588L648.593 172.675H808.03H900.762L1000 2.28882e-05H715.868H526.836H298.96C263.138 0.0084323 232.393 17.8324 214.461 48.9346L13.4223 398.9C-4.46781 430.078 -4.48036 465.827 13.4223 496.845ZM313.959 895.833H701.066C736.813 895.833 767.63 878.005 785.612 846.819L986.655 496.836C1004.44 465.827 1004.44 430.078 986.655 398.892L906.26 258.947H707.808L799.079 417.779C809.985 436.961 809.984 458.91 799.049 477.974L675.531 693.049C664.454 712.222 645.555 723.15 623.555 723.15H533.795L471.429 722.446L313.959 895.833Z"
                fill="currentColor"
            />
        </svg>
    );
}

function VersionSourcePicker({
    versions,
    selected,
    onChange,
}: {
    versions: PackageVersion[];
    selected: PackageVersion;
    onChange: (version: PackageVersion) => void;
}) {
    const [isOpen, setIsOpen] = useState(false);
    const rootRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!isOpen) return;
        const close = (event: PointerEvent) => {
            if (!rootRef.current?.contains(event.target as Node)) setIsOpen(false);
        };
        const closeOnEscape = (event: KeyboardEvent) => {
            if (event.key === 'Escape') setIsOpen(false);
        };
        document.addEventListener('pointerdown', close);
        document.addEventListener('keydown', closeOnEscape);
        return () => {
            document.removeEventListener('pointerdown', close);
            document.removeEventListener('keydown', closeOnEscape);
        };
    }, [isOpen]);

    return (
        <div ref={rootRef} className="relative inline-block w-fit max-w-full">
            <button
                type="button"
                onClick={() => setIsOpen(open => !open)}
                className="flex max-w-full items-center gap-2.5 rounded-lg border border-gray-700 bg-gray-800 px-2.5 py-1.5 text-sm text-gray-200 transition-colors hover:border-gray-600 focus:border-blue-500 focus:outline-none"
                aria-haspopup="listbox"
                aria-expanded={isOpen}
                aria-label={`Version ${selected.version_number} from ${sourceName(selected.source)}`}
            >
                <span className="flex items-center gap-2">
                    <StoreLogo source={selected.source} />
                    <span>v{selected.version_number}</span>
                </span>
                <svg className={`h-4 w-4 flex-shrink-0 text-gray-500 transition-transform ${isOpen ? 'rotate-180' : ''}`} viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                    <path fillRule="evenodd" d="M5.23 7.21a.75.75 0 0 1 1.06.02L10 11.19l3.71-3.96a.75.75 0 1 1 1.1 1.02l-4.25 4.54a.75.75 0 0 1-1.1 0L5.21 8.25a.75.75 0 0 1 .02-1.04Z" clipRule="evenodd" />
                </svg>
            </button>
            {isOpen && (
                <div className="absolute left-0 top-full z-30 mt-1 w-max min-w-full max-w-[min(22rem,calc(100vw-3rem))] overflow-hidden rounded-lg border border-gray-700 bg-gray-800 py-1 shadow-xl" role="listbox">
                    {versions.map(version => {
                        const active = versionChoiceKey(version) === versionChoiceKey(selected);
                        return (
                            <button
                                type="button"
                                key={versionChoiceKey(version)}
                                onClick={() => {
                                    onChange(version);
                                    setIsOpen(false);
                                }}
                                className={`flex w-full items-center gap-2 whitespace-nowrap px-3 py-2 text-left text-sm transition-colors ${active ? 'bg-blue-500/15 text-fg-accent' : 'text-gray-200 hover:bg-gray-700'}`}
                                role="option"
                                aria-selected={active}
                                title={sourceName(version.source)}
                            >
                                <StoreLogo source={version.source} />
                                <span>v{version.version_number}</span>
                                <span className="ml-auto pl-4 text-xs text-gray-500">{sourceName(version.source)}</span>
                            </button>
                        );
                    })}
                </div>
            )}
        </div>
    );
}

export function ModDetailModal({
    pkg,
    isOpen,
    onClose,
    onInstall,
    onUpdate,
    onUninstall,
    isInstalled,
    hasUpdate = false,
    gameId,
    installedMods = [],
    isBrowsing,
    legacyInstallMode = false,
    showDeprecatedWarning = true,
}: ModDetailModalProps) {
    const installedLabel = legacyInstallMode ? 'Installed' : 'Added';
    const installActionLabel = legacyInstallMode ? 'Install' : 'Add';

    const installedMod = useMemo(
        () => installedMods.find(m => packageIdentityKey(m.fullName) === pkg.full_name.toLowerCase()),
        [installedMods, pkg.full_name]
    );
    const installedVersionChoice = useMemo(() => {
        if (!installedMod) return undefined;
        const installedSource = installedMod.source || 'thunderstore';
        return pkg.versions.find(version =>
            version.version_number === installedMod.versionNumber
            && (version.source || 'thunderstore') === installedSource
        );
    }, [installedMod, pkg.versions]);
    const defaultVersionChoice = installedVersionChoice || pkg.versions[0];
    const defaultVersionKey = defaultVersionChoice ? versionChoiceKey(defaultVersionChoice) : '';
    const [selectedVersionChoice, setSelectedVersionChoice] = useState(() => ({
        packageName: pkg.full_name,
        versionKey: defaultVersionKey,
    }));
    const selectedVersionKey = selectedVersionChoice.packageName === pkg.full_name
        ? selectedVersionChoice.versionKey
        : defaultVersionKey;
    const wasOpenRef = useRef(isOpen);

    useEffect(() => {
        const justOpened = isOpen && !wasOpenRef.current;
        wasOpenRef.current = isOpen;
        if (justOpened) {
            setSelectedVersionChoice({ packageName: pkg.full_name, versionKey: defaultVersionKey });
        }
    }, [defaultVersionKey, isOpen, pkg.full_name]);
    const [prevPkgUuid, setPrevPkgUuid] = useState(pkg.uuid4);
    const [versionsList, setVersionsList] = useState<PackageVersion[]>(pkg.versions);

    if (pkg.uuid4 !== prevPkgUuid) {
        setPrevPkgUuid(pkg.uuid4);
        setVersionsList(pkg.versions);
    }

    const selectedVersion = useMemo(
        () => versionsList.find(v => versionChoiceKey(v) === selectedVersionKey) || versionsList[0] || pkg.versions[0],
        [versionsList, selectedVersionKey, pkg.versions]
    );
    const mod = selectedVersion;
    const packageNameParts = pkg.full_name.split('-', 2);
    const storePageUrl = mod.source === 'hexium'
        ? `https://${gameId}.hexium.gg/mods/${packageNameParts[0]}/${packageNameParts[1] || pkg.name}`
        : mod.source === 'outerwilds'
            ? pkg.package_url
            : `https://thunderstore.io/c/${gameId}/p/${packageNameParts[0]}/${packageNameParts[1] || pkg.name}/`;
    const websiteUrl = mod.website_url || storePageUrl;
    const isLocalMod = !!mod.isLocal;
    const localReadme = mod.localReadme?.trim() || '';
    const installedSource = installedMod?.source || 'thunderstore';
    const selectedSource = mod.source || 'thunderstore';
    const isSelectedInstalled = !!installedMod
        && installedMod.versionNumber === mod.version_number
        && installedSource === selectedSource;
    const isStoreSwitch = !!installedMod
        && installedMod.versionNumber === mod.version_number
        && installedSource !== selectedSource;
    const isSelectedLatest = versionsList[0]?.version_number === mod.version_number;

    const [activeTab, setActiveTab] = useState<Tab>('description');
    const [readmeContent, setReadmeContent] = useState<string | null>(null);
    const [changelogContent, setChangelogContent] = useState<string | null>(null);
    const [loadingContent, setLoadingContent] = useState(false);
    const [dependencies, setDependencies] = useState<Package[]>([]);
    const [loadingKey, setLoadingKey] = useState<string>('');
    const [showImageLightbox, setShowImageLightbox] = useState(false);

    const fetchGithubReadme = useCallback(async (owner: string, repo: string) => {
        setLoadingContent(true);
        try {
            const url = `https://api.github.com/repos/${owner}/${repo}/readme`;
            const jsonString = await window.ipcRenderer.fetchTextContent(url);
            const data = JSON.parse(jsonString);
            if (data.content && data.encoding === 'base64') {
                const base64Content = data.content.replace(/\s/g, '');
                const rawMarkdown = decodeURIComponent(escape(window.atob(base64Content)));
                const parsedHtml = await marked.parse(rawMarkdown);
                const sanitized = DOMPurify.sanitize(parsedHtml);
                setReadmeContent(sanitized);
            }
        } catch (e) {
            console.error("Failed to fetch README from GitHub:", e);
        } finally {
            setLoadingContent(false);
        }
    }, []);

    const fetchGithubChangelog = useCallback(async (owner: string, repo: string) => {
        setLoadingContent(true);
        try {
            const candidates = ['CHANGELOG.md', 'changelog.md', 'History.md', 'HISTORY.md'];
            let content = '';
            for (const file of candidates) {
                try {
                    const url = `https://api.github.com/repos/${owner}/${repo}/contents/${file}`;
                    const jsonString = await window.ipcRenderer.fetchTextContent(url);
                    const data = JSON.parse(jsonString);
                    if (data.content && data.encoding === 'base64') {
                        const base64Content = data.content.replace(/\s/g, '');
                        content = decodeURIComponent(escape(window.atob(base64Content)));
                        break;
                    }
                } catch {
                    // try next candidate
                }
            }
            if (content) {
                const parsedHtml = await marked.parse(content);
                const sanitized = DOMPurify.sanitize(parsedHtml);
                setChangelogContent(sanitized);
            } else {
                setChangelogContent("<p>No changelog found in repository.</p>");
            }
        } catch (e) {
            console.error("Failed to fetch changelog from GitHub:", e);
            setChangelogContent("<p>No changelog found in repository.</p>");
        } finally {
            setLoadingContent(false);
        }
    }, []);

    useEffect(() => {
        if (!isOpen || gameId !== 'outerwilds' || isLocalMod) return;

        const githubInfo = getGithubRepo(pkg.versions[0]?.website_url || pkg.package_url);
        if (!githubInfo) return;

        let cancelled = false;
        const fetchGithubReleases = async () => {
            try {
                const url = `https://api.github.com/repos/${githubInfo.owner}/${githubInfo.repo}/releases?per_page=100`;
                const jsonString = await window.ipcRenderer.fetchTextContent(url);
                const releases = JSON.parse(jsonString);
                if (Array.isArray(releases) && !cancelled) {
                    const mapped: PackageVersion[] = [];
                    for (const rel of releases) {
                        const zipAsset = rel.assets?.find((a: any) => a.name.toLowerCase().endsWith('.zip')) || rel.assets?.[0];
                        if (!zipAsset) continue;
                        const version_number = rel.tag_name.replace(/^v/, '');
                        mapped.push({
                            name: pkg.versions[0]?.name || pkg.name,
                            full_name: `${pkg.owner}-${pkg.name}-${version_number}`,
                            description: rel.name || pkg.versions[0]?.description || '',
                            icon: pkg.versions[0]?.icon || '',
                            version_number,
                            dependencies: [],
                            download_url: zipAsset.browser_download_url,
                            downloads: zipAsset.download_count || 0,
                            date_created: rel.published_at || rel.created_at || pkg.date_created,
                            website_url: pkg.versions[0]?.website_url || pkg.package_url,
                            is_active: true,
                            uuid4: String(rel.id || rel.tag_name),
                            file_size: zipAsset.size || 0,
                            source: 'outerwilds',
                        });
                    }
                    if (mapped.length > 0 && !cancelled) {
                        setVersionsList(mapped);
                    }
                }
            } catch (e) {
                console.error("Failed to fetch GitHub releases for version history:", e);
            }
        };

        void fetchGithubReleases();
        return () => {
            cancelled = true;
        };
    }, [isOpen, gameId, pkg, isLocalMod]);



    const handleVersionChange = (version: PackageVersion) => {
        setSelectedVersionChoice({ packageName: pkg.full_name, versionKey: versionChoiceKey(version) });
    };

    const fetchDependencies = useCallback(async (targetMod: PackageVersion) => {
        try {
            const result = await window.ipcRenderer.lookupPackagesByNames(gameId, targetMod.dependencies);
            if (result && Array.isArray(result.found)) {
                setDependencies(result.found);
            }
        } catch (e) {
            console.error("Failed to fetch dependencies", e);
        }
    }, [gameId]);

    const fetchContent = useCallback(async (targetMod: PackageVersion, type: 'readme' | 'changelog') => {
        setLoadingContent(true);
        try {
            const suffix = `-${targetMod.version_number}`;
            const packageName = targetMod.full_name.endsWith(suffix)
                ? targetMod.full_name.slice(0, -suffix.length)
                : targetMod.full_name;
            const separator = packageName.indexOf('-');
            const owner = separator === -1 ? packageName : packageName.slice(0, separator);
            const name = separator === -1 ? packageName : packageName.slice(separator + 1);
            const url = targetMod.source === 'hexium'
                ? `https://hexium.gg/api/experimental/package/${owner}/${name}/${targetMod.version_number}/${type}/`
                : `https://thunderstore.io/api/cyberstorm/package/${owner}/${name}/v/${targetMod.version_number}/${type}/`;

            const jsonString = await window.ipcRenderer.fetchTextContent(url);
            const data = JSON.parse(jsonString);

            if (data.html) {
                const sanitized = DOMPurify.sanitize(data.html);
                if (type === 'readme') setReadmeContent(sanitized);
                else setChangelogContent(sanitized);
            } else if (typeof data.markdown === 'string' && data.markdown.trim()) {
                const html = await marked.parse(data.markdown);
                const sanitized = DOMPurify.sanitize(html);
                if (type === 'readme') setReadmeContent(sanitized);
                else setChangelogContent(sanitized);
            }
        } catch (e) {
            console.error(`Failed to fetch ${type}:`, e);
        } finally {
            setLoadingContent(false);
        }
    }, []);

    useEffect(() => {
        const detailKey = versionChoiceKey(mod);
        if (!isOpen || loadingKey === detailKey) return;

        let cancelled = false;
        const loadModDetails = async () => {
            await Promise.resolve();
            if (cancelled) return;

            setLoadingKey(detailKey);
            setReadmeContent(null);
            setChangelogContent(null);
            setDependencies([]);
            setActiveTab('description');
            if (!isLocalMod) {
                if (gameId === 'outerwilds') {
                    const githubInfo = getGithubRepo(mod.website_url || pkg.package_url);
                    if (githubInfo) {
                        void fetchGithubReadme(githubInfo.owner, githubInfo.repo);
                    }
                } else {
                    void fetchContent(mod, 'readme');
                }

                if (mod.dependencies?.length > 0 && gameId) {
                    void fetchDependencies(mod);
                }
            }
        };

        void loadModDetails();
        return () => {
            cancelled = true;
        };
    }, [fetchContent, fetchDependencies, gameId, isLocalMod, isOpen, loadingKey, mod, pkg, fetchGithubReadme]);

    useEffect(() => {
        if (!isLocalMod && activeTab === 'changelog' && !changelogContent) {
            let cancelled = false;
            const loadChangelog = async () => {
                await Promise.resolve();
                if (!cancelled) {
                    if (gameId === 'outerwilds') {
                        const githubInfo = getGithubRepo(mod.website_url || pkg.package_url);
                        if (githubInfo) {
                            void fetchGithubChangelog(githubInfo.owner, githubInfo.repo);
                        }
                    } else {
                        void fetchContent(mod, 'changelog');
                    }
                }
            };

            void loadChangelog();
            return () => {
                cancelled = true;
            };
        }
    }, [activeTab, changelogContent, fetchContent, isLocalMod, mod, gameId, pkg, fetchGithubChangelog]);

    if (!isOpen) return null;

    // Format bytes to human readable string (KB, MB, GB)
    const formatBytes = (bytes: number): string => {
        if (bytes >= 1000 * 1000 * 1000) {
            return `${(bytes / (1000 * 1000 * 1000)).toFixed(1)} GB`;
        } else if (bytes >= 1000 * 1000) {
            return `${(bytes / (1000 * 1000)).toFixed(1)} MB`;
        } else if (bytes >= 1000) {
            return `${(bytes / 1000).toFixed(0)} KB`;
        }
        return `${bytes} B`;
    };

    // Calculate total size including dependencies
    const totalBytes = mod.file_size + dependencies.reduce((sum, dep) => sum + (dep.versions[0]?.file_size || 0), 0);
    const sizeDisplay = dependencies.length > 0 ? formatBytes(totalBytes) : formatBytes(mod.file_size);
    const detailTabs: Tab[] = isLocalMod ? ['description'] : ['description', 'changelog', 'dependencies'];

    return (
        <>
            <div
                className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-6"
                onClick={onClose}
            >
                <div
                    className="bg-gray-800 rounded-xl max-w-5xl w-full h-[90vh] flex flex-col border border-gray-700 shadow-2xl overflow-hidden"
                    onClick={(e) => e.stopPropagation()}
                >
                    {/* Header with Icon and Title */}
                    <div className="flex items-start gap-5 p-6 border-b border-gray-700 bg-gray-900/50 flex-shrink-0">
                        {/* Icon - larger, clickable for lightbox */}
                        <div
                            className="relative h-32 w-32 flex-shrink-0 cursor-pointer overflow-hidden rounded-xl border border-gray-700 bg-gray-900 shadow-lg group"
                            onClick={() => mod.icon && setShowImageLightbox(true)}
                        >
                            {mod.icon ? (
                                <img src={mod.icon} alt={mod.name} className="w-full h-full object-cover" />
                            ) : (
                                <div className="w-full h-full flex items-center justify-center text-4xl font-bold text-gray-600">
                                    {mod.name[0]}
                                </div>
                            )}
                            {mod.icon && (
                                <div className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center">
                                    <svg className="w-6 h-6 text-on-media" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0zM10 7v3m0 0v3m0-3h3m-3 0H7" />
                                    </svg>
                                </div>
                            )}
                        </div>

                        {/* Title & Info */}
                        <div className="min-w-0 flex-1">
                            <div className="flex items-start justify-between gap-4">
                                <div className="min-w-0 flex-1">
                                    <div className="flex min-w-0 flex-wrap items-center gap-2.5">
                                        <h2 className="truncate text-2xl font-bold leading-tight text-white">{mod.name}</h2>
                                        {showDeprecatedWarning && pkg.is_deprecated ? (
                                            <span className="inline-flex flex-shrink-0 items-center gap-1 rounded-full border border-red-500/35 bg-red-950/55 px-2 py-0.5 text-[11px] font-semibold text-fg-danger">
                                                <svg className="h-3.5 w-3.5" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true">
                                                    <path fillRule="evenodd" d="M8.26 3.1c.76-1.36 2.72-1.36 3.48 0l6.52 11.6c.75 1.33-.21 2.98-1.74 2.98H3.48c-1.53 0-2.49-1.65-1.74-2.98L8.26 3.1ZM10 7.75a.75.75 0 0 1 .75.75v3a.75.75 0 0 1-1.5 0v-3a.75.75 0 0 1 .75-.75Zm0 7.25a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z" clipRule="evenodd" />
                                                </svg>
                                                Deprecated
                                            </span>
                                        ) : null}
                                        {isSelectedInstalled ? (
                                            <span className="inline-flex flex-shrink-0 rounded-full border border-green-500/30 bg-green-500/15 px-2 py-0.5 text-[11px] font-semibold text-fg-success">
                                                {installedLabel}
                                            </span>
                                        ) : isSelectedLatest ? (
                                            <span className="inline-flex flex-shrink-0 rounded-full border border-blue-500/30 bg-blue-500/15 px-2 py-0.5 text-[11px] font-semibold text-fg-accent">
                                                Latest version
                                            </span>
                                        ) : (
                                            <span className="inline-flex flex-shrink-0 rounded-full border border-amber-500/30 bg-amber-500/15 px-2 py-0.5 text-[11px] font-semibold text-fg-warning">
                                                Outdated version
                                            </span>
                                        )}
                                    </div>
                                    <p className="text-sm text-gray-400 mt-0.5">
                                        {isLocalMod ? 'Custom local mod' : `by ${mod.full_name.split('-')[0]}`}
                                    </p>
                                    <div className="mt-2">
                                        <VersionSourcePicker
                                            versions={versionsList}
                                            selected={mod}
                                            onChange={handleVersionChange}
                                        />
                                    </div>
                                </div>
                                <div className="flex flex-shrink-0 flex-col items-end text-right">
                                    <span className="text-[10px] uppercase text-gray-500 font-bold tracking-wider">Updated</span>
                                    <span className="text-sm text-gray-300">{new Date(pkg.date_updated).toLocaleDateString()}</span>
                                    {websiteUrl && (
                                        <button
                                            onClick={() => {
                                                import('@tauri-apps/plugin-shell').then(({ open }) => {
                                                    open(websiteUrl);
                                                });
                                            }}
                                            className="flex items-center gap-1 text-sm text-fg-accent hover:text-fg-accent transition-colors mt-1"
                                        >
                                            {mod.website_url ? 'Website' : sourceName(mod.source)}
                                            <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                                            </svg>
                                        </button>
                                    )}
                                </div>
                            </div>

                            {/* Independent row so metadata never competes with the version picker. */}
                            <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-gray-500">
                                {!isLocalMod && (
                                    <>
                                        <span className="flex items-center gap-1.5" title={`${mod.downloads.toLocaleString()} downloads`}>
                                            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                                            </svg>
                                            {mod.downloads.toLocaleString()}
                                        </span>
                                        <LikeStat
                                            count={pkg.rating_score}
                                            className="gap-1.5 text-rose-400"
                                            iconClassName="w-4 h-4"
                                            title={`${pkg.rating_score.toLocaleString()} likes`}
                                        />
                                    </>
                                )}
                                <span className="flex items-center gap-1.5" title={dependencies.length > 0 ? `Package: ${formatBytes(mod.file_size)} + ${dependencies.length} dependencies` : undefined}>
                                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                    </svg>
                                    {sizeDisplay}
                                    {dependencies.length > 0 && <span className="text-gray-600 text-[10px] ml-0.5">(total)</span>}
                                </span>
                            </div>
                        </div>

                        {/* Close button */}
                        <button
                            onClick={onClose}
                            className="text-gray-500 hover:text-white transition-colors p-1 -mt-1 -mr-1"
                        >
                            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                        </button>
                    </div>

                    {/* Tabs */}
                    <div className="flex border-b border-gray-700 bg-gray-900/30 px-6 gap-6">
                        {detailTabs.map(tab => (
                            <button
                                key={tab}
                                onClick={() => setActiveTab(tab)}
                                className={`py-3 text-sm font-medium border-b-2 transition-colors capitalize flex items-center gap-2 ${activeTab === tab
                                    ? 'text-fg-accent border-blue-400'
                                    : 'text-gray-400 border-transparent hover:text-gray-200'
                                    }`}
                            >
                                {tab}
                                {tab === 'dependencies' && mod.dependencies?.length > 0 && (
                                    <span className={`text-xs px-1.5 py-0.5 rounded-full ${activeTab === tab ? 'bg-blue-500/20 text-fg-accent' : 'bg-gray-700 text-gray-400'}`}>
                                        {mod.dependencies.length}
                                    </span>
                                )}
                            </button>
                        ))}
                    </div>

                    {/* Scrollable Content */}
                    <div className="overflow-y-auto flex-1 p-6 min-h-0 bg-gray-900">
                        <style>{`
                        .prose details {
                            background-color: rgb(var(--r2-gray-800) / calc(var(--r2-gray-800-alpha, 1) * 0.5));
                            border: 1px solid rgb(var(--r2-gray-700) / calc(var(--r2-gray-700-alpha, 1) * 0.4));
                            border-radius: 0.5rem;
                            padding: 0.5rem;
                            margin-top: 1rem;
                            margin-bottom: 1rem;
                        }
                        .prose summary {
                            cursor: pointer;
                            font-weight: 600;
                            color: rgb(var(--r2-gray-300) / var(--r2-gray-300-alpha, 1));
                            outline: none;
                        }
                        .prose summary:hover {
                            color: rgb(var(--r2-white) / var(--r2-white-alpha, 1));
                        }
                    `}</style>
                        {activeTab === 'description' && (
                            <div>
                                {isLocalMod && localReadme ? (
                                    <div className="prose prose-invert prose-sm max-w-none whitespace-pre-wrap break-words overflow-hidden">
                                        {localReadme}
                                    </div>
                                ) : loadingContent && !readmeContent ? (
                                    <div className="animate-pulse space-y-3">
                                        <div className="h-4 bg-gray-700 rounded w-3/4"></div>
                                        <div className="h-4 bg-gray-700 rounded w-1/2"></div>
                                        <div className="h-4 bg-gray-700 rounded w-5/6"></div>
                                    </div>
                                ) : readmeContent ? (
                                    <div
                                        className="prose prose-invert prose-sm max-w-none prose-headings:text-white prose-a:text-fg-accent hover:prose-a:text-fg-accent prose-img:rounded-lg break-words overflow-hidden"
                                        dangerouslySetInnerHTML={{ __html: readmeContent }}
                                    />
                                ) : (
                                    <div className="text-center text-gray-500 py-10">
                                        <p>{mod.description || "No description available."}</p>
                                    </div>
                                )}
                            </div>
                        )}

                        {activeTab === 'changelog' && (
                            <div>
                                {loadingContent && !changelogContent ? (
                                    <div className="animate-pulse space-y-3">
                                        <div className="h-4 bg-gray-700 rounded w-1/4"></div>
                                        <div className="h-4 bg-gray-700 rounded w-full"></div>
                                    </div>
                                ) : changelogContent ? (
                                    <div
                                        className="prose prose-invert prose-sm max-w-none prose-headings:text-white prose-a:text-fg-accent hover:prose-a:text-fg-accent break-words overflow-hidden"
                                        dangerouslySetInnerHTML={{ __html: changelogContent }}
                                    />
                                ) : (
                                    <div className="text-center text-gray-500 py-10">
                                        <p>No changelog found.</p>
                                    </div>
                                )}
                            </div>
                        )}

                        {activeTab === 'dependencies' && (
                            <div>
                                {dependencies.length > 0 ? (
                                    <div className="space-y-2">
                                        {dependencies.map((dep, idx) => (
                                            <div key={idx} className="bg-gray-800 p-3 rounded border border-gray-700 flex items-center justify-between">
                                                <div className="flex items-center gap-3">
                                                    <div className="w-12 h-12 bg-gray-900 rounded flex-shrink-0 overflow-hidden border border-gray-700">
                                                        {dep.versions[0]?.icon ? (
                                                            <img src={dep.versions[0].icon} alt={dep.name} className="w-full h-full object-cover" />
                                                        ) : (
                                                            <div className="w-full h-full flex items-center justify-center text-xs font-bold text-gray-600">
                                                                {dep.name[0]}
                                                            </div>
                                                        )}
                                                    </div>
                                                    <div>
                                                        <div className="flex items-center gap-2">
                                                            <p className="text-white font-medium text-sm">{dep.name}</p>
                                                            <span className="bg-gray-700 px-1.5 py-0.5 rounded text-gray-400 text-xs">
                                                                v{dep.versions[0]?.version_number}
                                                            </span>
                                                            {installedMods.some(m => m.fullName.startsWith(dep.full_name)) && (
                                                                <span className="bg-green-500/20 text-fg-success px-1.5 py-0.5 rounded text-xs font-medium">
                                                                    {installedLabel}
                                                                </span>
                                                            )}
                                                        </div>
                                                        <p className="text-gray-500 text-xs truncate max-w-[300px]">{dep.versions[0]?.description}</p>
                                                    </div>
                                                </div>
                                            </div>
                                        ))}
                                    </div>
                                ) : (
                                    mod.dependencies && mod.dependencies.length > 0 ? (
                                        <div className="space-y-2 opacity-50">
                                            {mod.dependencies.map((dep, idx) => (
                                                <div key={idx} className="bg-gray-800 p-3 rounded border border-gray-700 flex items-center gap-3">
                                                    <div className="w-10 h-10 bg-gray-900 rounded flex items-center justify-center text-gray-500 text-xs font-mono border border-gray-700">DEP</div>
                                                    <span className="text-gray-300 font-mono text-sm">{dep}</span>
                                                </div>
                                            ))}
                                        </div>
                                    ) : (
                                        <div className="text-center text-gray-500 py-10">
                                            <p>No dependencies.</p>
                                        </div>
                                    )
                                )}
                            </div>
                        )}
                    </div>

                    {/* Footer with Actions */}
                    {!isBrowsing && (
                        <div className="p-6 border-t border-gray-700 bg-gray-900/50 flex gap-3 flex-shrink-0">
                            {isInstalled && onUninstall && (
                                <button
                                    onClick={() => {
                                        onUninstall();
                                    }}
                                    className="p-2 rounded-lg bg-red-500/10 text-red-500 hover:bg-red-500 hover:text-on-danger transition-all duration-200 border border-red-500/20 flex-shrink-0"
                                    title="Uninstall"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                    </svg>
                                </button>
                            )}
                            <button
                                onClick={() => {
                                    // If the mod is already in profile and user selected a different version,
                                    // always run update flow (also handles downgrades and non-latest switches).
                                    if (isInstalled && !isSelectedInstalled && onUpdate) {
                                        onUpdate(mod);
                                    } else {
                                        onInstall(mod);
                                    }
                                    onClose();
                                }}
                                disabled={isSelectedInstalled}
                                className={`flex-1 px-4 py-2.5 rounded-lg font-semibold transition-colors ${isSelectedInstalled
                                    ? 'bg-green-500/10 text-green-500 border border-green-500/20 cursor-default'
                                    : hasUpdate || isInstalled
                                    ? 'bg-amber-500 hover:bg-amber-600 text-on-warning'
                                    : 'bg-blue-600 hover:bg-blue-500 text-on-accent'
                                    }`}
                            >
                                {isSelectedInstalled
                                    ? installedLabel
                                    : isStoreSwitch
                                        ? `Switch to ${sourceName(mod.source)}`
                                        : isInstalled
                                            ? `Update to v${mod.version_number}`
                                            : `${installActionLabel} v${mod.version_number}`}
                            </button>
                        </div>
                    )}
                </div>
            </div>

            {/* Image Lightbox */}
            {
                showImageLightbox && mod.icon && (
                    <div
                        className="fixed inset-0 bg-black/90 flex items-center justify-center z-[100] cursor-pointer"
                        onClick={() => setShowImageLightbox(false)}
                    >
                        <img
                            src={mod.icon}
                            alt={mod.name}
                            className="max-w-[90vw] max-h-[90vh] object-contain rounded-lg shadow-2xl"
                            onClick={(e) => e.stopPropagation()}
                        />
                        <button
                            className="absolute top-6 right-6 text-white/70 hover:text-white transition-colors"
                            onClick={() => setShowImageLightbox(false)}
                        >
                            <svg className="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                        </button>
                    </div>
                )
            }
        </>
    );
}
