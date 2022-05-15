export interface Cache {
    china_etag: string;
    global_etag: string;
    bgm_etags: BgmEtag;
}

export interface BgmEtag {
    [id: string]: string;
}

export enum EtagType {
    China = 'china',
    Global = 'global',
    Bgm = 'bgm',
}

export interface Relation {
    title: string;
    bgm_id: string;
    mal_id: string;
}
