const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const ts = require('typescript');

// Run the real source with explicit substitutes at the I/O boundary.
// No account credentials, filesystem writes, browsers, or network calls.
function harness(overrides = {}, env = {}) {
    const cache = new Map();
    const messages = [];
    const noop = () => {};
    const stubs = {
        './log_util': {
            autoLog: message => messages.push(message), autoLogException: error => messages.push(String(error)),
            createProgressBar: noop, incrementProgressBar: noop, stopProgressBar: noop,
            LogLevel: {Error: 1, Warn: 2, Info: 3},
        },
        './data_util': {
            getGlobalAnimeItemByMal: async () => ({episodes: 12}),
            getGlobalAnimeItemByAnilist: () => ({episodes: 12}),
            getAnilistId: async () => '100',
        },
        './config_util': {config: {}, ignore_entries: {mal: [], anilist: [], bangumi: []}},
        './util': {isServerMode: false},
        './anilist_client': {}, './mal_client': {}, './bangumi_client': {}, './scheduler': {},
        'string-similarity': {}, 'node-notifier': {},
        limiter: {RateLimiter: class {async removeTokens() {}}},
        axios: {}, fs: {}, http: {}, crypto,
        open: () => {throw new Error('Unexpected browser launch');},
        ...overrides,
    };
    function load(file) {
        if (cache.has(file)) return cache.get(file);
        const source = fs.readFileSync(path.join(__dirname, '..', file), 'utf8');
        const js = ts.transpileModule(source, {compilerOptions: {
            module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2016, esModuleInterop: true,
        }}).outputText;
        const exports = {};
        cache.set(file, exports);
        vm.runInNewContext(js, {
            exports, URL, URLSearchParams, Buffer, Date, Map, setTimeout, clearTimeout,
            process: {env},
            require: id => {
                if (id === '../utils/log_util') return stubs['./log_util'];
                if (Object.hasOwn(stubs, id)) return stubs[id];
                if (id.startsWith('../types/')) return load(`src/types/${id.slice(9)}.ts`);
                throw new Error(`Unmocked dependency: ${id}`);
            },
        }, {filename: file});
        return exports;
    }
    return {load, messages};
}

const entry = (changes = {}) => ({
    title: 'Fixture', bgm_id: '1', mal_id: '10', anilist_id: '100',
    status: 'Watching', watched_episodes: 3, score: 0, comments: '', update_time: new Date(0), ...changes,
});

for (const scores of [[8, 6], [0, 9]]) {
    for (const reverse of [false, true]) {
        test(`unrated source keeps target writes isolated: ${scores}, reverse=${reverse}`, async () => {
            const requests = [];
            const h = harness({axios: {put: async (url, body) => {requests.push(new URLSearchParams(body));}}});
            const sync = h.load('src/utils/sync_util.ts');
            const source = Object.freeze(entry());
            const targets = [
                Object.freeze(entry({score: scores[0], watched_episodes: 2})),
                Object.freeze(entry({score: scores[1], anilist_id: undefined, watched_episodes: 2})),
            ];
            const plans = [];
            for (const i of reverse ? [1, 0] : [0, 1]) {
                plans[i] = await sync.generateChangelog([source], [targets[i]], true);
            }
            assert.equal(plans[0].length, 1);
            assert.equal(plans[1].length, 1);
            assert.notEqual(plans[0][0].after, plans[1][0].after);
            assert.notEqual(plans[0][0].after, source);
            assert.equal(source.score, 0);
            assert.equal(plans[0][0].after.anilist_id, '100');
            assert.equal(plans[1][0].after.mal_id, '10');
            assert.doesNotMatch(sync.renderDiff(targets[0], plans[0][0].after, true), /Score:/);
            const ani = h.load('src/utils/anilist_client.ts').anilistClient;
            let aniPayload;
            ani.query = async (_, payload) => {aniPayload = JSON.parse(JSON.stringify(payload)); return {SaveMediaListEntry: {id: 1}};};
            assert.equal(await ani.smartUpdateCollection([plans[0][0].after], true), 1);
            await h.load('src/utils/mal_client.ts').malClient.saveEntry(plans[1][0].after, true);
            assert.equal(Object.hasOwn(aniPayload, 'scoreRaw'), false);
            assert.equal(requests[0].has('score'), false);
        });
    }
}

test('score-only differences are ignored when unrated; rated changes produce one update', async () => {
    const sync = harness().load('src/utils/sync_util.ts');
    assert.equal((await sync.generateChangelog([entry()], [entry({score: 8})], true)).length, 0);
    const changes = await sync.generateChangelog([entry({score: 7, comments: 'new'})], [entry({score: 8, comments: 'old'})], true);
    assert.equal(changes.length, 1);
    assert.equal(changes[0].after.score, 7);
    assert.match(sync.renderDiff(changes[0].before, changes[0].after, true), /Score: 8 -> 7/);
});

test('new entries are copied independently for each destination', async () => {
    const sync = harness().load('src/utils/sync_util.ts');
    const source = Object.freeze(entry());
    const a = await sync.generateChangelog([source], [], false);
    const b = await sync.generateChangelog([source], [], false);
    a[0].after.score = 9;
    assert.equal(source.score, 0);
    assert.equal(b[0].after.score, 0);
});

test('MAL sends empty comments when enabled, omits when disabled, and preserves rated scores', async () => {
    const bodies = [];
    const client = harness({axios: {put: async (_, body) => bodies.push(new URLSearchParams(body))}})
        .load('src/utils/mal_client.ts').malClient;
    for (const comments of ['', undefined, null]) {
        assert.equal(await client.saveEntry(entry({score: 7, comments}), true), true);
        assert.equal(bodies.at(-1).get('comments'), '');
        assert.equal(bodies.at(-1).get('score'), '7');
    }
    await client.saveEntry(entry({comments: 'private note'}), false);
    assert.equal(bodies.at(-1).has('comments'), false);
});

test('AniList counts only successful single writes and clears missing comments explicitly', async () => {
    const client = harness().load('src/utils/anilist_client.ts').anilistClient;
    const responses = [null, {}, {SaveMediaListEntry: {id: 3}}];
    const payloads = [];
    client.query = async (_, payload) => {payloads.push(payload); return responses.shift();};
    assert.equal(await client.smartUpdateCollection([
        entry(), entry({anilist_id: '101'}), entry({anilist_id: '102', score: 7, comments: undefined}),
    ], true), 1);
    assert.equal(payloads[2].notes, '');
    assert.equal(payloads[2].scoreRaw, 70);
});

test('AniList batch writes omit unrated score and report exhausted queries as failures', async () => {
    const client = harness().load('src/utils/anilist_client.ts').anilistClient;
    const entries = Array.from({length: 71}, (_, i) => entry({anilist_id: String(i + 100)}));
    for (const c of entries) client.media_to_entry_id.set(c.anilist_id, c.anilist_id);
    client.query = async (_, payload) => {
        assert.equal(Object.hasOwn(JSON.parse(JSON.stringify(payload)), 'scoreRaw'), false);
        return null;
    };
    assert.equal(await client.smartUpdateCollection(entries), 0);
    client.query = async () => ({UpdateMediaListEntries: entries.map(c => ({id: c.anilist_id}))});
    assert.equal(await client.smartUpdateCollection(entries), 71);
});

test('MAL configuration is lazy but missing or placeholder credentials fail before I/O', async () => {
    for (const env of [{}, {MAL_CLIENT_ID: 'TODO'}, {MAL_CLIENT_ID: 'id', MAL_CLIENT_SECRET: 'TODO'}]) {
        const client = harness({}, env).load('src/utils/mal_client.ts').malClient;
        await assert.rejects(client.autoUpdateToken(), /MAL_CLIENT/);
    }
    const client = harness({}, {MAL_CLIENT_ID: 'id'}).load('src/utils/mal_client.ts').malClient;
    assert.doesNotThrow(() => client.validateConfiguration());
});

function oauthHarness(secret, denied = false, failToken = false) {
    let handler;
    let closed = false;
    let auth;
    const bodies = [];
    const h = harness({
        http: {createServer: callback => {
            handler = callback;
            return {
                once() {},
                listen(port, host, ready) {assert.equal(port, 3500); assert.equal(host, 'localhost'); ready();},
                close() {closed = true;},
            };
        }},
        open: async url => {
            auth = new URL(url);
            assert.equal(auth.searchParams.has('client_secret'), false);
            assert.equal(auth.searchParams.get('client_id'), 'test-id');
            const response = () => ({writeHead() {}, end() {}});
            handler({url: '/favicon.ico'}, response());
            assert.equal(closed, false);
            handler({url: '/?state=wrong&code=ignored'}, response());
            assert.equal(closed, false);
            const query = new URLSearchParams({state: auth.searchParams.get('state')});
            query.set(denied ? 'error' : 'code', denied ? 'access_denied' : 'test-code');
            handler({url: `/?${query}`}, response());
        },
        axios: {post: async (_, body) => {
            bodies.push(new URLSearchParams(body));
            if (failToken) throw {response: {status: 400}, toString: () => 'SECRET-LEAK'};
            return {data: {access_token: 'access', refresh_token: 'refresh', expires_in: 3600, token_type: 'Bearer'}};
        }},
    }, {MAL_CLIENT_ID: 'test-id', ...(secret ? {MAL_CLIENT_SECRET: secret} : {})});
    return {...h, client: h.load('src/utils/mal_client.ts').malClient, bodies, auth: () => auth, closed: () => closed};
}

for (const secret of [undefined, 'test-secret']) {
    test(`MAL authorization and refresh use configured credentials, secret=${!!secret}`, async () => {
        const h = oauthHarness(secret);
        await h.client.getToken();
        assert.equal(h.closed(), true);
        assert.equal(h.bodies[0].get('code'), 'test-code');
        assert.equal(h.bodies[0].get('code_verifier'), h.auth().searchParams.get('code_challenge'));
        assert.equal(h.bodies[0].get('redirect_uri'), 'http://localhost:3500');
        assert.equal(await h.client.refreshToken(), true);
        for (const body of h.bodies) {
            assert.equal(body.get('client_id'), 'test-id');
            assert.equal(body.get('client_secret'), secret || null);
        }
        assert.equal(h.bodies[1].get('grant_type'), 'refresh_token');
        assert.equal(h.bodies[1].get('refresh_token'), 'refresh');
        assert.equal(h.messages.join('\n').includes('test-secret'), false);
    });
}

test('MAL denial closes callback without exchanging a token', async () => {
    const h = oauthHarness(undefined, true);
    await assert.rejects(h.client.getToken(), /denied/);
    assert.equal(h.closed(), true);
    assert.equal(h.bodies.length, 0);
});

test('MAL token failures do not expose request bodies in errors or logs', async () => {
    const h = oauthHarness('test-secret', false, true);
    await assert.rejects(h.client.getToken(), /HTTP 400/);
    assert.equal(await h.client.refreshToken(), false);
    assert.doesNotMatch(h.messages.join('\n'), /SECRET-LEAK|test-secret/);
});

test('legacy config defaults survive reload and preserve explicit false', () => {
    for (const raw of [{}, {sync_to_anilist: false, sync_to_mal: true}, {sync_to_anilist: false, sync_to_mal: false}]) {
        const cfg = harness({'../../config/config.json': raw,
            '../../config/manual_relations.json': [], '../../config/ignore_entries.json': {},
        }).load('src/utils/config_util.ts');
        for (let i = 0; i < 2; i++) {
            assert.equal(cfg.config.sync_to_anilist, raw.sync_to_anilist ?? true);
            assert.equal(cfg.config.sync_to_mal, raw.sync_to_mal ?? false);
            cfg.reloadConfig();
        }
    }
});
