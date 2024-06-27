import cliProgress, {MultiBar, SingleBar} from "cli-progress";
import chalk from "chalk";
import fs from "fs";
import {isServerMode} from "./util";
import {config} from "./config_util";

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

/**
 * Create a new progress bar.
 * @param total The total number of items to process.
 */
export function createProgressBar(total: number) {
    if (isServerMode) return;

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

/**
 * Update the progress bar.
 * @param amount The amount to increment the progress bar by.
 */
export function incrementProgressBar(amount: number = 1) {
    if (isServerMode) return;

    if (progressBar && multiBar) {
        progressBar.increment(amount);
        multiBar.update();
    } else {
        autoLog("Progress bar not initialized", "incrementProgressBar", LogLevel.Error);
    }
}

/**
 * Stop the progress bar.
 */
export function stopProgressBar() {
    if (isServerMode) return;

    if (multiBar && progressBar) {
        progressBar.stop();
        multiBar.stop();
        progressBar = null;
        multiBar = null;
    }
}

/**
 * Log a message to the console and log file.
 * @param message The message to log.
 * @param tag The tag to log the message under.
 * @param level The level of the message.
 * @param format Whether to format the message or not.
 */
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
        fs.appendFileSync(logFile, `${getDateStr()} ${tag_str}${level_str}: ${message}\n`);

        // If the log file contains 1000+ lines, delete the first line.
        if (fs.readFileSync(logFile).toString().split("\n").length > 1000) {
            fs.writeFileSync(logFile, fs.readFileSync(logFile).toString().split("\n").slice(1).join("\n"));
        }
    }
}

/**
 * Log an error to the console and log file.
 * @param e The error to log.
 * @param tag The tag to log the error under.
 */
export function autoLogException(e: Error, tag: string = "") {
    autoLog(e.message, tag, LogLevel.Error);
    // Separate stack trace by line breaks, and print each line separately.
    if (e.stack) {
        e.stack.split("\n").forEach((line) => {
            autoLog(line, tag, LogLevel.Error);
        });
    }
}

/**
 * Get the current date and time as a formatted string.
 * @param sep The separator between h,m,s to use.
 * @returns The current date and time.
 */
function getDateStr(sep: string = ":"): string {
    const autoPad = (num: number, len: number = 2) => num.toString().padStart(len, "0");
    const date = new Date();
    return `${date.getFullYear()}-${autoPad(date.getMonth() + 1)}-${autoPad(date.getDate())} ${autoPad(date.getHours())}${sep}${autoPad(date.getMinutes())}${sep}${autoPad(date.getSeconds())}`;
}

// Setup log file and level
if (!fs.existsSync(config.log_path)) fs.mkdirSync(config.log_path);
logFile = `${config.log_path}/${getDateStr("-")}.log`;

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
