import {BaseCollection, Collection} from "./base";

export interface Stack<T> extends Collection<T> {
    push(item: T): void;

    pop(): T | null;

    peek(): T | null;
}

export function stackOf<T>(...items: T[]): Stack<T> {
    return new ArrayStack(items);
}

export class ArrayStack<T> extends BaseCollection<T> implements Stack<T> {
    constructor(
        private items: T[] = []
    ) {
        super();
    }

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T | null {
        return this.items.pop() ?? null;
    }


    peek(): T | null {
        return this.items[this.items.length - 1] ?? null;
    }

    size() {
        return this.items.length;
    }

    [Symbol.iterator](): Iterator<T> {
        const that = this;
        return (function* () {
            while (that.size() > 0) {
                yield that.pop()!;
            }
        })()
    }

    add(item: T): void {
        this.push(item);
    }

    clear(): void {
        this.items = [];
    }
}
