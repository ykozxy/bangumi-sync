import {RateLimiter} from "limiter";
import axios from "axios";
import {createServer, IncomingMessage, ServerResponse} from "http";
import fs from "fs";
import crypto from "crypto";
import {MalAnimeListResponse, MalListStatus, MalWatchStatus} from "../types/mal_api";
import {AnimeCollection, CollectionStatus} from "../types/anime_collection";
import {
    autoLog,
    autoLogException,
    createProgressBar,
    incrementProgressBar,
    LogLevel,
    stopProgressBar
} from "./log_util";
import {isServerMode, sleep} from "./util";
import {config} from "./config_util";
import open from "open";

class MalClient {
    private readonly limiter: RateLimiter;
    private readonly api_url: string = "https://api.myanimelist.net/v2";
    private readonly oauth_url: string = "https://myanimelist.net/v1/oauth2";
    private readonly client_id: string = "TODO";
    private readonly client_secret: string = "TODO";
    private readonly token = {
        access_token: "",
        refresh_token: "",
        expires_in: new Date(),
        token_type: "",
    };

    constructor() {
        // MAL rate limit is not well-documented; be conservative
        this.limiter = new RateLimiter({tokensPerInterval: 3, interval: 1000});
    }

    private get headers(): { [key: string]: string } {
        return {
            "Authorization": `${this.token.token_type} ${this.token.access_token}`,
        };
    }

    private static convertStatus(collectionStatus: CollectionStatus): MalWatchStatus {
        switch (collectionStatus) {
            case CollectionStatus.Watching:
                return "watching";
            case CollectionStatus.Completed:
                return "completed";
            case CollectionStatus.OnHold:
                return "on_hold";
            case CollectionStatus.Dropped:
                return "dropped";
            case CollectionStatus.PlanToWatch:
                return "plan_to_watch";
        }
    }

    /**
     * Get anime collection of the current user.
     */
    public async getAnimeCollection(): Promise<{ node: { id: number; title: string }; list_status: MalListStatus }[] | null> {
        let result: { node: { id: number; title: string }; list_status: MalListStatus }[] = [];
        let url: string | null = `${this.api_url}/users/@me/animelist?fields=list_status{status,score,num_episodes_watched,comments,updated_at,finish_date}&limit=1000&nsfw=true`;

        while (url) {
            await this.limiter.removeTokens(1);
            try {
                const response = await axios.get(url, {headers: this.headers});
                const data: MalAnimeListResponse = response.data;
                result = result.concat(data.data);
                url = data.paging?.next || null;
            } catch (e: any) {
                autoLog(`Network error when fetching anime collection.`, "MAL", LogLevel.Error);
                autoLogException(e as Error);
                return null;
            }
        }
        return result;
    }

    /**
     * Update or add an anime entry in the user's list.
     * @param collection The collection entry to update.
     * @param syncComment Whether to sync comments.
     */
    public async saveEntry(collection: AnimeCollection, syncComment: boolean = false): Promise<boolean> {
        if (!collection.mal_id) {
            autoLog(`Failed to save ${collection.title}, empty MAL ID.`, "MAL.saveEntry", LogLevel.Error);
            return false;
        }

        const url = `${this.api_url}/anime/${collection.mal_id}/my_list_status`;
        const params = new URLSearchParams();
        params.append("status", MalClient.convertStatus(collection.status));
        params.append("num_watched_episodes", String(collection.watched_episodes));

        // Only set score when non-zero (rated)
        if (collection.score > 0) {
            params.append("score", String(collection.score));
        }
        if (syncComment && collection.comments) {
            params.append("comments", collection.comments);
        }
        // Sync finish date when status is Completed
        if (collection.status === CollectionStatus.Completed && collection.completed_at) {
            const y = collection.completed_at.getFullYear();
            const m = String(collection.completed_at.getMonth() + 1).padStart(2, '0');
            const d = String(collection.completed_at.getDate()).padStart(2, '0');
            params.append("finish_date", `${y}-${m}-${d}`);
        }

        await this.limiter.removeTokens(1);
        try {
            await axios.put(url, params.toString(), {
                headers: {
                    ...this.headers,
                    "Content-Type": "application/x-www-form-urlencoded",
                },
            });
            return true;
        } catch (e: any) {
            autoLog(`Failed to save ${collection.title} (mal=${collection.mal_id}). Error: ${e}`, "MAL.saveEntry", LogLevel.Error);
            return false;
        }
    }

    /**
     * Smart update the anime collection based on the given list.
     * @param collection The list of anime to update.
     * @param syncComment Whether to sync comments.
     */
    public async smartUpdateCollection(collection: AnimeCollection[], syncComment: boolean = false): Promise<number> {
        createProgressBar(collection.length);
        let successCount = 0;

        for (let c of collection) {
            if (!c.mal_id) {
                autoLog(`Skipping ${c.title}, no MAL ID.`, "MAL.smartUpdateCollection", LogLevel.Warn);
                incrementProgressBar();
                continue;
            }
            if (await this.saveEntry(c, syncComment)) {
                successCount++;
            }
            incrementProgressBar();
        }

        stopProgressBar();
        return successCount;
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
            autoLog("Failed to refresh MAL token.", "MAL", LogLevel.Warn);
            await this.getToken();
        }

        // Save token to file
        if (!fs.existsSync(config.cache_path)) {
            fs.mkdirSync(config.cache_path);
        }
        fs.writeFileSync(config.cache_path + "/mal_token.json", JSON.stringify(this.token));

        // Verify token works
        await this.limiter.removeTokens(1);
        try {
            await axios.get(`${this.api_url}/users/@me`, {headers: this.headers});
        } catch (e: any) {
            autoLog("MAL token verification failed.", "MAL", LogLevel.Error);
            autoLogException(e as Error);
            await this.getToken();
        }
    }

    private async getToken() {
        // Handle server mode
        if (isServerMode) {
            autoLog("MAL token expired. Please run `npm run token` to get new token.", "MAL", LogLevel.Error);
            autoLog("Waiting for token...", "MAL", LogLevel.Info);

            // Wait until token is available
            while (1) {
                this.loadToken();
                if (this.tokenExists() && await this.refreshToken())
                    break;
                await sleep(5000);
            }

            return;
        }

        // Generate PKCE code verifier (plain method)
        const codeVerifier = crypto.randomBytes(64).toString("base64url").substring(0, 128);

        // Setup callback server
        let code: string = "";
        const server = createServer((request: IncomingMessage, response: ServerResponse) => {
            const html = "<html lang='en'><head><title>[BGM-Sync] MAL Token generated</title></head><body><h1>MAL Token generated! Please close this window.</h1></body></html>";
            response.writeHead(200, {
                "Content-Type": "text/html",
                "Content-Length": Buffer.byteLength(html),
            });
            response.end(html);

            if (request.url?.includes("?code=")) {
                const url = new URL(request.url, "http://localhost:3500");
                code = url.searchParams.get("code") || "";
            }
            server.close();
        });
        server.listen(3500);

        const auth_url = `${this.oauth_url}/authorize?response_type=code&client_id=${this.client_id}&client_secret=${this.client_secret}&code_challenge=${codeVerifier}&code_challenge_method=plain&redirect_uri=http://localhost:3500`;
        autoLog(`If auto open browser failed, please visit this link manually to authorize with MAL: ${auth_url}`, "MAL");
        await open(auth_url);

        await new Promise((resolve) => {
            function check() {
                if (code) resolve(code);
                else setTimeout(check, 500);
            }

            check();
        });

        // Exchange code for token
        const token_url = `${this.oauth_url}/token`;
        const params = new URLSearchParams();
        params.append("client_id", this.client_id);
        params.append("client_secret", this.client_secret);
        params.append("grant_type", "authorization_code");
        params.append("code", code);
        params.append("redirect_uri", "http://localhost:3500");
        params.append("code_verifier", codeVerifier);

        try {
            const response = await axios.post(token_url, params.toString(), {
                headers: {"Content-Type": "application/x-www-form-urlencoded"},
            });
            let token = response.data;
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(Date.now() + token.expires_in * 1000);
            this.token.token_type = token.token_type;
        } catch (e: any) {
            autoLog("Failed to get MAL token.", "MAL", LogLevel.Error);
            autoLogException(e as Error);
            process.exit(1);
        }
    }

    private async refreshToken(): Promise<boolean> {
        const url = `${this.oauth_url}/token`;
        const params = new URLSearchParams();
        params.append("client_id", this.client_id);
        params.append("client_secret", this.client_secret);
        params.append("grant_type", "refresh_token");
        params.append("refresh_token", this.token.refresh_token);

        try {
            const response = await axios.post(url, params.toString(), {
                headers: {"Content-Type": "application/x-www-form-urlencoded"},
            });
            let token = response.data;
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(Date.now() + token.expires_in * 1000);
            this.token.token_type = token.token_type;
            return true;
        } catch (e: any) {
            autoLog("Failed to refresh MAL token.", "MAL", LogLevel.Error);
            autoLogException(e as Error);
            return false;
        }
    }

    private tokenExists() {
        return this.token.access_token && this.token.refresh_token && this.token.expires_in && this.token.token_type;
    }

    private loadToken() {
        if (fs.existsSync(config.cache_path + "/mal_token.json")) {
            let token = JSON.parse(fs.readFileSync(config.cache_path + "/mal_token.json", "utf8"));
            this.token.access_token = token.access_token;
            this.token.refresh_token = token.refresh_token;
            this.token.expires_in = new Date(token.expires_in);
            this.token.token_type = token.token_type;
        }
    }
}

export const malClient = new MalClient();
