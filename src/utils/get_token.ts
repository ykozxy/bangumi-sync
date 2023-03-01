import {bangumiClient} from "./bangumi_client";
import {anilistClient} from "./anilist_client";

(async () => {
    await bangumiClient.autoUpdateToken();
    await anilistClient.autoUpdateToken();
})().then(() => {
    console.log("done");
    process.exit(0);
});
