import {buildDatabase, getChinaAnimeItem, matchChinaToGlobal} from "./utils/data_util";
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

async function main() {
    console.log("Initializing...")
    await buildDatabase();
    console.log("Database initialized.\n");

    await bangumiClient.checkToken();
    await anilistClient.checkToken();
    console.log("Finished.\n")
    await setTimeout(() => {
    }, 200);

    console.log("Fetching Bangumi collections...")
    let bangumiCollection = await getBangumiCollections();
    console.log("Fetching Anilist collections...")
    let anilistCollection = await getAnilistCollections();
    console.log("Finished.\n")
    await setTimeout(() => {
    }, 200);

    console.log("Matching collections...");
    bangumiCollection = await fillBangumiCollection(bangumiCollection);
    console.log("Finished.\n");
    await setTimeout(() => {
    }, 200);

    console.log("Generating changelog...");
    let changeLog = await generateChangelog(bangumiCollection, anilistCollection);
    for (let change of changeLog) {
        let name = ""
        if (change.after.bgm_id) {
            await getChinaAnimeItem(change.after.bgm_id).then(item => {
                if (item) {
                    name = item.title;
                }
            })
        }
        if (!name) name = <string>change.after.bgm_id;
        console.log(`${name} (bgm=${change.after.bgm_id}, mal=${change.after.mal_id}):`);
        console.log(renderDiff(change.before, change.after));
    }
    console.log(`${changeLog.length} changes.`);
    let confirm = await new Promise<boolean>((resolve, reject) => {
        let rl = readline.createInterface({
            input: process.stdin,
            output: process.stdout
        });
        rl.question("Confirm? (y/n) ", (answer) => {
            rl.close();
            resolve(answer.toLowerCase() === "y" || answer === "");
        });
    });

    let successCount = await anilistClient.smartUpdateCollection(changeLog.map(change => change.after));
    console.log(`${successCount} changes successfully updated.`);
}

async function test() {
    // CN: 342667 296739 298477 767 812 233
    // 285666 840 72767 7707 253
    console.log("Initializing...")
    await buildDatabase();
    await bangumiClient.checkToken();
    await anilistClient.checkToken();
    console.log("Finished.\n")

    let cn = await getChinaAnimeItem("840");
    if (cn) {
        let global = await matchChinaToGlobal(cn);
        console.log(global);

        let res = await anilistClient.searchAnime(cn.title);
        console.log(res);
    }
}

// test().then(() => process.exit(0));
main().then(() => process.exit(0));

