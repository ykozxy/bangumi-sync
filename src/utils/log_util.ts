import cliProgress, {MultiBar, SingleBar} from "cli-progress";
import chalk from "chalk";
import fs from "fs";
import {Config} from "../types/config";

const config: Config = require("../../config.json");

let multiBar: MultiBar | null;
let progressBar: SingleBar | null;
let logFile: string;
let logFileLevel: LogLevel;
let logConsoleLevel: LogLevel;

export enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

export function createProgressBar(total: number) {
    if (progressBar || multiBar) {
        autoLog("Trying to overwrite existing progress bar", "createProgressBar", LogLevel.Warn);
        multiBar?.stop();
        progressBar?.stop();
    }

    multiBar = new cliProgress.MultiBar({
        stopOnComplete: true,
        etaBuffer: 200,
        hideCursor: true,
        forceRedraw: true,
        etaAsynchronousUpdate: true,
    }, cliProgress.Presets.shades_classic);
    progressBar = multiBar.create(total, 0, {});
}

export function incrementProgressBar(amount: number = 1) {
    if (progressBar && multiBar) {
        progressBar.increment(amount);
        multiBar.update();
    } else {
        autoLog("Progress bar not initialized", "incrementProgressBar", LogLevel.Error);
    }
}

export function stopProgressBar() {
    if (multiBar && progressBar) {
        progressBar.stop();
        multiBar.stop();
        progressBar = null;
        multiBar = null;
    }
}

export function autoLog(message: string, tag: string = "", level: LogLevel = LogLevel.Info, format: boolean = true) {
    const tag_str = tag ? `[${tag}] ` : "";
    let console_message: string;
    let level_str: string;
    switch (level) {
        case LogLevel.Debug:
            // console_message = `${tag_str}${format ? chalk.gray(message) : message}`;
            console_message = format ? chalk.gray(`${tag_str}${message}`) : `${tag_str}${message}`;
            level_str = "DEBUG";
            break;
        case LogLevel.Warn:
            // console_message = `${tag_str}${format ? chalk.yellow(message) : message}`;
            console_message = format ? chalk.yellow(`${tag_str}${message}`) : `${tag_str}${message}`;
            level_str = "WARN";
            break;
        case LogLevel.Error:
            // console_message = `${tag_str}${format ? chalk.red(message) : message}`;
            console_message = format ? chalk.red(`${tag_str}${message}`) : `${tag_str}${message}`;
            level_str = "ERROR";
            break;
        case LogLevel.Info:
        default:
            // console_message = `${tag_str}${message}`;
            console_message = `${tag_str}${message}`;
            level_str = "INFO";
            break;
    }

    // If progress bar is enabled, use multiBar's log function.
    if (level >= logConsoleLevel) {
        if (multiBar && progressBar) {
            multiBar.log(console_message + "\n");
        } else {
            console.log(console_message);
        }
    }

    // Log to file
    if (logFile && level >= logFileLevel) {
        // Get local date and time string
        const date = new Date();
        const date_str = `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()} ${date.getHours()}:${date.getMinutes()}:${date.getSeconds().toString().padStart(2, "0")}`;

        fs.appendFileSync(logFile, `${date_str} ${tag_str}${level_str}: ${message}\n`);
    }
}

export function autoLogException(e: Error, invoker: string = "") {
    autoLog(`${e.message}\n${e.stack}`, invoker, LogLevel.Error, false);
}

// Setup log file and level
if (!fs.existsSync(config.log_path)) fs.mkdirSync(config.log_path);
const date = new Date();
const date_str = `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()} ${date.getHours()}-${date.getMinutes()}-${date.getSeconds().toString().padStart(2, "0")}`;
logFile = `${config.log_path}/${date_str}.log`;

switch (config.log_file_level) {
    case "debug":
        logFileLevel = LogLevel.Debug;
        break;
    case "warn":
        logFileLevel = LogLevel.Warn;
        break;
    case "error":
        logFileLevel = LogLevel.Error;
        break;
    case "info":
    default:
        logFileLevel = LogLevel.Info;
        break;
}

switch (config.log_console_level) {
    case "debug":
        logConsoleLevel = LogLevel.Debug;
        break;
    case "warn":
        logConsoleLevel = LogLevel.Warn;
        break;
    case "error":
        logConsoleLevel = LogLevel.Error;
        break;
    case "info":
    default:
        logConsoleLevel = LogLevel.Info;
        break;
}
