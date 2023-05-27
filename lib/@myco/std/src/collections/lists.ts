import {BaseCollection, Collection} from "./base";
import {equals} from "../core";

export interface List<T> extends Collection<T> {
    get(index: number): T;

    set(index: number, item: T): void;

    insert(index: number, item: T): void;

    removeAt(index: number): void;

    indexOf(item: T): number;

    lastIndexOf(item: T): number;

    subList(fromIndex: number, toIndex: number): List<T>;
}

export function listOf<T>(...items: T[]): List<T> {
    return new ArrayList(items);
}

export class ArrayList<T> extends BaseCollection<T> implements List<T> {
    constructor(
        private items: T[] = []
    ) {
        super();
    }

    get(index: number): T {
        return this.items[index];
    }

    set(index: number, item: T): void {
        this.items[index] = item;
    }

    insert(index: number, item: T): void {
        this.items.splice(index, 0, item);
    }

    removeAt(index: number): void {
        this.items.splice(index, 1);
    }

    indexOf(item: T): number {
        return this.items.indexOf(item);
    }

    lastIndexOf(item: T): number {
        return this.items.lastIndexOf(item);
    }

    subList(fromIndex: number, toIndex: number): List<T> {
        return new ArrayList(this.items.slice(fromIndex, toIndex));
    }

    [Symbol.iterator](): Iterator<T> {
        return this.items[Symbol.iterator]();
    }

    add(item: T): void {
        this.items.push(item);
    }

    remove(item: T): T | null {
        const index = this.items.findIndex(i => equals(item, i));
        if (index === -1) {
            return null;
        }
        return this.items.splice(index, 1)[0];
    }

    clear(): void {
        this.items = [];
    }

    contains(item: T): boolean {
        return false;
    }

    toString(): string {
        return this.items.toString();
    }
}
