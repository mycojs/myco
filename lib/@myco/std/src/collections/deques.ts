import {Collection} from "./base";
import {Stack} from "./stacks";
import {Queue} from "./queues";
import {AsyncStream, SyncStream} from "../streams";

export interface Deque<T> extends Stack<T>, Queue<T> {
}

export function dequeOf<T>(...items: T[]): Deque<T> {
    return new ArrayDeque(items);
}

export class ArrayDeque<T> implements Deque<T> {
    constructor(
        private items: T[] = []
    ) {}

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T | null {
        return this.items.pop() ?? null;
    }


    top(): T | null {
        return this.items[this.items.length - 1] ?? null;
    }

    enqueue(item: T): void {
        this.items.push(item);
    }

    dequeue(): T | null {
        return this.items.shift() ?? null;
    }

    front(): T | null {
        return this.items[0] ?? null;
    }

    clear(): void {
        this.items = [];
    }

    size(): number {
        let count = 0;
        for (const _ of this) {
            count++;
        }
        return count;
    }

    isEmpty(): boolean {
        return this.size() === 0;
    }

    toArray(): T[] {
        return [...this];
    }

    stream(): SyncStream<T> {
        return SyncStream.from(this);
    }

    asyncStream(): T extends Promise<infer U> ? AsyncStream<U> : never {
        return AsyncStream.of(...(this as Iterable<Promise<any>>)) as T extends Promise<infer U> ? AsyncStream<U> : never;
    }
}
