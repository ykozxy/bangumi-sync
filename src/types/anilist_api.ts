export interface MediaList {
    id: number;
    media: Media;
    status: MediaListStatus;
    score: number;
    notes?: string;
    progress: number;
    updatedAt: number;
    completedAt: {
        year?: number;
        month?: number;
        day?: number;
    }
}

export enum MediaListStatus {
    CURRENT = 'CURRENT',
    COMPLETED = 'COMPLETED',
    PAUSED = 'PAUSED',
    DROPPED = 'DROPPED',
    PLANNING = 'PLANNING',
    REPEATING = 'REPEATING',
}

export interface Media {
    id: number;
    idMal: number;
    title: {
        romaji: string;
        english: string;
        native: string;
    };
    format: MediaFormat;
    startDate: {
        year: number;
        month: number;
        day: number;
    };
    episodes: number;
    isAdult: boolean;
}

export enum MediaFormat {
    TV = 'TV',
    TV_SHORT = 'TV_SHORT',
    MOVIE = 'MOVIE',
    SPECIAL = 'SPECIAL',
    OVA = 'OVA',
    ONA = 'ONA',
    MUSIC = 'MUSIC',
    MANGA = 'MANGA',
    NOVEL = 'NOVEL',
    ONE_SHOT = 'ONE_SHOT',
}
