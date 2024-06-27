import {RateLimiter} from "limiter";
import axios from "axios";
import {createServer, IncomingMessage, ServerResponse} from "http";
import fs from "fs";
import {Media, MediaFormat, MediaList, MediaListStatus} from "../types/anilist_api";
import {AnimeCollection, CollectionStatus} from "../types/anime_collection";
import {getAnilistId, getGlobalAnimeItemByMal} from "./data_util";
import open from "open";
import {
    autoLog,
    autoLogException,
    createProgressBar,
    incrementProgressBar,
    LogLevel,
    stopProgressBar
} from "./log_util";
import {GlobalAnimeData} from "../types/global_anime_data";
import {isServerMode, sleep} from "./util";
import {config} from "./config_util";

class AnilistClient {
    private readonly mainLimiter: RateLimiter;
    private readonly perTokenLimiter: RateLimiter;
    private readonly api_url: string = "https://graphql.anilist.co";
    private readonly token = {
        access_token: "",
        refresh_token: "",
        expires_in: new Date(),
        token_type: "",
    };
    private user_id: number = 0;
    private user_timezone: string = "+0900";

    // Maps anilist media id to entry id.
    private media_to_entry_id: Map<string, string> = new Map();

    constructor() {
        this.mainLimiter = new RateLimiter({
            tokensPerInterval: 30,
            interval: 60 * 1000
        });
        this.perTokenLimiter = new RateLimiter({
            tokensPerInterval: 1,
            interval: 500
        });
    }

    private get headers(): { [key: string]: string } {
        let headers: { [key: string]: string } = {
            'Content-Type': 'application/json',
            'Accept': 'application/json',
        };
        if (this.token.access_token) {
            headers["Authorization"] = `${this.token.token_type} ${this.token.access_token}`;
        }
        return headers;
    }

    private static convertStatus(collectionStatus: CollectionStatus): MediaListStatus {
        switch (collectionStatus) {
            case CollectionStatus.Watching:
                return MediaListStatus.CURRENT;
            case CollectionStatus.Completed:
                return MediaListStatus.COMPLETED;
            case CollectionStatus.OnHold:
                return MediaListStatus.PAUSED;
            case CollectionStatus.Dropped:
                return MediaListStatus.DROPPED;
            case CollectionStatus.PlanToWatch:
                return MediaListStatus.PLANNING;
        }
    }

    /**
     * Get anime collection of the current user.
     */
    public async getAnimeCollection(): Promise<MediaList[] | null> {
        const query = `
        query($id: Int, $page: Int) {
          Page(page: $page, perPage: 100) {
            pageInfo {
              hasNextPage
            }
            mediaList(userId: $id, type: ANIME) {
              id
              media {
                id
                idMal
                title {
                  romaji
                  english
                  native
                }
                format
                startDate {
                  year
                  month
                  day
                }
                episodes
                isAdult
              }
              status
              score
              notes
              progress
              updatedAt
              completedAt {
                year
                month
                day
              }
            }
          }
        }
        `;
        const variables = {'id': this.user_id, "page": 1};

        let result: MediaList[] = [];
        while (1) {
            let data = await this.query(query, variables);
            if (!data) return null;

            // Add to id map
            for (let media of data.Page.mediaList) {
                this.media_to_entry_id.set(String(media.media.id), String(media.id));
            }

            result = result.concat(data.Page.mediaList);
            if (!data.Page.pageInfo.hasNextPage) break;
            variables.page++;
        }
        return result;
    }

    /**
     * Smart update the anime collection based on the given list.
     * @param collection The list of anime to update.
     * @param syncComment Whether to sync the comment.
     */
    public async smartUpdateCollection(collection: AnimeCollection[], syncComment: boolean = false): Promise<number> {
        const query = `
        mutation($ids: [Int], $status: MediaListStatus, $scoreRaw: Int, $progress: Int, $notes: String, $completedAt: FuzzyDateInput) {
          UpdateMediaListEntries(ids: $ids, status: $status, scoreRaw: $scoreRaw, progress: $progress, notes: $notes, completedAt: $completedAt) {
            id
            status
            score
            progress
            notes
            completedAt {
              year
              month
              day
            }
          }
        }`;
        const variables: {
            ids: number[],
            status: MediaListStatus,
            scoreRaw: number,
            progress: number,
            notes?: string,
            completedAt?: {
                year: number,
                month: number,
                day: number,
            }
        }[] = [];

        createProgressBar(collection.length);
        let successCount = 0;

        // Group collections with same status, score, progress, and comments together.
        const grouped = collection.reduce((acc: { [key: string]: AnimeCollection[] }, cur) => {
            let key: string;
            if (syncComment)
                key = `${cur.status}-${cur.score}-${cur.watched_episodes}-${cur.comments}`;
            else
                key = `${cur.status}-${cur.score}-${cur.watched_episodes}`;
            if (!acc[key]) acc[key] = [];
            acc[key].push(cur);
            return acc;
        }, {});

        // Construct variables
        for (const key in grouped) {
            const collections = grouped[key];
            const scoreRaw = collections[0].score * 10; // Convert to 1-100 raw score scale
            const progress = collections[0].watched_episodes;
            const notes = syncComment ? collections[0].comments : undefined;
            const status = AnilistClient.convertStatus(collections[0].status);

            const findAnilistId = async (c: AnimeCollection) => {
                if (c.anilist_id) return c.anilist_id;
                if (!c.mal_id) throw new Error("Anilist and Bangumi ID both missing.");
                let id: string | null = await getAnilistId(c.mal_id);
                if (!id) {
                    let id2 = await this.getId(Number(c.mal_id));
                    if (!id2) {
                        autoLog(`Could not find anilist ID for ${c.title} (mal=${c.mal_id}).`, "Anilist.smartUpdateCollection", LogLevel.Warn);
                        return null;
                    }
                    id = String(id2);
                }
                return id;
            };

            // Use single query if there's only one collection
            if (collections.length == 1 || collection.length <= 70) {
                for (let c of collections) {
                    const id = await findAnilistId(c);
                    if (id) {
                        c.anilist_id = id;
                        await this.saveEntry(c, syncComment);
                        successCount++;
                    }
                    incrementProgressBar();
                }
                continue;
            }

            // Use multi query
            let ids: number[] = [];
            for (let c of collections) {
                if (!c.anilist_id) {
                    const id = await findAnilistId(c);
                    if (!id) {
                        incrementProgressBar();
                        continue;
                    }
                    c.anilist_id = id;
                }

                if (!this.media_to_entry_id.has(c.anilist_id)) {
                    // For anime not in the list, we need to create a new entry.
                    if (await this.saveEntry(c, syncComment)) {
                        successCount++;
                    }
                    incrementProgressBar();
                    continue;
                }
                ids.push(Number(this.media_to_entry_id.get(c.anilist_id)));
            }
            variables.push({
                ids,
                status,
                scoreRaw,
                progress,
                notes,
            });
        }

        // Update
        for (let variable of variables) {
            await this.query(query, variable);
            incrementProgressBar(variable.ids.length);
            successCount += variable.ids.length;
        }

        stopProgressBar();
        return successCount;
    }

    /**
     * Save a new entry to collection
     * @param collection New collection entry
     * @param syncComment Whether to sync comments
     */
    public async saveEntry(collection: AnimeCollection, syncComment: boolean = false): Promise<boolean> {
        if (!collection.anilist_id) {
            autoLog(`Failed to save ${collection.title} (mal=${collection.mal_id}), empty anilist ID.`, "Anilist.saveEntry", LogLevel.Error);
            return false;
        }

        const query = `
            mutation ($mediaId: Int, $status: MediaListStatus, $scoreRaw: Int, $progress: Int, $notes: String) {
                SaveMediaListEntry (mediaId: $mediaId, status: $status, scoreRaw: $scoreRaw, progress: $progress, notes: $notes) {
                    id
                }
            }
        `;
        let variables = {
            mediaId: Number(collection.anilist_id),
            status: AnilistClient.convertStatus(collection.status),
            scoreRaw: collection.score * 10,
            progress: collection.watched_episodes,
            notes: syncComment ? collection.comments : undefined,
        };
        let result = await this.query(query, variables);
        if (result.SaveMediaListEntry) {
            this.media_to_entry_id.set(collection.anilist_id, result.SaveMediaListEntry.id);
            return true;
        }
        return false;
    }

    /**
     * Get anilist id from mal id
     * @param mal_id Mal id
     */
    public async getId(mal_id: number): Promise<number | null> {
        const query = `
        query($id: Int) {
        Page(page: 1, perPage: 10) {
            media(idMal: $id, type: ANIME) {
              id
              idMal
              title {
                romaji
                english
                native
              }
              format
              startDate {
                year
                month
                day
              }
              episodes
              isAdult
            } 
          } 
        }`;
        const variables = {'id': mal_id};
        const data = await this.query(query, variables);
        if (!data) return null;

        let gl = await getGlobalAnimeItemByMal(String(mal_id));
        for (let media of data.Page.media as Media[]) {
            // Check Format
            switch (media.format) {
                case MediaFormat.TV:
                case MediaFormat.TV_SHORT:
                    if (gl?.type !== GlobalAnimeData.Type.Tv) continue;
                    break;
                case MediaFormat.MOVIE:
                    if (gl?.type !== GlobalAnimeData.Type.Movie) continue;
                    break;
                case MediaFormat.SPECIAL:
                    if (gl?.type !== GlobalAnimeData.Type.Special) continue;
                    break;
                case MediaFormat.OVA:
                    if (gl?.type !== GlobalAnimeData.Type.Ova) continue;
                    break;
                case MediaFormat.ONA:
                    if (gl?.type !== GlobalAnimeData.Type.Ona) continue;
                    break;
            }

            // Check air date
            if (media.startDate.year !== gl?.animeSeason.year) continue;
            switch (media.startDate.month) {
                case 1:
                case 2:
                    if (gl.animeSeason.season !== GlobalAnimeData.Season.Winter) continue;
                    break;
                case 3:
                    if (gl.animeSeason.season !== GlobalAnimeData.Season.Winter && gl.animeSeason.season !== GlobalAnimeData.Season.Spring) continue;
                    break;
                case 4:
                case 5:
                    if (gl.animeSeason.season !== GlobalAnimeData.Season.Spring) continue;
                    break;
                case 6:
                    if (gl.animeSeason.season !== GlobalAnimeData.Season.Spring && gl.animeSeason.season !== GlobalAnimeData.Season.Summer) continue;
                    break;
                case 7:
                case 8:
                    if (gl.animeSeason.season !== GlobalAnimeData.Season.Summer) continue;
                    break;
                case 9:
                    if (gl.animeSeason.season !== GlobalAnimeData.Season.Summer && gl.animeSeason.season !== GlobalAnimeData.Season.Fall) continue;
                    break;
                case 10:
                case 11:
                    if (gl.animeSeason.season !== GlobalAnimeData.Season.Fall) continue;
                    break;
                case 12:
                    if (gl.animeSeason.season !== GlobalAnimeData.Season.Fall && gl.animeSeason.season !== GlobalAnimeData.Season.Winter) continue;
                    break;
                default:
                    continue;
            }

            // Check episodes
            // if (media.episodes !== gl?.episodes) continue;

            return media.id;
        }

        return null;
    }

    /**
     * Search for an anime by name.
     * @param title The title of the anime.
     */
    public async searchAnime(title: string): Promise<Media[] | null> {
        const query = `
        query($title: String) {
          Page(page: 1, perPage: 10) {
            media(search: $title, type: ANIME) {
              id
              idMal
              title {
                romaji
                english
                native
              }
              format
              startDate {
                year
                month
                day
              }
              episodes
              isAdult
            }
          }
        }
        `;
        const variables = {'title': title};

        let result: Media[] = [];
        let data = await this.query(query, variables);
        if (!data) return null;
        result = result.concat(data.Page.media);

        return result
    }

    /**
     * Query the API with exponential backoff.
     * @param query The query to send.
     * @param variables The variables to send.
     * @param retries The number of retries in case of failure.
     * @param delay The initial delay for retries.
     */
    public async query(query: string, variables: any = {}, retries: number = 7, delay: number = 1000): Promise<any> {
        await this.perTokenLimiter.removeTokens(1);
        await this.mainLimiter.removeTokens(1);

        let response;
        try {
            response = await axios.post(this.api_url, JSON.stringify({
                query,
                variables
            }), {headers: this.headers});

            // Handle rate limit
            let requestRemain = Number(response.headers['x-ratelimit-remaining']) || 0;
            let limiterRemain = this.mainLimiter.getTokensRemaining();
            if (requestRemain < limiterRemain) {
                await this.mainLimiter.removeTokens(limiterRemain - requestRemain);
            }
            return response.data.data;
        } catch (error: any) {
            // Log error info
            autoLog(`Network error when querying. Error: ${error as Error}`, "Anilist.query", LogLevel.Error);
            autoLog(`Response header: ${JSON.stringify(response?.headers)}, data: ${JSON.stringify(response?.data)}`, "Anilist.query", LogLevel.Info);

            if (retries > 0) {
                const nextDelay = delay * 2;
                autoLog(`Next retrying in ${delay}ms... (retries left = ${retries - 1})`, "Anilist.query", LogLevel.Error);
                await new Promise(resolve => setTimeout(resolve, delay));
                return await this.query(query, variables, retries - 1, nextDelay);
            } else {
                autoLog(`No more retries left.`, "Anilist.query", LogLevel.Error);
                return null;
            }
        }
    }

    /**
     * Automatically load and check user token. If token is expired, prompt user to login.
     */
    public async autoUpdateToken() {
        // Load token from file
        if (!this.tokenExists()) {
            this.loadToken();
            // Check if the token doesn't exist
            if (!this.tokenExists()) {
                await this.getToken();
            }
        }

        // Refresh the token
        if (!await this.refreshToken()) {
            autoLog("Failed to refresh anilist token.", "Anilist.checkToken", LogLevel.Warn);
            await this.getToken();
        }

        // Save token to file
        if (!fs.existsSync(config.cache_path)) {
            fs.mkdirSync(config.cache_path);
        }
        fs.writeFileSync(config.cache_path + "/anilist_token.json", JSON.stringify(this.token));

        // Get user id
        const user = await this.query(`
            query {
                Viewer {
                    id
                }
            }
        `);
        this.user_id = user.Viewer.id;
        if (!this.user_id) {
            throw new Error("Failed to get user id.");
        }

        // Get user timezone
        let query = `
            query ($id: Int) {
                User(id: $id) {
                    options {
                        timezone
                    }
                }
            }
        `;
        let variables = {
            id: this.user_id
        };
        let data = await this.query(query, variables);
        this.user_timezone = data.User.options.timezone;
    }

    private async getToken() {
        // Handle server mode
        if (isServerMode) {
            autoLog("Anilist token expired. Please run `npm run token` to get new token.", "Anilist", LogLevel.Error);
            autoLog("Waiting for token...", "Anilist", LogLevel.Info);

            // Wait until token is available
            while (1) {
                this.loadToken();
                if (this.tokenExists() && await this.refreshToken())
                    break;
                await sleep(5000);
            }

            return;
        }

        // Setup callback server
        let code = "";
        const server = createServer((request: IncomingMessage, response: ServerResponse) => {
            const html = `<html lang='en'><head><title>[BGM-Sync] Token generated</title></head><body><h1>Token generated! Please close this window.</h1></body></html>`;
            response.writeHead(200, {
                "Content-Type": "text/html",
                "Content-Length": Buffer.byteLength(html),
            });
            response.end(html);

            if (request.url?.includes("code=")) {
                code = request.url.split("=")[1];
            }
            server.close();
        });
        server.listen(3499);

        const auth_url = `https://anilist.co/api/v2/oauth/authorize?client_id=7280&redirect_uri=http://localhost:3499&response_type=code`
        autoLog(`If auto open browser failed, please visit this link manually to authorize with anilist: ${auth_url}`, "Anilist");
        await open(auth_url);

        await new Promise((resolve) => {
            function check() {
                if (code) resolve(code);
                else setTimeout(check, 500);
            }

            check();
        });

        // Get token
        const token_url = "https://anilist.co/api/v2/oauth/token";
        const params = {
            client_id: "7280",
            client_secret: "oBmDJzVMNrHgIV8bANgLtxyG243skxD4XYh0TXbx",
            grant_type: "authorization_code",
            code,
            redirect_uri: "http://localhost:3499",
        }

        try {
            await this.mainLimiter.removeTokens(1);
            const response = await axios.post(token_url, params);
            let token = response.data;
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(Date.now() + token.expires_in * 1000);
            this.token.token_type = token.token_type;
        } catch (e: any) {
            autoLog("Failed to get anilist token.", "Anilist", LogLevel.Error);
            autoLogException(e as Error);
            process.exit(1);
        }
    }

    private async refreshToken(): Promise<Boolean> {
        const url = "https://anilist.co/api/v2/oauth/token";
        const params = {
            client_id: "7280",
            client_secret: "oBmDJzVMNrHgIV8bANgLtxyG243skxD4XYh0TXbx",
            grant_type: "refresh_token",
            refresh_token: this.token.refresh_token,
            redirect_uri: "http://localhost:3499",
        }

        try {
            await this.mainLimiter.removeTokens(1);
            const response = await axios.post(url, params);
            let token = response.data;
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(Date.now() + token.expires_in * 1000);
            this.token.token_type = token.token_type;
            return true;
        } catch (e: any) {
            autoLog("Failed to refresh anilist token.", "Anilist", LogLevel.Error);
            autoLogException(e as Error);
            return false;
        }
    }

    /**
     * Check if the token exists.
     */
    private tokenExists() {
        return this.token.access_token && this.token.refresh_token && this.token.expires_in && this.token.token_type;
    }

    /**
     * Load token from file.
     */
    private loadToken() {
        if (fs.existsSync(config.cache_path + "/anilist_token.json")) {
            let token = JSON.parse(fs.readFileSync(config.cache_path + "/anilist_token.json", "utf8"));
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(token.expires_in);
            this.token.token_type = token.token_type;
        }
    }
}


export const anilistClient = new AnilistClient();
