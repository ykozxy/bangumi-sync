export interface MalAnimeListResponse {
    data: MalAnimeListItem[];
    paging: {
        previous?: string;
        next?: string;
    };
}

export interface MalAnimeListItem {
    node: MalAnimeNode;
    list_status: MalListStatus;
}

export interface MalAnimeNode {
    id: number;
    title: string;
}

export interface MalListStatus {
    status: MalWatchStatus;
    score: number;
    num_episodes_watched: number;
    is_rewatching: boolean;
    updated_at: string;
    finish_date?: string;
    comments: string;
}

export type MalWatchStatus = 'watching' | 'completed' | 'on_hold' | 'dropped' | 'plan_to_watch';
