import {BaseCollection, Collection} from "./base";
import {equals, hashCode} from "../core";

export interface Set<T> extends Collection<T> {
    union(other: Set<T>): Set<T>;

    intersection(other: Set<T>): Set<T>;

    difference(other: Set<T>): Set<T>;

    isSubsetOf(other: Set<T>): boolean;

    isSupersetOf(other: Set<T>): boolean;
}

export function setOf<T>(...items: T[]): Set<T> {
    return HashSet.of(...items);
}

export class HashSet<T> extends BaseCollection<T> implements Set<T> {
    private readonly _map = new Map<number, T[]>();

    static of<T>(...items: T[]): Set<T> {
        const set: Set<T> = new HashSet();
        for (const item of items) {
            set.add(item);
        }
        return set;
    }

    add(item: T): void {
        const hash = hashCode(item);
        const items = this._map.get(hash);
        if (!items) {
            this._map.set(hash, [item]);
        } else {
            for (const x of items) {
                if (equals(x, item)) {
                    return;
                }
            }
        }
    }

    remove(item: T): T | null {
        const hash = hashCode(item);
        const items = this._map.get(hash);
        if (!items) {
            return null;
        }
        for (let i = 0; i < items.length; i++) {
            if (equals(items[i], item)) {
                return items.splice(i, 1)[0];
            }
        }
        return null;
    }

    contains(item: T): boolean {
        const hash = hashCode(item);
        const items = this._map.get(hash);
        if (!items) {
            return false;
        }
        return items.some(x => equals(x, item));
    }

    clear(): void {
        this._map.clear();
    }

    union(other: Set<T>): Set<T> {
        const newSet = new HashSet<T>();
        for (const item of this) {
            newSet.add(item);
        }
        for (const item of other) {
            newSet.add(item);
        }
        return newSet;
    }

    intersection(other: Set<T>): Set<T> {
        const newSet = new HashSet<T>();
        for (const item of this) {
            if (other.contains(item)) {
                newSet.add(item);
            }
        }
        return newSet;
    }

    difference(other: Set<T>): Set<T> {
        const newSet = new HashSet<T>();
        for (const item of this) {
            if (!other.contains(item)) {
                newSet.add(item);
            }
        }
        return newSet;
    }

    isSubsetOf(other: Set<T>): boolean {
        for (const item of this) {
            if (!other.contains(item)) {
                return false;
            }
        }
        return true;
    }

    isSupersetOf(other: Set<T>): boolean {
        return other.isSubsetOf(this);
    }

    [Symbol.iterator](): Iterator<T> {
        const values = this._map.values();
        return function*() {
            for (const items of values) {
                for (const item of items) {
                    yield item;
                }
            }
        }()
    }
}
