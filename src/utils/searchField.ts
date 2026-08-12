export interface SearchEscapeEvent {
    key: string;
    defaultPrevented?: boolean;
    metaKey?: boolean;
    ctrlKey?: boolean;
    altKey?: boolean;
    shiftKey?: boolean;
    isComposing?: boolean;
    keyCode?: number;
}

export function shouldReleaseSearchFocus(event: SearchEscapeEvent): boolean {
    if (event.key !== 'Escape') return false;
    if (event.defaultPrevented) return false;
    if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return false;
    if (event.isComposing || event.keyCode === 229) return false;
    return true;
}
