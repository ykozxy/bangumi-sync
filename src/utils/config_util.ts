import {Config, IgnoreEntries, ManualRelations} from "../types/config";

export let config: Config = require("../../config/config.json");
export let manual_relations: ManualRelations = require("../../config/manual_relations.json");
export let ignore_entries: IgnoreEntries = require("../../config/ignore_entries.json");

export function reloadConfig() {
    config = require("../config/config.json");
    manual_relations = require("../config/manual_relations.json");
    ignore_entries = require("../config/ignore_entries.json");
}

