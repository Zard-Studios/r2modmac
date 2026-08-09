import {
    Ban,
    Check,
    Clock3,
    Copy,
    Download,
    FileText,
    Folder,
    Gamepad2,
    Heart,
    House,
    Import,
    Keyboard,
    LayoutGrid,
    List,
    Palette,
    PanelsTopLeft,
    Play,
    Plus,
    RefreshCw,
    Search,
    Settings,
    Square,
    Trash2,
    TriangleAlert,
    UserRound,
    Video,
    X,
    Zap,
    type LucideIcon,
} from 'lucide-react';

/**
 * Semantic icon names used by r2modmac, backed exclusively by Lucide.
 *
 * Keeping this adapter means screens share the exact same library glyph for
 * the same concept instead of accumulating slightly different inline SVGs.
 */
export type IconName =
    | 'play'
    | 'stop'
    | 'apply'
    | 'profile'
    | 'game'
    | 'settings'
    | 'theme'
    | 'keyboard'
    | 'plus'
    | 'import'
    | 'copy'
    | 'file'
    | 'browse'
    | 'install'
    | 'version'
    | 'parallel'
    | 'logs'
    | 'layout'
    | 'warning'
    | 'cache'
    | 'stream'
    | 'update'
    | 'support'
    | 'folder'
    | 'search'
    | 'home'
    | 'list'
    | 'disable'
    | 'close';

const ICONS: Record<IconName, LucideIcon> = {
    play: Play,
    stop: Square,
    apply: Check,
    profile: UserRound,
    game: Gamepad2,
    settings: Settings,
    theme: Palette,
    keyboard: Keyboard,
    plus: Plus,
    import: Import,
    copy: Copy,
    file: FileText,
    browse: LayoutGrid,
    install: Download,
    version: Clock3,
    parallel: Zap,
    logs: FileText,
    layout: PanelsTopLeft,
    warning: TriangleAlert,
    cache: Trash2,
    stream: Video,
    update: RefreshCw,
    support: Heart,
    folder: Folder,
    search: Search,
    home: House,
    list: List,
    disable: Ban,
    close: X,
};

export function AppIcon({
    name,
    className = 'h-5 w-5',
    strokeWidth = 1.75,
}: {
    name: IconName;
    className?: string;
    strokeWidth?: number;
}) {
    const Icon = ICONS[name];
    return <Icon className={className} strokeWidth={strokeWidth} aria-hidden="true" />;
}
