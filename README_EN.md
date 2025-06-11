# Bangumi-Sync

> [中文](README.md) | English

**A script that automatically syncs all anime collections from bgm.tv to Anilist.**

## Installation

This project used [Node.js](https://nodejs.org) environment. To clone the project and install dependencies, run the
following commands in terminal:

```bash
git clone https://github.com/ykozxy/bangumi-sync
cd bangumi-sync
npm i
```

## Start Using

<img src="./asset/image-20220522130730661.png" alt="image-20220522130730661" style="zoom: 67%;" />

In the first run, the script will automatically download the anime databases from the provided data sources, which may take a while due to the large database file. Then, the authentication pages of Bangumi and Anilist will be opened in browser automatically -- just follow the prompts.

### Single Execution Mode

In this mode, the script will only perform a single sync then exit. The `manual_confirm` entry in the configuration file can be used to set whether to manually confirm before synchronizing updates.

Execute the following commands to run the script in single execution mode:

```bash
npm start
```

Additional command line flags:

* `--backward` - Sync changes from Anilist back to Bangumi.
* `--both` - Detect the newer entry between the two platforms and update the other accordingly.

### Docker Mode

Run the following commands to build and run the docker image:

```shell
docker build -t bangumi-sync .
docker run -d \
	--name="bangumi-sync" \
	-v $(pwd):/app \
	--restart=unless-stopped bangumi-sync
```

When hosting with Docker, the `manual_confirm` field is automatically ignored. The script is executed at the interval set by the `server_mode_interval` entry (in seconds) in the configuration file, and all output is written to a log file named after the script's starting time.

Before the first run, the following command needs to be executed to get the token:

```shell
npm run token
```

You can also use the above command to refresh the token if it expires or is missing while the Docker image is running.

## Configuration

The configuration file is `config/config.json`, where the configurable fields are:

| Field                       | Description                                                                                   | Parameters                         |
|-----------------------------|-----------------------------------------------------------------------------------------------|------------------------------------|
| `sync_comments`             | Whether to synchronize comments.                                                              | `true` / `false`                   |
| `manual_confirm`            | Whether to confirm manually before uploading updates. Automatically `false` in `server` mode. | `true` / `false`                   |
| `server_mode_interval`      | Controls the time interval (in seconds) between two executions in `server` mode.              | `number`                           |
| `enable_notifications`      | *Whether to enable desktop notifications in `server` mode (only on exceptions) (deprecated).* | `true` / `false`                   |
| `cache_path`                | Cache path.                                                                                   | Relative Path                      |
| `log_path`                  | Log file path.                                                                                | Relative Path                      |
| `log_file_level`            | Minimum output level to log files.                                                            | `debug` / `info`/ `warn` / `error` |
| `log_console_level`         | Minimum output level to console.                                                              | `debug` / `info`/ `warn` / `error` |
| `global_anime_database_url` | Url to global anime database.                                                                 | url                                |
| `china_anime_database_url`  | Url to CN anime database.                                                                     | url                                |

## Manual entry matching

Since this project cannot do 100% auto-matching (yet), entries can be manually matched or ignored by
editing `config/manual_relations.json` or `config/ignore_entries.json`.

Each item in `manual_relation.json` should be of the form `[bangumi_id, anilist_id]`, representing a forced matching of
the two entries.

Each item in `ignore_entries.json` should be the anime ID on each website. Entries with the same ID will be ignored when
they are being processed.

## Entry Matching Algorithm

Since entries in all the CN anime databases I found could not perfectly match those from the MAL/anidb-based databases,
naive matching by entry name is not ideal. Therefore, the matching algorithm of this project combines a fuzzy name match
with exact metadata comparison. Since a large number of queries are required for each matching, this approach sacrifices
efficiency for a higher precision.

In testing, only 22 out of 250+ anime entries in my Bangumi collection failed to match Anilist entries. Excluding
non-Japanese and Bangumi entries with missing information, only 9 failed due to database information mismatch. The total
success rate is about 95%.

The implementation of the algorithm can be found in [data_util.ts](src/utils/data_util.ts)
and [sync_util.ts](src/utils/sync_util.ts).

## Known Limitations

- Because the Anilist API has rate limited of 90 requests/min, too many entries in a single sync will trigger the limit
  and take much longer.
- The entry matching algorithm currently in use requires matching 30,000+ global database entries one by one, so it may
  take longer for the first time to run the scipt. Later it will be faster as the cache will be built when running.

## TODO

- [x] Server mode (via Docker).
- [x] Notification push in Server mode (No longer works in Docker mode).
- [x] Two-way synchronization from Anilist to bangumi.
- [ ] Switch to postgres/mongoDB as backend storage.

## Data Sources

- Global anime data：[manami-project/anime-offline-database](https://github.com/manami-project/anime-offline-database)
- CN anime data：[bangumi-data](https://github.com/bangumi-data/bangumi-data)
