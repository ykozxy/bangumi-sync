import {buildDatabase, getChinaAnimeItem} from "./utils/data_util";
import {bangumiClient} from "./utils/bangumi_client";
import {anilistClient} from "./utils/anilist_client";
import {
    fillBangumiCollection,
    generateChangelog,
    getAnilistCollections,
    getBangumiCollections,
    renderDiff
} from "./utils/sync_util";
import * as readline from "readline";
import {Config} from "./types/config";
import {autoLog} from "./utils/log_util";

const config: Config = require("../config.json");

async function main(userConfirm: boolean) {
    autoLog("Initializing...")
    await buildDatabase();
    autoLog("Database initialized.\n");

    await bangumiClient.checkToken();
    await anilistClient.checkToken();
    autoLog("Finished.\n")
    await setTimeout(() => {
    }, 200);

    autoLog("Fetching Bangumi collections...")
    let bangumiCollection = await getBangumiCollections();
    autoLog("Fetching Anilist collections...")
    let anilistCollection = await getAnilistCollections();
    autoLog("Finished.\n")
    await setTimeout(() => {
    }, 200);

    autoLog("Matching collections...");
    bangumiCollection = await fillBangumiCollection(bangumiCollection);
    autoLog("Finished.\n");
    await setTimeout(() => {
    }, 200);

    autoLog("Generating changelog...");
    let changeLog = await generateChangelog(bangumiCollection, anilistCollection);
    for (let change of changeLog) {
        let name = "";
        if (change.after.bgm_id) {
            await getChinaAnimeItem(change.after.bgm_id).then(item => {
                if (item) {
                    name = item.title;
                }
            })
        }
        if (!name) name = <string>change.after.bgm_id;
        autoLog(`${name} (bgm=${change.after.bgm_id}, mal=${change.after.mal_id}):`);
        autoLog(renderDiff(change.before, change.after));
    }
    autoLog(`${changeLog.length} changes.\n`);

    if (changeLog.length === 0) {
        return;
    }

    if (userConfirm) {
        let confirm = await new Promise<boolean>((resolve) => {
            let rl = readline.createInterface({
                input: process.stdin,
                output: process.stdout
            });
            rl.question("Confirm? (y/n) ", (answer) => {
                rl.close();
                resolve(answer.toLowerCase() === "y" || answer === "");
            });
        });
        if (confirm) {
            let successCount = await anilistClient.smartUpdateCollection(changeLog.map(change => change.after));
            autoLog(`${successCount} changes successfully updated.`);
        }
    } else {
        await setTimeout(() => {
        }, 200);
        let successCount = await anilistClient.smartUpdateCollection(changeLog.map(change => change.after));
        autoLog(`${successCount} changes successfully updated.`);
    }
}

main(config.manual_confirm).then(() => process.exit(0));

