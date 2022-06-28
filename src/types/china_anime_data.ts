import {autoLog, LogLevel} from "../utils/log_util";

export namespace ChinaAnimeData {
    export interface Item {
        title: string;
        titleTranslate: {
            en?: string[];
            ja?: string[];
            "zh-Hans"?: string[];
            "zh-Hant"?: string[];
        };
        type: Type;
        begin: Date;
        end?: Date;
        sites: SiteElement[];
    }

    export enum Type {
        Movie = "movie",
        Ova = "ova",
        Tv = "tv",
        Web = "web",
    }

    interface SiteElement {
        site: string;
        id: string;
        begin?: Date;
        broadcast?: Date;
        url?: string;
        comment?: string;
    }

    export function loadData(json: any): Item[] {
        return json.items.reduce((acc: Item[], item: any) => {
            let type: Type;
            switch (item.type) {
                case "movie":
                    type = Type.Movie;
                    break;
                case "ova":
                    type = Type.Ova;
                    break;
                case "tv":
                    type = Type.Tv;
                    break;
                case "web":
                    type = Type.Web;
                    break;
                default:
                    autoLog(`Unknown type ${item.type} when processing ${item.title}`, "loadChinaAnimeData", LogLevel.Error);
                    return acc;
            }

            acc.push({
                title: item.title,
                titleTranslate: item.titleTranslate,
                type: type,
                begin: new Date(item.begin),
                end: new Date(item.end),
                sites: item.sites.filter((site: any) => site.id !== "").map((site: any) => ({
                    site: site.site,
                    id: site.id,
                    begin: site.begin ? new Date(site.begin) : undefined,
                    broadcast: site.broadcast ? new Date(site.broadcast) : undefined,
                    url: site.url,
                    comment: site.comment,
                })),
            });
            return acc;
        }, []);
    }
}
