/**
 * A scheduler for async tasks.
 */
export default class Scheduler {
    private readonly limit: number;
    private queue: Array<() => Promise<void>> = [];
    private running: number = 0;

    /**
     * @param limit The maximum number of tasks that can be running at the same time.
     */
    constructor(limit: number) {
        this.limit = limit;
    }

    /**
     * Adds a task to the queue.
     * @param task The task to add.
     */
    public push(task: () => Promise<void>): void {
        this.queue.push(task);
        if (this.running < this.limit) this.next();
    }

    /**
     * Waits for all tasks to finish.
     */
    public async wait(): Promise<void> {
        while (this.running) await new Promise(resolve => setTimeout(resolve, 100));
    }

    private next(): void {
        if (this.queue.length) this.run(this.queue.shift()!);
    }

    private run(task: () => Promise<void>): void {
        this.running++;
        task().then(() => {
            this.running--;
            this.next();
        });
    }
}