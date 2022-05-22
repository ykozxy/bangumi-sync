import {anilistClient} from "./anilist_client";
import {AnimeCollection, CollectionStatus} from "../types/anime_collection";
import {MediaListStatus} from "../types/anilist_api";
import {bangumiClient} from "./bangumi_client";
import Scheduler from "./scheduler";
import {GlobalAnimeItem} from "../types/global_anime_data";
import {
    compareChinaWithGlobal,
    getAnilistId,
    getChinaAnimeItem,
    getGlobalAnimeItemByAnilist,
    getGlobalAnimeItemByMal,
    getMalId,
    ignore_entries,
    manual_relations,
    matchChinaToGlobal
} from "./data_util";
import stringSimilarity from "string-similarity";
import {autoLog, createProgressBar, incrementProgressBar, LogLevel, stopProgressBar} from "./log_util";
import {notify} from "node-notifier";
import {Config} from "../types/config";

const config: Config = require("../../config.json");


/**
 * @description Fetch user's Anilist collections
 */
export async function getAnilistCollections(): Promise<AnimeCollection[]> {
    // Get raw collections
    let collection = await anilistClient.getAnimeCollection();
    let res: AnimeCollection[] = [];
    if (!collection) return res;

    // Convert format
    for (let mediaList of collection) {
        let status: CollectionStatus;
        switch (mediaList.status) {
            case MediaListStatus.CURRENT:
                status = CollectionStatus.Watching
                break;
            case MediaListStatus.COMPLETED:
            case MediaListStatus.REPEATING:
                status = CollectionStatus.Completed
                break;
            case MediaListStatus.PAUSED:
                status = CollectionStatus.OnHold
                break;
            case MediaListStatus.DROPPED:
                status = CollectionStatus.Dropped
                break;
            case MediaListStatus.PLANNING:
                status = CollectionStatus.PlanToWatch
                break;
        }

        let update_time: Date;
        if (mediaList.updatedAt) {
            update_time = new Date(mediaList.updatedAt);
        } else if (mediaList.completedAt.year && mediaList.completedAt.month && mediaList.completedAt.day) {
            update_time = new Date(mediaList.completedAt.year, mediaList.completedAt.month - 1, mediaList.completedAt.day);
        } else {
            update_time = new Date(0);
        }

        // Skip if in ignore list
        if (ignore_entries.anilist.includes(mediaList.media.id)) continue;
        if (ignore_entries.mal.includes(mediaList.media.idMal)) continue;

        res.push({
            title: mediaList.media.title.native,
            comments: mediaList.notes,
            mal_id: String(mediaList.media.idMal),
            anilist_id: String(mediaList.media.id),
            score: mediaList.score,
            status,
            update_time,
            watched_episodes: mediaList.progress,
        });
    }

    return res;
}

/**
 * @description Fetch user's Bangumi collections
 */
export async function getBangumiCollections(): Promise<AnimeCollection[]> {
    // Get raw collections
    let collection = await bangumiClient.getAnimeCollection();
    let res: AnimeCollection[] = [];
    if (!collection) return res;

    // Convert format
    for (let entry of collection) {
        let status: CollectionStatus;
        switch (entry.type) {
            case 1:
                status = CollectionStatus.PlanToWatch;
                break;
            case 2:
                status = CollectionStatus.Completed;
                break;
            case 3:
                status = CollectionStatus.Watching;
                break;
            case 4:
                status = CollectionStatus.OnHold;
                break;
            case 5:
                status = CollectionStatus.Dropped;
                break;
        }

        // Skip if in ignore list
        if (ignore_entries.bangumi.includes(entry.subject_id)) continue;

        res.push({
            // title: (await getChinaAnimeItem(String(entry.subject_id)))?.title,
            bgm_id: String(entry.subject_id),
            comments: entry.comment,
            score: entry.rate,
            status,
            update_time: new Date(entry.updated_at),
            watched_episodes: entry.ep_status,
        });
    }

    return res;
}

/**
 * @description Match and fill mal_id from global anime objects for bangumi collections.
 * @param bangumiCollection The Bangumi collection to be matched.
 */
export async function fillBangumiCollection(bangumiCollection: AnimeCollection[]): Promise<AnimeCollection[]> {
    // Setup progress bar
    createProgressBar(bangumiCollection.length);

    // Schedule fixed amount of async jobs at most to avoid blocking and delays in console.log
    let scheduler = new Scheduler(15);
    let result: Array<{ bgm: AnimeCollection, global?: GlobalAnimeItem }> = new Array(bangumiCollection.length);
    for (let i = 0; i < bangumiCollection.length; i++) {
        let bangumiItem = bangumiCollection[i];
        // Push job to scheduler
        scheduler.push(async () => {
            if (!bangumiItem.bgm_id) {
                incrementProgressBar();
                autoLog(`${bangumiItem} has no bgm_id.`, "matchEntry", LogLevel.Warn);
                result[i] = {
                    bgm: bangumiItem,
                };
                return;
            }

            // Ignore if in ignore_entries
            if (ignore_entries.bangumi.find(r => String(r) === bangumiItem.bgm_id)) {
                incrementProgressBar();
                result[i] = {
                    bgm: bangumiItem,
                };
                return;
            }

            // If in manual relation, fetch the entry directly
            let manual_id = manual_relations.find(r => String(r[0]) === bangumiItem.bgm_id);
            if (manual_id && manual_id[1]) {
                let gl = await getGlobalAnimeItemByAnilist(String(manual_id[1]));
                if (gl) {
                    incrementProgressBar();
                    result[i] = {
                        bgm: bangumiItem,
                        global: gl,
                    };
                    return;
                }
            }

            // Get China anime object
            const chinaItem = await getChinaAnimeItem(bangumiItem.bgm_id);
            if (!chinaItem) {
                incrementProgressBar();
                autoLog(`Cannot construct cn_anime object for bgm=${bangumiItem.bgm_id}.`, "matchEntry", LogLevel.Warn);
                result[i] = {
                    bgm: bangumiItem,
                };
                return;
            }

            // Match China anime object to global
            const globalItem = await matchChinaToGlobal(chinaItem);
            if (globalItem) {
                incrementProgressBar();
                result[i] = {
                    bgm: bangumiItem,
                    global: globalItem,
                };
                return;
            }

            // If no match found in database, search directly on Anilist
            const cnTitles = [chinaItem.title];
            for (const [, names] of Object.entries(chinaItem.titleTranslate)) if (names) cnTitles.push(...names);
            const anilistItems = await anilistClient.searchAnime(chinaItem.title);
            if (anilistItems) {
                for (let anilistItem of anilistItems) {
                    let newGlobalItem = await getGlobalAnimeItemByMal(String(anilistItem.idMal));
                    if (!newGlobalItem) continue;

                    // Check name similarity
                    let maxSimilarity = 0;
                    const globalTitles = [newGlobalItem.title, ...newGlobalItem.synonyms];
                    for (const cnTitle of cnTitles) {
                        maxSimilarity = Math.max(maxSimilarity, stringSimilarity.findBestMatch(cnTitle, globalTitles).bestMatch.rating);
                    }

                    // Check two object, enable strict mode when similarity is below 0.75
                    if (await compareChinaWithGlobal(chinaItem, newGlobalItem, maxSimilarity < 0.75)) {
                        incrementProgressBar();
                        result[i] = {
                            bgm: bangumiItem,
                            global: newGlobalItem,
                        };
                        return;
                    }
                }
            }

            incrementProgressBar();
            autoLog(`Cannot match ${chinaItem.title} (${bangumiItem.bgm_id}) to an global anime object.`, "matchEntry", LogLevel.Warn);
            result[i] = {
                bgm: bangumiItem,
            };
            return;
        })
    }
    await scheduler.wait(); // Wait for all jobs to finish
    stopProgressBar();

    // Fill mal_id and anilist_id to each collection
    let failedCount = 0;
    for (let globalMatchedElement of result) {
        if (!globalMatchedElement.global) {
            failedCount++;
        } else {
            globalMatchedElement.bgm.mal_id = getMalId(globalMatchedElement.global) || undefined;

            let manual_id = manual_relations.find(r => String(r[0]) === globalMatchedElement.bgm.bgm_id);
            globalMatchedElement.bgm.anilist_id = manual_id ? String(manual_id[1]) : undefined;
            if (!globalMatchedElement.bgm.anilist_id && globalMatchedElement.bgm.mal_id) {
                globalMatchedElement.bgm.anilist_id = getAnilistId(globalMatchedElement.bgm.mal_id) || undefined;
            }
        }
    }
    autoLog(`${failedCount}/${result.length} entries cannot be matched.`, "matchEntry", LogLevel.Info);

    if (failedCount > 0 && config.enable_notifications && process.argv[2] === "--server") {
        notify({
            title: "Bangumi-Sync",
            message: `${failedCount} bangumi entries cannot be matched to anilist, see log for details.`,
        });
    }

    return result.map(element => element.bgm);
}


export async function generateChangelog(bangumiCollection: AnimeCollection[], anilistCollection: AnimeCollection[]): Promise<{ before?: AnimeCollection, after: AnimeCollection }[]> {
    let result: { before?: AnimeCollection, after: AnimeCollection }[] = [];

    for (let bangumi of bangumiCollection) {
        if (!bangumi.mal_id && !bangumi.anilist_id) continue;

        // Fix bangumi episode count
        let globalEpisodeCount: number;
        if (bangumi.mal_id) {
            globalEpisodeCount = await getGlobalAnimeItemByMal(bangumi.mal_id).then(item => item?.episodes || 0);
        } else {
            globalEpisodeCount = await getGlobalAnimeItemByAnilist(String(bangumi.anilist_id)).then(item => item?.episodes || 0);
        }
        if (bangumi.status == CollectionStatus.Completed || bangumi.watched_episodes > globalEpisodeCount) {
            bangumi.watched_episodes = globalEpisodeCount;
        }

        // If the entry don't exist in to, add it to result
        let anilist = anilistCollection.find(col => {
            if (bangumi.anilist_id) {
                return col.anilist_id === bangumi.anilist_id;
            } else {
                return col.mal_id === bangumi.mal_id;
            }
        });
        if (!anilist) {
            result.push({
                after: bangumi,
            });
            continue;
        }

        // Sync ids
        bangumi.mal_id = anilist.mal_id;
        bangumi.anilist_id = anilist.anilist_id;
        anilist.bgm_id = bangumi.bgm_id;

        // Syne titles
        if (!anilist.title) anilist.title = bangumi.title;
        if (!bangumi.title) bangumi.title = anilist.title;

        // Compare entries for changes
        if (bangumi.score != anilist.score || bangumi.status != anilist.status || bangumi.watched_episodes != anilist.watched_episodes) {
            result.push({
                before: anilist,
                after: bangumi,
            });
        }
    }

    return result;
}

/**
 * @description Pretty format changelog
 * @param before Collection before changes
 * @param after Collection after changes
 * @param join_str The string to separate each field
 */
export function renderDiff(before: AnimeCollection | undefined, after: AnimeCollection, join_str = "\n"): string {
    let results: string[] = [];

    if (!before || before.score != after.score) {
        results.push(`Score: ${before ? before.score : 'NA'} -> ${after.score}`);
    }
    if (!before || before.status != after.status) {
        results.push(`Status: ${before ? before.status : 'NA'} -> ${after.status}`);
    }
    if (!before || before.watched_episodes != after.watched_episodes) {
        results.push(`Watched episodes: ${before ? before.watched_episodes : 'NA'} -> ${after.watched_episodes}`);
    }

    return results.join(join_str);
}
