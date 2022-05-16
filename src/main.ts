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
import {autoLog, autoLogException} from "./utils/log_util";

const config: Config = require("../config.json");

function sleep(ms: number) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

async function singleMode(userConfirm: boolean) {
    autoLog("Initializing...")
    await buildDatabase();
    await bangumiClient.checkToken();
    await anilistClient.checkToken();
    autoLog("Finished.")
    await sleep(200);

    autoLog("Fetching Bangumi collections...")
    let bangumiCollection = await getBangumiCollections();
    autoLog("Fetching Anilist collections...")
    let anilistCollection = await getAnilistCollections();
    autoLog("Finished.")
    await sleep(200);

    autoLog("Matching collections...");
    bangumiCollection = await fillBangumiCollection(bangumiCollection);
    autoLog("Finished.\n");
    await sleep(200);

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
    autoLog(`${changeLog.length} changes.`);

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
            autoLog(`${successCount} changes successfully applied.`);
        }
    } else {
        await sleep(200);
        let successCount = await anilistClient.smartUpdateCollection(changeLog.map(change => change.after));
        autoLog(`${successCount} changes successfully applied.`);
    }
}


async function serverMode() {
    /* Initialize */
    autoLog("Initializing...", "Main");
    // Setup timer to update database every 12 hours
    const updateDatabase = async () => {
        await buildDatabase();
        setTimeout(updateDatabase, 12 * 60 * 60 * 1000);
    };
    await updateDatabase();

    // Setup token auto-refresh every hour
    const refreshToken = async () => {
        await bangumiClient.checkToken();
        await anilistClient.checkToken();
        setTimeout(refreshToken, 60 * 60 * 1000);
    };
    await refreshToken();

    /* Main loop */
    while (1) {
        autoLog("Fetching Bangumi collections...", "Main");
        let bangumiCollection = await getBangumiCollections();
        autoLog("Fetching Anilist collections...", "Main");
        let anilistCollection = await getAnilistCollections();

        autoLog("Matching collections...", "Main");
        bangumiCollection = await fillBangumiCollection(bangumiCollection);

        autoLog("Generating changelog...", "Main");
        let changeLog = await generateChangelog(bangumiCollection, anilistCollection);

        autoLog("Updating Anilist collections...", "Main");
        let successCount = await anilistClient.smartUpdateCollection(changeLog.map(change => change.after));
        autoLog(`${successCount} changes successfully applied.`, "Main");

        autoLog(`Sleeping for ${config.server_mode_interval} seconds...`, "Main");
        await sleep(config.server_mode_interval * 1000);
    }
}


if (process.argv[2] === "--server") {
    autoLog("Running in server mode.", "Main");
    serverMode().then(() => {
        process.exit(0);
    }).catch(e => {
        autoLogException(e);
        process.exit(1);
    })
} else {
    // Check pm2 to see if another instance is running
    let exec = require('child_process').exec;
    exec('pm2 list', (err: Error, stdout: string) => {
        if (err) {
            autoLogException(err);
            return;
        }
        let lines = stdout.split("\n");
        let running = false;
        for (let line of lines) {
            if (line.indexOf("bangumi-sync") !== -1) {
                if (line.indexOf("online") !== -1) {
                    running = true;
                }
                break;
            }
        }
        if (running) {
            autoLog("Another server-mode instance is running. Exiting...", "Main");
            process.exit(0);
        }

        // Start script
        autoLog("Running in single mode.");
        singleMode(config.manual_confirm).then(() => {
            process.exit(0);
        }).catch(e => {
            autoLogException(e);
            process.exit(1);
        });
    });
}