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

export interface TextEntryTarget {
    tagName?: string;
    type?: string;
    isContentEditable?: boolean;
}

const NON_TEXT_INPUT_TYPES = new Set([
    'button',
    'checkbox',
    'color',
    'file',
    'hidden',
    'image',
    'radio',
    'range',
    'reset',
    'submit',
]);

export function shouldReleaseSearchFocus(event: SearchEscapeEvent): boolean {
    if (event.key !== 'Escape') return false;
    if (event.defaultPrevented) return false;
    if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return false;
    if (event.isComposing || event.keyCode === 229) return false;
    return true;
}

export function isTextEntryTarget(target: TextEntryTarget | null | undefined): boolean {
    if (!target) return false;
    if (target.isContentEditable) return true;

    const tag = (target.tagName || '').toLowerCase();
    if (tag === 'textarea') return true;
    if (tag !== 'input') return false;

    return !NON_TEXT_INPUT_TYPES.has((target.type || 'text').toLowerCase());
}
