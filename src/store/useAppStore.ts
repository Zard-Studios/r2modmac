import { create } from 'zustand';
import type { Community, CommunityPlatformInfo } from '../types/thunderstore';

interface AppState {
    communities: Community[];
    communityImages: Record<string, string>;
    communityPlatforms: Record<string, CommunityPlatformInfo>;
    streamMode: boolean;
    username: string | null;

    setCommunities: (communities: Community[]) => void;
    setCommunityImages: (images: Record<string, string>) => void;
    setCommunityPlatforms: (platforms: Record<string, CommunityPlatformInfo>) => void;
    setStreamMode: (mode: boolean) => void;
    setUsername: (username: string | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
    communities: [],
    communityImages: {},
    communityPlatforms: {},
    streamMode: false,
    username: null,

    setCommunities: (communities) => set({ communities }),
    setCommunityImages: (communityImages) => set({ communityImages }),
    setCommunityPlatforms: (communityPlatforms) => set({ communityPlatforms }),
    setStreamMode: (streamMode) => set({ streamMode }),
    setUsername: (username) => set({ username }),
}));
