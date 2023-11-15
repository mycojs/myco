import {BaseCollection, Collection} from "./base";
import {Comparator, Comparison} from "../core";

export interface Queue<T> extends Collection<T> {
    enqueue(item: T): void;

    dequeue(): T | null;

    peek(): T | null;
}

export function queueOf<T>(...items: T[]): Queue<T> {
    return new ArrayQueue(items);
}

export class ArrayQueue<T> extends BaseCollection<T> implements Queue<T> {
    constructor(
        private items: T[] = []
    ) {
        super();
    }

    enqueue(item: T): void {
        this.items.push(item);
    }

    dequeue(): T | null {
        return this.items.shift() ?? null;
    }

    peek(): T | null {
        return this.items[0] ?? null;
    }

    size() {
        return this.items.length;
    }

    [Symbol.iterator](): Iterator<T> {
        const that = this;
        return (function* () {
            while (that.size() > 0) {
                yield that.items.shift()!;
            }
        })()
    }

    clear(): void {
        this.items = [];
    }
}

export class PriorityQueue<T> extends BaseCollection<T> implements Queue<T> {
    private heap: T[] = [];

    constructor(
        private comparator: Comparator<T>
    ) {
        super();
    }

    enqueue(item: T): void {
        this.heap.push(item);
        this.heapifyUp();
    }

    dequeue(): T | null {
        if (this.heap.length === 0) {
            return null;
        }
        const smallest = this.heap[0];
        if (this.heap.length === 1) {
            this.heap = [];
        } else {
            this.heap[0] = this.heap.pop()!;
            this.heapifyDown();
        }
        return smallest;
    }

    peek(): T | null {
        return this.heap[0] ?? null;
    }

    size() {
        return this.heap.length;
    }

    [Symbol.iterator](): Iterator<T> {
        const that = this;
        return (function* () {
            while (that.size() > 0) {
                yield that.dequeue()!;
            }
        })()
    }

    clear(): void {
        this.heap = [];
    }

    private heapifyUp() {
        let index = this.heap.length - 1;
        let parentIndex = this.parentIndex(index);
        while (parentIndex >= 0 && this.comparator(this.heap[parentIndex], this.heap[index]) == Comparison.GreaterThan) {
            this.swap(this.parentIndex(index), index);
            index = parentIndex;
            parentIndex = this.parentIndex(index);
        }
    }

    private heapifyDown() {
        let index = 0;
        let leftChildIndex = this.leftChildIndex(index);
        let rightChildIndex = this.rightChildIndex(index);
        while (this.heap[leftChildIndex] && this.heap[rightChildIndex]) {
            if (this.comparator(this.heap[leftChildIndex], this.heap[rightChildIndex]) == Comparison.LessThan) {
                this.swap(index, leftChildIndex);
                index = leftChildIndex;
            } else if (this.comparator(this.heap[leftChildIndex], this.heap[rightChildIndex]) == Comparison.GreaterThan) {
                this.swap(index, rightChildIndex);
                index = rightChildIndex;
            } else {
                break;
            }
            leftChildIndex = this.leftChildIndex(index);
            rightChildIndex = this.rightChildIndex(index);
        }
    }

    private parentIndex(index: number) {
        return Math.floor((index - 1) / 2);
    }

    private leftChildIndex(index: number) {
        return index * 2 + 1;
    }

    private rightChildIndex(index: number) {
        return index * 2 + 2;
    }

    private swap(index1: number, index2: number) {
        const temp = this.heap[index1];
        this.heap[index1] = this.heap[index2];
        this.heap[index2] = temp;
    }
}
