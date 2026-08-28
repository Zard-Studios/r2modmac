export interface EscapeLikeEvent {
    key: string;
    defaultPrevented?: boolean;
    isComposing?: boolean;
    keyCode?: number;
    altKey?: boolean;
    ctrlKey?: boolean;
    metaKey?: boolean;
    shiftKey?: boolean;
}

export function isPlainEscape(event: EscapeLikeEvent): boolean {
    return event.key === 'Escape'
        && !event.defaultPrevented
        && !event.isComposing
        && event.keyCode !== 229
        && !event.altKey
        && !event.ctrlKey
        && !event.metaKey
        && !event.shiftKey;
}

export function createDialogStack() {
    const entries: symbol[] = [];
    return {
        register(token: symbol) {
            entries.push(token);
        },
        unregister(token: symbol) {
            const index = entries.lastIndexOf(token);
            if (index >= 0) entries.splice(index, 1);
        },
        isTop(token: symbol) {
            return entries.at(-1) === token;
        },
        size() {
            return entries.length;
        },
    };
}

export const dialogStack = createDialogStack();
