import { create } from 'zustand';
import type { Community } from '../types/thunderstore';

interface AppState {
    communities: Community[];
    communityImages: Record<string, string>;

    setCommunities: (communities: Community[]) => void;
    setCommunityImages: (images: Record<string, string>) => void;
}

export const useAppStore = create<AppState>((set) => ({
    communities: [],
    communityImages: {},

    setCommunities: (communities) => set({ communities }),
    setCommunityImages: (communityImages) => set({ communityImages }),
}));
