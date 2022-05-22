import fs from "fs";
import {Cache, EtagType} from "../types/cache";
import {Config} from "../types/config";

const config: Config = require("../../config.json");

// Check cache path and files
if (!fs.existsSync(config.cache_path)) {
    fs.mkdirSync(config.cache_path);
}

if (!fs.existsSync(config.cache_path + '/cache.json')) {
    const d: Cache = {
        china_etag: "",
        global_etag: "",
        bgm_etags: {}
    };
    fs.writeFileSync(config.cache_path + '/cache.json', JSON.stringify(d));
}

const cache: Cache = JSON.parse(fs.readFileSync(config.cache_path + '/cache.json').toString());

/**
 * Get etag from cache
 * @param etag_type Etag type
 * @param subject_id Subject id, only used for bgm subject cache
 */
export function getEtagCache(etag_type: EtagType, subject_id?: string): string | null {
    switch (etag_type) {
        case EtagType.China:
            return cache.china_etag;
        case EtagType.Global:
            return cache.global_etag;
        case EtagType.Bgm:
            if (!subject_id) throw new Error("subject_id is required for getting bgm subject etag");
            return cache.bgm_etags[subject_id];
    }
}

/**
 * Set etag to cache
 * @param etag_type Etag type
 * @param etag Etag
 * @param subject_id Subject id, only used for bgm subject cache
 */
export function setEtagCache(etag_type: EtagType, etag: string, subject_id?: string): void {
    switch (etag_type) {
        case EtagType.China:
            cache.china_etag = etag;
            break;
        case EtagType.Global:
            cache.global_etag = etag;
            break;
        case EtagType.Bgm:
            if (!subject_id) throw new Error("subject_id is required for setting bgm subject etag");
            cache.bgm_etags[subject_id] = etag;
            break;
    }

    fs.writeFileSync(config.cache_path + '/cache.json', JSON.stringify(cache));
}
