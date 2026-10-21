import {bangumiClient} from "./bangumi_client";
import {anilistClient} from "./anilist_client";
import {malClient} from "./mal_client";
import {config} from "./config_util";

(async () => {
    if (config.sync_to_mal) malClient.validateConfiguration();
    await bangumiClient.autoUpdateToken();
    if (config.sync_to_anilist) {
        await anilistClient.autoUpdateToken();
    }
    if (config.sync_to_mal) {
        await malClient.autoUpdateToken();
    }
})().then(() => {
    console.log("done");
    process.exit(0);
}).catch((error: Error) => {
    console.error(error.message);
    process.exit(1);
});
