import {anilistClient} from "./anilist_client";
import {AnimeCollection, CollectionStatus} from "../types/anime_collection";
import {MediaListStatus} from "../types/anilist_api";
import {bangumiClient} from "./bangumi_client";
import cliProgress from "cli-progress";
import Scheduler from "./scheduler";
import {GlobalAnimeItem} from "../types/global_anime_data";
import {compareChinaWithGlobal, getChinaAnimeItem, getGlobalAnimeItemByMal, getMalId, matchChinaToGlobal} from "./data_util";
import stringSimilarity from "string-similarity";


/**
 * Fetch user's Anilist collections
 */
export async function getAnilistCollections(): Promise<AnimeCollection[]> {
    let collection = await anilistClient.getAnimeCollection();
    let res: AnimeCollection[] = [];
    if (!collection) return res;

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

        // TODO: handle anilist media entry

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
 * Fetch user's Bangumi collections
 */
export async function getBangumiCollections(): Promise<AnimeCollection[]> {
    let collection = await bangumiClient.getAnimeCollection();
    let res: AnimeCollection[] = [];
    if (!collection) return res;

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
 * Match and fill mal_id from global anime objects for bangumi collections.
 * @param bangumiCollection The Bangumi collection to be matched.
 */
export async function fillBangumiCollection(bangumiCollection: AnimeCollection[]): Promise<AnimeCollection[]> {
    // Setup progress bar
    const progressBar = new cliProgress.MultiBar({
        stopOnComplete: true,
        etaBuffer: 100,
        hideCursor: true,
        forceRedraw: true,
    }, cliProgress.Presets.shades_classic);
    const bar1 = progressBar.create(bangumiCollection.length, 0, {});

    // Schedule fixed amount of async jobs at most to avoid blocking and delays in console.log
    let scheduler = new Scheduler(10);
    let result: Array<{ bgm: AnimeCollection, global?: GlobalAnimeItem }> = new Array(bangumiCollection.length);
    for (let i = 0; i < bangumiCollection.length; i++) {
        let bangumiItem = bangumiCollection[i];
        scheduler.push(async () => {
            if (!bangumiItem.bgm_id) {
                bar1.increment();
                progressBar.log(`[fillBangumiCollection] ${bangumiItem} has no bgm_id.\n`);
                result[i] = {
                    bgm: bangumiItem,
                };
                return;
            }

            // Get China anime object
            const chinaItem = await getChinaAnimeItem(bangumiItem.bgm_id);
            if (!chinaItem) {
                bar1.increment();
                progressBar.log(`[fillBangumiCollection] Cannot construct cn_anime object for ${bangumiItem.bgm_id}.\n`);
                result[i] = {
                    bgm: bangumiItem,
                };
                return;
            }

            // Match China anime object to global
            const globalItem = await matchChinaToGlobal(chinaItem, bar1, progressBar);
            if (globalItem) {
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
                        result[i] = {
                            bgm: bangumiItem,
                            global: newGlobalItem,
                        };
                        return;
                    }
                }
            }

            progressBar.log(`[fillBangumiCollection] Cannot match ${chinaItem.title} (${bangumiItem.bgm_id}) to an global anime object.\n`);
            result[i] = {
                bgm: bangumiItem,
            };
            return;
        })
    }
    await scheduler.wait(); // Wait for all jobs to finish
    bar1.stop();

    // Fill mal_id to each collection
    let failedCount = 0;
    for (let globalMatchedElement of result) {
        if (!globalMatchedElement.global) {
            failedCount++;
        } else {
            globalMatchedElement.bgm.mal_id = getMalId(globalMatchedElement.global) || undefined;
        }
    }
    console.log(`${failedCount}/${result.length} entries cannot be matched.`);

    return result.map(element => element.bgm);
}


export async function generateChangelog(bangumiCollection: AnimeCollection[], anilistCollection: AnimeCollection[]): Promise<{ before?: AnimeCollection, after: AnimeCollection }[]> {
    let result: { before?: AnimeCollection, after: AnimeCollection }[] = [];

    for (let bangumi of bangumiCollection) {
        if (!bangumi.mal_id) continue;

        // Fix bangumi episode count
        let globalEpisodeCount = await getGlobalAnimeItemByMal(bangumi.mal_id).then(item => item?.episodes || 0);
        if (bangumi.status == CollectionStatus.Completed || bangumi.watched_episodes > globalEpisodeCount) {
            bangumi.watched_episodes = globalEpisodeCount;
        }

        // If the entry don't exist in to, add it to result
        let anilist = anilistCollection.find(col => col.mal_id === bangumi.mal_id);
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


export function renderDiff(before: AnimeCollection | undefined, after: AnimeCollection): string {
    let result = '';

    if (!before || before.score != after.score) {
        result += `Score: ${before ? before.score : 'NA'} -> ${after.score}\n`;
    }
    if (!before || before.status != after.status) {
        result += `Status: ${before ? before.status : 'NA'} -> ${after.status}\n`;
    }
    if (!before || before.watched_episodes != after.watched_episodes) {
        result += `Watched episodes: ${before ? before.watched_episodes : 'NA'} -> ${after.watched_episodes}\n`;
    }

    return result;
}
