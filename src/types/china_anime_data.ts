// To parse this data:
//
//   import { Convert, ChinaAnimeData } from "./file";
//
//   const chinaAnimeData = Convert.toChinaAnimeData(json);
//
// These functions will throw an error if the JSON doesn't
// match the expected interface, even if the JSON is valid.
// noinspection SpellCheckingInspection

export interface ChinaAnimeData {
    siteMeta: { [key: string]: SiteMeta };
    items: ChinaAnimeItem[];
}

export interface ChinaAnimeItem {
    title: string;
    titleTranslate: TitleTranslate;
    type: ChinaAnimeType;
    lang: Lang;
    officialSite: string;
    begin: Date;
    end: string;
    sites: SiteElement[];
    broadcast?: string;
    comment?: string;
}

export enum Lang {
    En = "en",
    Ja = "ja",
    ZhHans = "zh-Hans",
}

export interface SiteElement {
    site: SiteEnum;
    id?: string;
    begin?: string;
    broadcast?: string;
    url?: string;
    comment?: string;
}

export enum SiteEnum {
    Acfun = "acfun",
    AniOneAsia = "ani_one_asia",
    Bangumi = "bangumi",
    Bilibili = "bilibili",
    BilibiliTw = "bilibili_tw",
    BilibiliHkTw = "bilibili_hk_mo",
    BilibiliHkMoTw = "bilibili_hk_mo_tw",
    Dmhy = "dmhy",
    Gamer = "gamer",
    Iqiyi = "iqiyi",
    Letv = "letv",
    Mgtv = "mgtv",
    MuseHk = "muse_hk",
    Netflix = "netflix",
    Nicovideo = "nicovideo",
    Pptv = "pptv",
    Qq = "qq",
    Sohu = "sohu",
    Viu = "viu",
    Youku = "youku",
}

interface TitleTranslate {
    "zh-Hans"?: string[];
    en?: string[];
    "zh-Hant"?: string[];
    ja?: string[];
}

export enum ChinaAnimeType {
    Movie = "movie",
    Ova = "ova",
    Tv = "tv",
    Web = "web",
}

interface SiteMeta {
    title: string;
    urlTemplate: string;
    regions?: string[];
    type: SiteMetaType;
}

enum SiteMetaType {
    Info = "info",
    Onair = "onair",
    Resource = "resource",
}

// Converts JSON strings to/from your types
// and asserts the results of JSON.parse at runtime
export class ConvertChinaAnime {
    public static toChinaAnimeData(json: string): ChinaAnimeData {
        return cast(JSON.parse(json), r("ChinaAnimeData"));
    }

    public static chinaAnimeDataToJson(value: ChinaAnimeData): string {
        return JSON.stringify(uncast(value, r("ChinaAnimeData")), null, 2);
    }
}

function invalidValue(typ: any, val: any, key: any = ''): never {
    if (key) {
        throw Error(`Invalid value for key "${key}". Expected type ${JSON.stringify(typ)} but got ${JSON.stringify(val)}`);
    }
    throw Error(`Invalid value ${JSON.stringify(val)} for type ${JSON.stringify(typ)}`,);
}

function jsonToJSProps(typ: any): any {
    if (typ.jsonToJS === undefined) {
        const map: any = {};
        typ.props.forEach((p: any) => map[p.json] = {key: p.js, typ: p.typ});
        typ.jsonToJS = map;
    }
    return typ.jsonToJS;
}

function jsToJSONProps(typ: any): any {
    if (typ.jsToJSON === undefined) {
        const map: any = {};
        typ.props.forEach((p: any) => map[p.js] = {key: p.json, typ: p.typ});
        typ.jsToJSON = map;
    }
    return typ.jsToJSON;
}

function transform(val: any, typ: any, getProps: any, key: any = ''): any {
    function transformPrimitive(typ: string, val: any): any {
        if (typeof typ === typeof val) return val;
        return invalidValue(typ, val, key);
    }

    function transformUnion(typs: any[], val: any): any {
        // val must validate against one typ in typs
        const l = typs.length;
        for (let i = 0; i < l; i++) {
            const typ = typs[i];
            try {
                return transform(val, typ, getProps);
            } catch (_) {
            }
        }
        return invalidValue(typs, val);
    }

    function transformEnum(cases: string[], val: any): any {
        if (cases.indexOf(val) !== -1) return val;
        return invalidValue(cases, val);
    }

    function transformArray(typ: any, val: any): any {
        // val must be an array with no invalid elements
        if (!Array.isArray(val)) return invalidValue("array", val);
        return val.map(el => transform(el, typ, getProps));
    }

    function transformDate(val: any): any {
        if (val === null) {
            return null;
        }
        const d = new Date(val);
        if (isNaN(d.valueOf())) {
            return invalidValue("Date", val);
        }
        return d;
    }

    function transformObject(props: { [k: string]: any }, additional: any, val: any): any {
        if (val === null || typeof val !== "object" || Array.isArray(val)) {
            return invalidValue("object", val);
        }
        const result: any = {};
        Object.getOwnPropertyNames(props).forEach(key => {
            const prop = props[key];
            const v = Object.prototype.hasOwnProperty.call(val, key) ? val[key] : undefined;
            result[prop.key] = transform(v, prop.typ, getProps, prop.key);
        });
        Object.getOwnPropertyNames(val).forEach(key => {
            if (!Object.prototype.hasOwnProperty.call(props, key)) {
                result[key] = transform(val[key], additional, getProps, key);
            }
        });
        return result;
    }

    if (typ === "any") return val;
    if (typ === null) {
        if (val === null) return val;
        return invalidValue(typ, val);
    }
    if (typ === false) return invalidValue(typ, val);
    while (typeof typ === "object" && typ.ref !== undefined) {
        typ = typeMap[typ.ref];
    }
    if (Array.isArray(typ)) return transformEnum(typ, val);
    if (typeof typ === "object") {
        return typ.hasOwnProperty("unionMembers") ? transformUnion(typ.unionMembers, val)
            : typ.hasOwnProperty("arrayItems") ? transformArray(typ.arrayItems, val)
                : typ.hasOwnProperty("props") ? transformObject(getProps(typ), typ.additional, val)
                    : invalidValue(typ, val);
    }
    // Numbers can be parsed by Date but shouldn't be.
    if (typ === Date && typeof val !== "number") return transformDate(val);
    return transformPrimitive(typ, val);
}

function cast<T>(val: any, typ: any): T {
    return transform(val, typ, jsonToJSProps);
}

function uncast<T>(val: T, typ: any): any {
    return transform(val, typ, jsToJSONProps);
}

function a(typ: any) {
    return {arrayItems: typ};
}

function u(...typs: any[]) {
    return {unionMembers: typs};
}

function o(props: any[], additional: any) {
    return {props, additional};
}

function m(additional: any) {
    return {props: [], additional};
}

function r(name: string) {
    return {ref: name};
}

const typeMap: any = {
    "ChinaAnimeData": o([
        {json: "siteMeta", js: "siteMeta", typ: m(r("SiteMeta"))},
        {json: "items", js: "items", typ: a(r("Item"))},
    ], false),
    "Item": o([
        {json: "title", js: "title", typ: ""},
        {json: "titleTranslate", js: "titleTranslate", typ: r("TitleTranslate")},
        {json: "type", js: "type", typ: r("ItemType")},
        {json: "lang", js: "lang", typ: r("Lang")},
        {json: "officialSite", js: "officialSite", typ: ""},
        {json: "begin", js: "begin", typ: Date},
        {json: "end", js: "end", typ: ""},
        {json: "sites", js: "sites", typ: a(r("SiteElement"))},
        {json: "broadcast", js: "broadcast", typ: u(undefined, "")},
        {json: "comment", js: "comment", typ: u(undefined, "")},
    ], false),
    "SiteElement": o([
        {json: "site", js: "site", typ: r("SiteEnum")},
        {json: "id", js: "id", typ: u(undefined, "")},
        {json: "begin", js: "begin", typ: u(undefined, "")},
        {json: "broadcast", js: "broadcast", typ: u(undefined, "")},
        {json: "url", js: "url", typ: u(undefined, "")},
        {json: "comment", js: "comment", typ: u(undefined, "")},
    ], false),
    "TitleTranslate": o([
        {json: "zh-Hans", js: "zh-Hans", typ: u(undefined, a(""))},
        {json: "en", js: "en", typ: u(undefined, a(""))},
        {json: "zh-Hant", js: "zh-Hant", typ: u(undefined, a(""))},
        {json: "ja", js: "ja", typ: u(undefined, a(""))},
    ], false),
    "SiteMeta": o([
        {json: "title", js: "title", typ: ""},
        {json: "urlTemplate", js: "urlTemplate", typ: ""},
        {json: "regions", js: "regions", typ: u(undefined, a(""))},
        {json: "type", js: "type", typ: r("SiteMetaType")},
    ], false),
    "Lang": [
        "en",
        "ja",
        "zh-Hans",
    ],
    "SiteEnum": [
        "acfun",
        "ani_one_asia",
        "bangumi",
        "bilibili",
        "bilibili_tw",
        "bilibili_hk_mo",
        "bilibili_hk_mo_tw",
        "dmhy",
        "gamer",
        "iqiyi",
        "letv",
        "mgtv",
        "muse_hk",
        "netflix",
        "nicovideo",
        "pptv",
        "qq",
        "sohu",
        "viu",
        "youku",
    ],
    "ItemType": [
        "movie",
        "ova",
        "tv",
        "web",
    ],
    "SiteMetaType": [
        "info",
        "onair",
        "resource",
    ],
};
