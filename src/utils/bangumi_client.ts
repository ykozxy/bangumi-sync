import {BangumiComponents, BangumiOperations} from "../types/bangumi_api";
import axios from "axios";
import {RateLimiter} from "limiter";
import {Config} from "../types/config";
import {createServer, IncomingMessage, ServerResponse} from 'http';
import fs from "fs";
import {memoize} from "decko";
import open from "open";

const config: Config = require("../../config.json");

class BangumiClient {
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

    constructor() {
        this.limiter = new RateLimiter({tokensPerInterval: 2, interval: 1000});
    }

    private get headers(): { [key: string]: string } {
        return {
            "Authorization": `${this.token.token_type} ${this.token.access_token}`,
            "User-Agent": "Bangumi-Sync/1.0.0",
        }
    }

    public async checkToken() {
        function tokenExists(this: BangumiClient) {
            return this.token.access_token && this.token.refresh_token && this.token.expires_in && this.token.token_type && this.token.user_id
        }

        // Load token from file
        if (!tokenExists.call(this)) {
            if (fs.existsSync(config.cache_path + "/bangumi_token.json")) {
                let token = JSON.parse(fs.readFileSync(config.cache_path + "/bangumi_token.json", "utf8"));
                this.token.access_token = token.access_token;
                this.token.refresh_token = token.refresh_token;
                this.token.expires_in = new Date(token.expires_in);
                this.token.token_type = token.token_type;
                this.token.user_id = token.user_id;
            }
        }

        // Check if the token doesn't exist
        if (!tokenExists.call(this)) {
            await this.getToken();
        }

        // Refresh the token
        if (!await this.refreshToken()) {
            console.error("[Bangumi] Failed to refresh bangumi token.");
            await this.getToken();
        }

        // Check token from site
        const url = "https://bgm.tv/oauth/token_status";
        const params = {access_token: this.token.access_token};
        try {
            await this.limiter.removeTokens(1);
            await axios.get(url, {params});
        } catch (e: any) {
            if (e.response.status === 401) {
                await this.getToken();
            } else {
                throw e;
            }
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
                console.error("[Bangumi] Cannot get user info.")
                console.error(e);
                process.exit(1);
            }
        }
    }

    @memoize
    public async getSubjectById(bgm_id: string, retry: boolean = true): Promise<BangumiComponents["schemas"]["Subject"] | null> {
        await this.limiter.removeTokens(1);
        const url = this.api_url + `/v0/subjects/${bgm_id}`;
        try {
            const response = await axios.get(url, {headers: this.headers});
            return response.data as BangumiComponents["schemas"]["Subject"];
        } catch (e: any) {
            console.error(`[Bangumi] Network error when fetching subject ${bgm_id}`);
            console.error(e.response);

            if (retry) {
                await setTimeout(() => {
                }, 1000);
                return await this.getSubjectById(bgm_id, false);
            }
            return null;
        }
    }

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
                console.error(`[Bangumi] Error when fetching anime collection.`);
                console.error(e.response);
                return null;
            }

            numFetched += 50;
            total = data.total as number;
            collections = collections.concat(data.data as BangumiComponents["schemas"]["UserCollection"][]);
            // process.stdout.write(".");
        }
        // console.log();
        return collections;
    }

    /**
     * Prompt the user to get a new token.
     */
    private async getToken(): Promise<void> {
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
        // console.log(`[Bangumi] Open the url to authorize with bgm.tv: ${auth_url}`);
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
            console.error("[Bangumi] Failed to get bangumi token.");
            console.error(e.response);
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
            console.error("[Bangumi] Failed to refresh bangumi token.");
            console.error(e);
            return false;
        }
    }
}

export const bangumiClient = new BangumiClient();
