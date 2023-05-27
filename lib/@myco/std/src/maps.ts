import {Stream, AsyncStream} from "./streams";
import {equals, hashCode} from "./core";

export interface Map<K, V> extends Iterable<[K, V]> {
    get(key: K): V | null;

    set(key: K, value: V): void;

    remove(key: K): V | null;

    containsKey(key: K): boolean;

    containsValue(value: V): boolean;

    size(): number;

    isEmpty(): boolean;

    clear(): void;

    keys(): Iterable<K>;

    values(): Iterable<V>;

    entries(): Iterable<[K, V]>;

    toArray(): [K, V][];

    toObject(): K extends (string | number | symbol) ? Record<K, V> : never;

    stream(): Stream<[K, V]>;

    asyncStream(): V extends Promise<infer U> ? AsyncStream<[K, U]> : never;
}

export function mapOf<K, V>(...entries: [K, V][]): Map<K, V> {
    return HashMap.of(...entries);
}

export class HashMap<K, V> implements Map<K, V> {
    private readonly _map = new Map<number, {key: K, value: V}[]>();

    static of<K, V>(...entries: [K, V][]): HashMap<K, V> {
        const map = new HashMap<K, V>();
        for (const [key, value] of entries) {
            map.set(key, value);
        }
        return map;
    }

    get(key: K): V | null {
        const items = this._map.get(hashCode(key));
        if (!items) {
            return null;
        }
        for (const item of items) {
            if (equals(item.key, key)) {
                return item.value;
            }
        }
        return null;
    }

    set(key: K, value: V): void {
        const items = this._map.get(hashCode(key));
        if (!items) {
            this._map.set(hashCode(key), [{key, value}]);
            return;
        }
        for (const item of items) {
            if (equals(item.key, key)) {
                item.value = value;
                return;
            }
        }
        items.push({key, value});
    }

    remove(key: K): V | null {
        const items = this._map.get(hashCode(key));
        if (!items) {
            return null;
        }
        for (let i = 0; i < items.length; i++) {
            const item = items[i];
            if (equals(item.key, key)) {
                items.splice(i, 1);
                return item.value;
            }
        }
        return null;
    }

    containsKey(key: K): boolean {
        const items = this._map.get(hashCode(key));
        if (!items) {
            return false;
        }
        for (const item of items) {
            if (equals(item.key, key)) {
                return true;
            }
        }
        return false;
    }

    containsValue(value: V): boolean {
        for (const items of this._map.values()) {
            for (const item of items) {
                if (equals(item.value, value)) {
                    return true;
                }
            }
        }
        return false;
    }

    size(): number {
        let count = 0;
        for (const items of this._map.values()) {
            count += items.length;
        }
        return count;
    }

    isEmpty(): boolean {
        return this.size() === 0;
    }

    clear(): void {
        this._map.clear();
    }

    keys(): Iterable<K> {
        const keys: K[] = [];
        for (const items of this._map.values()) {
            for (const item of items) {
                keys.push(item.key);
            }
        }
        return keys;
    }

    values(): Iterable<V> {
        const values: V[] = [];
        for (const items of this._map.values()) {
            for (const item of items) {
                values.push(item.value);
            }
        }
        return values;
    }

    entries(): Iterable<[K, V]> {
        const entries: [K, V][] = [];
        for (const items of this._map.values()) {
            for (const item of items) {
                entries.push([item.key, item.value]);
            }
        }
        return entries;
    }

    toArray(): [K, V][] {
        return [...this.entries()];
    }

    toObject(): K extends string | number | symbol ? Record<K, V> : never {
        const obj: any = {};
        for (const items of this._map.values()) {
            for (const item of items) {
                obj[item.key as any] = item.value;
            }
        }
        return obj;
    }

    stream(): Stream<[K, V]> {
        return Stream.from(this.entries());
    }

    asyncStream(): V extends Promise<infer U> ? AsyncStream<[K, U]> : never {
        return AsyncStream.from(this.entries() as any) as V extends Promise<infer U> ? AsyncStream<[K, U]> : never;
    }

    [Symbol.iterator](): Iterator<[K, V]> {
        return this.entries()[Symbol.iterator]();
    }
}
