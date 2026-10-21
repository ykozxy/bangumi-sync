import {Config, IgnoreEntries, ManualRelations} from "../types/config";

export let config: Config = loadConfig();
export let manual_relations: ManualRelations = require("../../config/manual_relations.json");
export let ignore_entries: IgnoreEntries = require("../../config/ignore_entries.json");

/**
 * @description Load config.json and fill in defaults for fields missing from older configs
 */
function loadConfig(): Config {
    const raw = require("../../config/config.json");
    return {
        ...raw,
        sync_to_anilist: raw.sync_to_anilist ?? true,
        sync_to_mal: raw.sync_to_mal ?? false,
    };
}

export function reloadConfig() {
    config = loadConfig();
    manual_relations = require("../../config/manual_relations.json");
    ignore_entries = require("../../config/ignore_entries.json");
}
