import {Stream, AsyncStream} from "../streams";

export interface Collection<T> extends Iterable<T> {
    size(): number;

    isEmpty(): boolean;

    clear(): void;

    toArray(): T[];

    stream(): Stream<T>;

    asyncStream(): T extends Promise<infer U> ? AsyncStream<U> : never;
}

export abstract class BaseCollection<T> implements Collection<T> {
    abstract [Symbol.iterator](): Iterator<T>;

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

    abstract clear(): void;

    toArray(): T[] {
        return [...this];
    }

    stream(): Stream<T> {
        return Stream.from(this);
    }

    asyncStream(): T extends Promise<infer U> ? AsyncStream<U> : never {
        return AsyncStream.of(...(this as BaseCollection<Promise<any>>)) as T extends Promise<infer U> ? AsyncStream<U> : never;
    }
}