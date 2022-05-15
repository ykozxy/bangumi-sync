export interface AnimeCollection {
    mal_id?: string;
    bgm_id?: string;
    anilist_id?: string;
    title?: string;

    status: CollectionStatus;
    watched_episodes: number;
    score: number;
    comments?: string;

    update_time: Date;
}


export enum CollectionStatus {
    Watching = 'Watching',
    Completed = 'Completed',
    OnHold = 'OnHold',
    Dropped = 'Dropped',
    PlanToWatch = 'PlanToWatch'
}
