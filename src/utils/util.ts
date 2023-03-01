export function sleep(ms: number) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

export const isServerMode = (process.argv[2] === "--server");
