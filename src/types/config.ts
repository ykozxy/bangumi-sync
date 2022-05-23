export interface Config {
    sync_comments: boolean;
    manual_confirm: boolean;
    server_mode_interval: number;  // in seconds
    enable_notifications: boolean;
    cache_path: string;
    log_path: string;
    log_file_level: "debug" | "info" | "warn" | "error";
    log_console_level: "debug" | "info" | "warn" | "error";
    china_anime_database_url: string;
    global_anime_database_url: string;
}

export type ManualRelations = [number, number][];

export type IgnoreEntries = {
    bangumi: number[];
    anilist: number[];
    mal: number[];
}
