import { create } from 'zustand';
import type { Community, CommunityPlatformInfo } from '../types/thunderstore';

interface AppState {
    communities: Community[];
    communityImages: Record<string, string>;
    communityPlatforms: Record<string, CommunityPlatformInfo>;

    setCommunities: (communities: Community[]) => void;
    setCommunityImages: (images: Record<string, string>) => void;
    setCommunityPlatforms: (platforms: Record<string, CommunityPlatformInfo>) => void;
}

export const useAppStore = create<AppState>((set) => ({
    communities: [],
    communityImages: {},
    communityPlatforms: {},

    setCommunities: (communities) => set({ communities }),
    setCommunityImages: (communityImages) => set({ communityImages }),
    setCommunityPlatforms: (communityPlatforms) => set({ communityPlatforms }),
}));
