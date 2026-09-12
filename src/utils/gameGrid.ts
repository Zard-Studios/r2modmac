export function gameGridColumnCount(containerWidth: number): number {
    if (containerWidth >= 1200) return 8;
    if (containerWidth >= 1000) return 7;
    if (containerWidth >= 768) return 5;
    if (containerWidth >= 640) return 4;
    return 3;
}
