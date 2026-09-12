export type ProfileModView = 'all' | 'updates' | 'sync';

export function resolveProfileModView(
    currentView: ProfileModView,
    availableViews: readonly ProfileModView[],
): ProfileModView {
    return availableViews.includes(currentView) ? currentView : 'all';
}
