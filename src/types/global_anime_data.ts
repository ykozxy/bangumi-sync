import {autoLog, LogLevel} from "../utils/log_util";

export namespace GlobalAnimeData {
    export interface Item {
        title: string;
        synonyms: string[];
        type: Type;
        episodes: number;
        status: Status;
        animeSeason: {
            season: Season;
            year?: number;
        };
        sites: { [key in Site]?: string };
        relations: string[];
        tags: string[];
    }

    export enum Season {
        Fall = "FALL",
        Spring = "SPRING",
        Summer = "SUMMER",
        Undefined = "UNDEFINED",
        Winter = "WINTER",
    }

    export enum Status {
        Finished = "FINISHED",
        Ongoing = "ONGOING",
        Unknown = "UNKNOWN",
        Upcoming = "UPCOMING",
    }

    export enum Type {
        Movie = "MOVIE",
        Ona = "ONA",
        Ova = "OVA",
        Special = "SPECIAL",
        Tv = "TV",
        Unknown = "UNKNOWN",
    }

    enum Site {
        AniDB = "AniDB",
        AniList = "AniList",
        AnimeCountdown = "AnimeCountdown",
        AnimeNewsNetwork = "AnimeNewsNetwork",
        AnimePlanet = "AnimePlanet",
        AnimeSearch = "AnimeSearch",
        Kitsu = "Kitsu",
        LiveChart = "LiveChart",
        MyAnimeList = "MyAnimeList",
        NotifyMoe = "NotifyMoe",
        Simkl = "Simkl",
    }

    export function loadData(json: any): Item[] {
        return json.data.reduce((acc: Item[], item: any) => {
            // if (!item.animeSeason.year) return acc;

            // Parse sites
            let sites = item.sources.reduce((acc: { [key in Site]: string }, site: string) => {
                let res: RegExpMatchArray | null;

                if ((res = site.match(/anidb.net\/anime\/(\d+)$/)) && res[1]) {
                    acc["AniDB"] = res[1];
                } else if ((res = site.match(/anilist.co\/anime\/(\d+)$/)) && res[1]) {
                    acc["AniList"] = res[1];
                } else if ((res = site.match(/animecountdown.com\/(\d+)$/)) && res[1]) {
                    acc["AnimeCountdown"] = res[1];
                } else if ((res = site.match(/animenewsnetwork.com\/encyclopedia\/anime.php\?id=(\d+)$/)) && res[1]) {
                    acc["AnimeNewsNetwork"] = res[1];
                } else if ((res = site.match(/anime-planet.com\/anime\/(.+)$/)) && res[1]) {
                    acc["AnimePlanet"] = res[1];
                } else if ((res = site.match(/anisearch.com\/anime\/(\d+)$/)) && res[1]) {
                    acc["AnimeSearch"] = res[1];
                } else if ((res = site.match(/kitsu.(app|io)\/anime\/(\d+)$/)) && res[1]) {
                    acc["Kitsu"] = res[1];
                } else if ((res = site.match(/livechart.me\/anime\/(\d+)$/)) && res[1]) {
                    acc["LiveChart"] = res[1];
                } else if ((res = site.match(/myanimelist.net\/anime\/(\d+)$/)) && res[1]) {
                    acc["MyAnimeList"] = res[1];
                } else if ((res = site.match(/notify.moe\/anime\/(.+)$/)) && res[1]) {
                    acc["NotifyMoe"] = res[1];
                } else if ((res = site.match(/simkl.com\/anime\/(\d+)$/)) && res[1]) {
                    acc["Simkl"] = res[1];
                } else {
                    autoLog(`Cannot parse ${site} when processing ${item.title}`, "loadGlobalAnimeData", LogLevel.Error);
                }

                return acc;
            }, {} as { [key in Site]: string });

            acc.push({
                title: item.title,
                synonyms: item.synonyms,
                type: item.type,
                episodes: item.episodes,
                status: item.status,
                animeSeason: {
                    season: item.animeSeason.season,
                    year: item.animeSeason.year,
                },
                sites,
                relations: item.relations,
                tags: item.tags,
            });

            return acc;
        }, []);
    }
}
