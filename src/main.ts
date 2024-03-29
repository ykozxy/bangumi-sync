import {buildDatabase, getChinaAnimeItem, releaseDatabase} from "./utils/data_util";
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
import {autoLog, autoLogException} from "./utils/log_util";
import {notify} from "node-notifier";
import {isServerMode, sleep} from "./utils/util";
import {config, reloadConfig} from "./utils/config_util";

async function singleMode(userConfirm: boolean) {
    autoLog("Initializing...", "Main")
    await buildDatabase();
    await bangumiClient.autoUpdateToken();
    await anilistClient.autoUpdateToken();
    autoLog("Finished.", "Main")
    await sleep(200);

    autoLog("Fetching Bangumi collections...", "Main")
    let bangumiCollection = await getBangumiCollections();
    autoLog("Fetching Anilist collections...", "Main")
    let anilistCollection = await getAnilistCollections();
    autoLog("Finished.", "Main")
    await sleep(200);

    autoLog("Matching collections...", "Main");
    bangumiCollection = await fillBangumiCollection(bangumiCollection);
    autoLog("Finished.", "Main");
    await sleep(200);

    autoLog("Generating changelog...", "Main");
    let changeLog = await generateChangelog(bangumiCollection, anilistCollection, config.sync_comments);
    for (let change of changeLog) {
        let name = "";
        if (change.after.bgm_id) {
            await getChinaAnimeItem(change.after.bgm_id, false).then(item => {
                if (item) {
                    name = item.title;
                }
            })
        }
        if (!name) name = <string>change.after.bgm_id;
        autoLog(`${name} (bgm=${change.after.bgm_id}, mal=${change.after.mal_id}):`, "RenderDiff");
        autoLog(renderDiff(change.before, change.after, config.sync_comments, "; "), "RenderDiff");
    }
    autoLog(`${changeLog.length} changes.`, "Main");

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
            let successCount = await anilistClient.smartUpdateCollection(changeLog.map(change => change.after), config.sync_comments);
            autoLog(`${successCount} changes successfully applied.`, "Main");
        }
    } else {
        await sleep(200);
        let successCount = await anilistClient.smartUpdateCollection(changeLog.map(change => change.after, config.sync_comments));
        autoLog(`${successCount} changes successfully applied.`, "Main");
    }
}

async function serverMode() {
    /* Initialize */
    autoLog("Getting tokens...", "Main");

    // Setup token auto-refresh every hour
    const refreshToken = async () => {
        await bangumiClient.autoUpdateToken();
        await anilistClient.autoUpdateToken();
        setTimeout(refreshToken, 60 * 60 * 1000);
    };
    await refreshToken();

    /* Main loop */
    while (1) {
        autoLog("Loading config...", "Main");
        reloadConfig();

        autoLog("Building database...", "Main");
        await buildDatabase();

        autoLog("Fetching Bangumi collections...", "Main");
        let bangumiCollection = await getBangumiCollections();
        autoLog("Fetching Anilist collections...", "Main");
        let anilistCollection = await getAnilistCollections();

        autoLog("Matching collections...", "Main");
        bangumiCollection = await fillBangumiCollection(bangumiCollection);

        autoLog("Generating changelog...", "Main");
        let changeLog = await generateChangelog(bangumiCollection, anilistCollection, config.sync_comments);

        for (let change of changeLog) {
            let name = "";
            if (change.after.bgm_id) {
                await getChinaAnimeItem(change.after.bgm_id, false).then(item => {
                    if (item) {
                        name = item.title;
                    }
                })
            }
            if (!name) name = <string>change.after.bgm_id;
            autoLog(`${name} (bgm=${change.after.bgm_id}, mal=${change.after.mal_id}):`, "RenderDiff");
            autoLog(renderDiff(change.before, change.after, config.sync_comments, "; "), "RenderDiff");
        }

        autoLog("Updating Anilist collections...", "Main");
        let successCount = await anilistClient.smartUpdateCollection(changeLog.map(change => change.after), config.sync_comments);
        autoLog(`${successCount} changes successfully applied.`, "Main");

        if (successCount != changeLog.length && config.enable_notifications) {
            notify({
                title: "Bangumi-Sync",
                message: `[Anilist] Failed to update ${changeLog.length - successCount} collections, see log for details.`,
            });
        }

        autoLog("Freeing memory...", "Main");
        releaseDatabase();

        autoLog(`Sleeping for ${config.server_mode_interval} seconds...`, "Main");
        await sleep(config.server_mode_interval * 1000);
    }
}

if (isServerMode) {
    autoLog("Running in server mode.", "Main");
    serverMode().then(() => {
        process.exit(0);
    }).catch(e => {
        autoLogException(e);
        if (config.enable_notifications) {
            notify({
                title: "Bangumi-Sync",
                message: `Unhandled exception, exiting: ${e.message}`,
            }, () => {
                process.exit(1);
            });
        } else {
            process.exit(1);
        }
    })
} else {
    // Check pm2 to see if another instance is running
    let exec = require('child_process').exec;
    exec('pm2 list', (err: Error, stdout: string) => {
        if (!err) {
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
        }
        // Start script
        autoLog("Running in single mode.", "Main");
        singleMode(config.manual_confirm).then(() => {
            process.exit(0);
        }).catch(e => {
            autoLogException(e);
            process.exit(1);
        });
    });
}
