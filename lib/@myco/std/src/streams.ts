import {Comparison} from "./core";
import {Map, HashMap, Set, HashSet, List, ArrayList} from "./collections";

export function streamOf<T>(...items: T[]): Stream<T> {
    return SyncStream.of(...items);
}

export class SyncStream<T> {
    constructor(
        private iterator: Iterator<T>,
    ) {
    }

    static of<T>(...items: T[]): SyncStream<T> {
        return new SyncStream(items[Symbol.iterator]());
    }

    static empty<T>(): SyncStream<T> {
        return new SyncStream((function* (): Iterator<T> {
            return;
        })());
    }

    static from<T>(iterable: Iterable<T>): SyncStream<T> {
        return new SyncStream(iterable[Symbol.iterator]());
    }

    // ----------------
    // Stream operators
    // ----------------
    peek(consumer: (item: T) => void): SyncStream<T> {
        let iterator = this.iterator;
        return new SyncStream((function* (): Iterator<T> {
            while (true) {
                const {value, done} = iterator.next();
                if (done) {
                    break;
                }
                consumer(value);
                yield value;
            }
        })());
    }

    filter(predicate: (item: T) => boolean): SyncStream<T> {
        let iterator = this.iterator;
        return new SyncStream((function* () {
            while (true) {
                const {value, done} = iterator.next();
                if (done) {
                    break;
                }
                if (predicate(value)) {
                    yield value;
                }
            }
        })());
    }

    map<U>(mapper: (item: T) => U): SyncStream<U> {
        let iterator = this.iterator;
        return new SyncStream((function* () {
            while (true) {
                const {value, done} = iterator.next();
                if (done) {
                    break;
                }
                yield mapper(value);
            }
        })());
    }

    flatMap<U>(mapper: (item: T) => Iterable<U>): SyncStream<U> {
        let iterator = this.iterator;
        return new SyncStream((function* () {
            while (true) {
                const {value, done} = iterator.next();
                if (done) {
                    break;
                }
                yield* mapper(value);
            }
        })());
    }

    // ------------------
    // Terminal operators
    // ------------------
    [Symbol.iterator](): Iterator<T> {
        return this.iterator;
    }

    forEach(consumer: (item: T) => void): void {
        while (true) {
            const {value, done} = this.iterator.next();
            if (done) {
                break;
            }
            consumer(value);
        }
    }

    fold<U>(initialValue: U, reducer: (accumulator: U, item: T) => U): U {
        let accumulator = initialValue;
        this.forEach(item => {
            accumulator = reducer(accumulator, item);
        });
        return accumulator;
    }

    reduce<U>(reducer: (accumulator: U, item: T) => U): U | null {
        let {value: accumulator, done} = this.iterator.next();
        if (done) {
            return null;
        }
        return this.fold(accumulator, reducer);
    }

    associateBy<K>(keySelector: (item: T) => K): Map<K, T> {
        return this.fold(new HashMap<K, T>(), (map, item) => {
            const key = keySelector(item);
            map.set(key, item);
            return map;
        });
    }

    groupBy<K>(keySelector: (item: T) => K): Map<K, T[]> {
        return this.fold(new HashMap<K, T[]>(), (map, item) => {
            const key = keySelector(item);
            const list = map.get(key) ?? [];
            list.push(item);
            map.set(key, list);
            return map;
        });
    }

    toList(): List<T> {
        return this.fold(new ArrayList<T>(), (list, item) => {
            list.add(item);
            return list;
        });
    }

    toSet(): Set<T> {
        return this.fold(new HashSet<T>(), (set, item) => {
            set.add(item);
            return set;
        });
    }

    count(): number {
        let count = 0;
        this.forEach(_ => {
            count++;
        });
        return count;
    }

    any(predicate: (item: T) => boolean): boolean {
        while (true) {
            const {value, done} = this.iterator.next();
            if (done) {
                break;
            }
            if (predicate(value)) {
                return true;
            }
        }
        return false;
    }

    all(predicate: (item: T) => boolean): boolean {
        while (true) {
            const {value, done} = this.iterator.next();
            if (done) {
                break;
            }
            if (!predicate(value)) {
                return false;
            }
        }
        return true;
    }

    find(predicate: (item: T) => boolean): T | null {
        while (true) {
            const {value, done} = this.iterator.next();
            if (done) {
                break;
            }
            if (predicate(value)) {
                return value;
            }
        }
        return null;
    }

    findOrElse(predicate: (item: T) => boolean, defaultValue: T): T {
        return this.find(predicate) ?? defaultValue;
    }

    min(comparator: (a: T, b: T) => Comparison): T | null {
        const list = this.toList();
        if (list.size() === 0) {
            return null;
        }
        let min = list.get(0);
        for (let item of list) {
            if (comparator(item, min) === Comparison.LessThan) {
                min = item;
            }
        }
        return min;
    }

    max(comparator: (a: T, b: T) => Comparison): T | null {
        const list = this.toList();
        if (list.size() === 0) {
            return null;
        }
        let max = list.get(0);
        for (let item of list) {
            if (comparator(item, max) === Comparison.GreaterThan) {
                max = item;
            }
        }
        return max;
    }
}

export function asyncStreamOf<T>(...items: Promise<T>[]): AsyncStream<T> {
    return AsyncStream.of(...items);
}

export class AsyncStream<T> {
    constructor(
        private iterator: AsyncIterator<T>,
    ) {}

    static of<T>(...items: Promise<T>[]): AsyncStream<T> {
        return new AsyncStream((async function* () {
            for (let item of items) {
                yield await item;
            }
        })());
    }

    static empty<T>(): AsyncStream<T> {
        return new AsyncStream((async function* (): AsyncIterator<T> {
        })());
    }

    static from<T>(iterable: AsyncIterable<T>): AsyncStream<T> {
        return new AsyncStream(iterable[Symbol.asyncIterator]());
    }

    // ----------------
    // Stream operators
    // ----------------
    peek(consumer: (item: T) => void): AsyncStream<T> {
        let iterator = this.iterator;
        return new AsyncStream((async function* () {
            while (true) {
                const {value, done} = await iterator.next();
                if (done) {
                    break;
                }
                consumer(value);
                yield value;
            }
        })());
    }

    filter(predicate: (item: T) => boolean): AsyncStream<T> {
        let iterator = this.iterator;
        return new AsyncStream((async function* () {
            while (true) {
                const {value, done} = await iterator.next();
                if (done) {
                    break;
                }
                if (predicate(value)) {
                    yield value;
                }
            }
        })());
    }

    map<U>(mapper: (item: T) => U): AsyncStream<U> {
        let iterator = this.iterator;
        return new AsyncStream((async function* () {
            while (true) {
                const {value, done} = await iterator.next();
                if (done) {
                    break;
                }
                yield mapper(value);
            }
        })());
    }

    flatMap<U>(mapper: (item: T) => AsyncIterable<U>): AsyncStream<U> {
        let iterator = this.iterator;
        return new AsyncStream((async function* () {
            while (true) {
                const {value, done} = await iterator.next();
                if (done) {
                    break;
                }
                yield* mapper(value);
            }
        })());
    }

    // ------------------
    // Terminal operators
    // ------------------
    [Symbol.asyncIterator](): AsyncIterator<T> {
        return this.iterator;
    }

    async forEach(consumer: (item: T) => void): Promise<void> {
        while (true) {
            const {value, done} = await this.iterator.next();
            if (done) {
                break;
            }
            consumer(value);
        }
    }

    async fold<U>(initialValue: U, reducer: (accumulator: U, item: T) => U): Promise<U> {
        let accumulator = initialValue;
        await this.forEach(item => {
            accumulator = reducer(accumulator, item);
        });
        return accumulator;
    }

    async reduce<U>(reducer: (accumulator: U, item: T) => U): Promise<U | null> {
        let {value: accumulator, done} = await this.iterator.next();
        if (done) {
            return null;
        }
        return await this.fold(accumulator, reducer);
    }

    async associateBy<K>(keySelector: (item: T) => K): Promise<Map<K, T>> {
        return await this.fold(new HashMap(), (map, item) => {
            map.set(keySelector(item), item);
            return map;
        });
    }

    async groupBy<K>(keySelector: (item: T) => K): Promise<Map<K, T[]>> {
        return await this.fold(new HashMap(), (map, item) => {
            const key = keySelector(item);
            const group = map.get(key) ?? [];
            group.push(item);
            map.set(key, group);
            return map;
        });
    }

    async toList(): Promise<List<T>> {
        return await this.fold(new ArrayList(), (list, item) => {
            list.add(item);
            return list;
        });
    }

    async toSet(): Promise<Set<T>> {
        return await this.fold(new HashSet(), (set, item) => {
            set.add(item);
            return set;
        });
    }

    async count(): Promise<number> {
        let count = 0;
        await this.forEach(_ => {
            count++;
        });
        return count;
    }

    async any(predicate: (item: T) => boolean): Promise<boolean> {
        while (true) {
            const {value, done} = await this.iterator.next();
            if (done) {
                break;
            }
            if (predicate(value)) {
                return true;
            }
        }
        return false;
    }

    async all(predicate: (item: T) => boolean): Promise<boolean> {
        while (true) {
            const {value, done} = await this.iterator.next();
            if (done) {
                break;
            }
            if (!predicate(value)) {
                return false;
            }
        }
        return true;
    }

    async find(predicate: (item: T) => boolean): Promise<T | null> {
        while (true) {
            const {value, done} = await this.iterator.next();
            if (done) {
                break;
            }
            if (predicate(value)) {
                return value;
            }
        }
        return null;
    }

    async findOrElse(predicate: (item: T) => boolean, defaultValue: T): Promise<T> {
        return (await this.find(predicate)) ?? defaultValue;
    }

    async min(comparator: (a: T, b: T) => Comparison): Promise<T | null> {
        const list = await this.toList();
        if (list.size() === 0) {
            return null;
        }
        let min = list.get(0);
        for (let item of list) {
            if (comparator(item, min) === Comparison.LessThan) {
                min = item;
            }
        }
        return min;
    }

    async max(comparator: (a: T, b: T) => Comparison): Promise<T | null> {
        const list = await this.toList();
        if (list.size() === 0) {
            return null;
        }
        let max = list.get(0);
        for (let item of list) {
            if (comparator(item, max) === Comparison.GreaterThan) {
                max = item;
            }
        }
        return max;
    }
}

export type Stream<T> = AsyncStream<T> | SyncStream<T>;
