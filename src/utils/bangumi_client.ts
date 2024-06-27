import {BangumiComponents, BangumiOperations} from "../types/bangumi_api";
import axios from "axios";
import {RateLimiter} from "limiter";
import {createServer, IncomingMessage, ServerResponse} from 'http';
import fs from "fs";
import open from "open";
import {autoLog, autoLogException, LogLevel} from "./log_util";
import {isServerMode, sleep} from "./util";
import {config} from "./config_util";

class BangumiClient {
    private static subjectCache: Map<string, {
        data: BangumiComponents["schemas"]["Subject"],
        expire: Date
    }> = new Map();
    private readonly limiter: RateLimiter;
    private readonly api_url: string = "https://api.bgm.tv";
    private readonly token = {
        access_token: "",
        refresh_token: "",
        expires_in: new Date(),
        token_type: "",
        user_id: 0,
    };
    private username: string = "";

    static {
        // Clear cache every hour
        const clearCache = () => {
            const now = new Date();
            BangumiClient.subjectCache.forEach((value, key) => {
                if (value.expire < now) {
                    BangumiClient.subjectCache.delete(key);
                }
            });
            setTimeout(clearCache, 1000 * 60 * 60);
        };

        clearCache();
    }

    constructor() {
        this.limiter = new RateLimiter({tokensPerInterval: 2, interval: 1000});
    }

    private get headers(): { [key: string]: string } {
        return {
            "Authorization": `${this.token.token_type} ${this.token.access_token}`,
            "User-Agent": "Bangumi-Sync/1.0.0",
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
            autoLog("Failed to refresh bangumi token.", "Bangumi", LogLevel.Error);
            await this.getToken();
        }

        // Check token from site
        if (!await this.checkToken()) {
            autoLog("Bangumi token expired.", "Bangumi", LogLevel.Error);
            await this.getToken();
        }

        // Save token to file
        if (!fs.existsSync(config.cache_path)) {
            fs.mkdirSync(config.cache_path);
        }
        fs.writeFileSync(config.cache_path + "/bangumi_token.json", JSON.stringify(this.token));

        // Get username
        if (!this.username) {
            const url = this.api_url + "/v0/me";
            await this.limiter.removeTokens(1);
            try {
                const response = await axios.get(url, {headers: this.headers});
                const data: BangumiComponents["schemas"]["User1"] = response.data;
                this.username = data.username;
            } catch (e) {
                autoLog("Failed to get user info.", "Bangumi", LogLevel.Error);
                autoLogException(e as Error);
                process.exit(1);
            }
        }
    }

    /**
     * Get subject by id.
     * @param bgm_id Subject id.
     * @param retries The number of retry attempts made.
     * @param delay The delay between retries in milliseconds.
     */
    public async getSubjectById(bgm_id: string, retries: number = 5, delay: number = 250): Promise<BangumiComponents["schemas"]["Subject"] | null> {
        // Check cache
        let cache = BangumiClient.subjectCache.get(bgm_id);
        if (cache) {
            return cache.data;
        }

        await this.limiter.removeTokens(1);
        const url = this.api_url + `/v0/subjects/${bgm_id}`;
        try {
            const response = await axios.get(url, {headers: this.headers});
            let res = response.data as BangumiComponents["schemas"]["Subject"];
            const cache_data = {data: res, expire: new Date(Date.now() + 24 * 60 * 60 * 1000)};
            BangumiClient.subjectCache.set(bgm_id, cache_data);
            return res;
        } catch (e: any) {
            autoLog(`Network error when fetching subject ${bgm_id}`, "Bangumi", LogLevel.Error);
            autoLogException(e as Error);

            if (retries > 0) {
                autoLog(`Retrying to fetch subject ${bgm_id} in ${delay}ms...`, "Bangumi", LogLevel.Error);
                await new Promise(resolve => setTimeout(resolve, delay));
                return await this.getSubjectById(bgm_id, retries - 1, delay * 2);
            }
            return null;
        }
    }

    /**
     * Search anime by title
     * @param title Title to search.
     * @param retries The number of retry attempts made.
     * @param delay The delay between retries in milliseconds.
     */
    public async searchAnime(title: string, retries: number = 5, delay: number = 250): Promise<BangumiComponents["schemas"]["SubjectSmall"][]> {
        await this.limiter.removeTokens(1);
        const url = this.api_url + `/search/subject/${encodeURIComponent(title)}`;
        let query: BangumiOperations["searchSubjectByKeywords"]["parameters"]["query"] = {
            type: 2,
        };

        try {
            const response = await axios.get(url, {params: query, headers: this.headers});
            return response.data.list as BangumiComponents["schemas"]["SubjectSmall"][];
        } catch (e) {
            autoLog(`Network error when searching anime ${title}`, "Bangumi", LogLevel.Error);
            autoLogException(e as Error);

            if (retries > 0) {
                autoLog(`Retrying to search anime ${title} in ${delay}ms...`, "Bangumi", LogLevel.Error);
                await new Promise(resolve => setTimeout(resolve, delay));
                return await this.searchAnime(title, retries - 1, delay * 2);
            }
            return [];
        }
    }

    /**
     * Get anime collection of the logged-in user.
     */
    public async getAnimeCollection(): Promise<BangumiComponents["schemas"]["UserCollection"][] | null> {
        await this.limiter.removeTokens(1);
        const url = this.api_url + `/v0/users/${this.username}/collections`;
        let query: BangumiOperations["getUserCollectionsByUsername"]["parameters"]["query"] = {
            subject_type: 2,
            offset: 0,
            limit: 50,
        }

        let collections: BangumiComponents["schemas"]["UserCollection"][] = [];
        let total = -1;
        let numFetched = 0;
        while (total === -1 || numFetched < total) {
            // console.log(`Fetching collection ${numFetched}...`);
            query.offset = numFetched;
            let data: BangumiComponents["schemas"]["Paged_UserCollection_"];
            try {
                const response = await axios.get(url, {params: query, headers: this.headers});
                data = response.data as BangumiComponents["schemas"]["Paged_UserCollection_"];
            } catch (e: any) {
                autoLog(`Network error when fetching anime collection.`, "Bangumi", LogLevel.Error);
                autoLogException(e as Error);
                return null;
            }

            numFetched += 50;
            total = data.total as number;
            collections = collections.concat(data.data as BangumiComponents["schemas"]["UserCollection"][]);
        }
        return collections;
    }

    /**
     * Prompt the user to get a new token.
     */
    private async getToken(): Promise<void> {
        // Handle server mode
        if (isServerMode) {
            autoLog("Bangumi token expired. Please run `npm run token` to get new token.", "Bangumi", LogLevel.Error);
            autoLog("Waiting for token...", "Bangumi", LogLevel.Info);

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
        let code: string = "";
        const server = createServer((request: IncomingMessage, response: ServerResponse) => {
            const html = "<html lang='en'><head><title>[BGM-Sync] Token generated</title></head><body><h1>Token generated! Please close this window.</h1></body></html>";
            response.writeHead(200, {
                "Content-Type": "text/html",
                "Content-Length": Buffer.byteLength(html),
            });
            response.end(html);

            if (request.url?.includes("?code="))
                code = request.url?.substr(request.url.indexOf("=") + 1);
            server.close();
        });
        server.listen(3498);

        // Prompt user to token generation web page
        const auth_url = `https://bgm.tv/oauth/authorize?client_id=bgm2304627c3f99e9682&response_type=code&redirect_uri=http://localhost:3498`;
        autoLog(`If auto open browser failed, please visit this link manually to authorize with bgm.tv: ${auth_url}`, "Bangumi");
        await open(auth_url);

        await new Promise((resolve) => {
            function check() {
                if (code) resolve(code);
                else setTimeout(check, 500);
            }

            check();
        });

        // Get token
        const token_url = "https://bgm.tv/oauth/access_token";
        const params = {
            client_id: "bgm2304627c3f99e9682",
            client_secret: "2afcec9ec22e28697b6c0601fa047cf7",
            grant_type: "authorization_code",
            code,
            redirect_uri: "http://localhost:3498",
        }

        try {
            const response = await axios.post(token_url, params);
            let token = response.data;
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(Date.now() + token.expires_in * 1000);
            this.token.token_type = token.token_type;
            this.token.user_id = token.user_id;
        } catch (e: any) {
            autoLog(`Failed to get bangumi token.`, "Bangumi", LogLevel.Error);
            autoLogException(e as Error);
            process.exit(1);
        }
    }

    /**
     * Refresh the token.
     */
    private async refreshToken(): Promise<Boolean> {
        const url = "https://bgm.tv/oauth/access_token";
        const params = {
            client_id: "bgm2304627c3f99e9682",
            client_secret: "2afcec9ec22e28697b6c0601fa047cf7",
            grant_type: "refresh_token",
            refresh_token: this.token.refresh_token,
            redirect_uri: "http://localhost:3498",
        }

        try {
            const response = await axios.post(url, params);
            let token = response.data;
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(Date.now() + token.expires_in * 1000);
            this.token.token_type = token.token_type;
            this.token.user_id = token.user_id;
            return true;
        } catch (e) {
            autoLog(`Failed to refresh bangumi token.`, "Bangumi", LogLevel.Error);
            autoLogException(e as Error);
            return false;
        }
    }

    /**
     * Check if the token exists.
     */
    private tokenExists() {
        return this.token.access_token && this.token.refresh_token && this.token.expires_in && this.token.token_type && this.token.user_id;
    }

    /**
     * Check if the token is valid.
     */
    private async checkToken() {
        if (!this.tokenExists()) {
            return false;
        }

        const url = "https://bgm.tv/oauth/token_status";
        const params = {access_token: this.token.access_token};
        try {
            await this.limiter.removeTokens(1);
            await axios.get(url, {params});
        } catch (e: any) {
            if (e.response.status === 401) {
                return false;
            } else {
                throw e;
            }
        }
        return true;
    }

    /**
     * Load token from file.
     */
    private loadToken() {
        if (fs.existsSync(config.cache_path + "/bangumi_token.json")) {
            let token = JSON.parse(fs.readFileSync(config.cache_path + "/bangumi_token.json", "utf8"));
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(token.expires_in);
            this.token.token_type = token.token_type;
            this.token.user_id = token.user_id;
        }
    }
}

export const bangumiClient = new BangumiClient();
