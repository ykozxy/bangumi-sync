import {bangumiClient} from "./bangumi_client";
import {anilistClient} from "./anilist_client";
import {config} from "./config_util";

(async () => {
    await bangumiClient.autoUpdateToken();
    if (config.sync_to_anilist !== false) {
        await anilistClient.autoUpdateToken();
    }
})().then(() => {
    console.log("done");
    process.exit(0);
});
