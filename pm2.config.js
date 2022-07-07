module.exports = {
    apps: [{
        name: "bangumi-sync",
        script: "./build/main.js",
        args: "--server",
        // watch_delay: 1000,
        // watch: ["build", "./ignore_entries.json", "./manual_relations.json", "./config.json"],
        // ignore_watch: ["node_modules", "cache", "log"]
    }]
}
