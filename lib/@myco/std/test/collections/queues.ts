import {TestSuite, expect} from "vendor/@myco/test";
import {ArrayQueue, PriorityQueue, queueOf} from "../../src/collections";

export const queuesTests: TestSuite = {
    "ArrayQueue": {
        "should be able to enqueue items": () => {
            const queue = new ArrayQueue<number>();
            queue.enqueue(1);
            queue.enqueue(2);
            queue.enqueue(3);
            expect(queue).toIterateOver([1, 2, 3]);
        },
        "should be able to dequeue items": () => {
            const queue = queueOf(1, 2, 3);
            expect(queue.dequeue()).toBe(1);
            expect(queue).toIterateOver([2, 3]);
        },
        "should be able to peek items": () => {
            const queue = queueOf(1, 2, 3)
            expect(queue.peek()).toBe(1);
            expect(queue).toIterateOver([1, 2, 3]);
        },
        "should be able to clear items": () => {
            const queue = queueOf(1, 2, 3);
            queue.clear();
            expect(queue).toIterateOver([]);
        },
        "should be able to iterate over items": () => {
            const queue = queueOf(1, 2, 3);
            let items = [];
            for (const item of queue) {
                items.push(item);
            }
            expect(items).toEqual([1, 2, 3]);
        },
    },
    "PriorityQueue": {
        "should be able to enqueue items": () => {
            const queue = new PriorityQueue<number>((a, b) => Math.sign(a - b));
            queue.enqueue(3);
            queue.enqueue(1);
            queue.enqueue(2);
            expect(queue).toIterateOver([1, 2, 3]);
        },
        "should be able to dequeue items": () => {
            const queue = new PriorityQueue<number>((a, b) => Math.sign(a - b));
            queue.enqueue(3);
            queue.enqueue(1);
            queue.enqueue(2);
            expect(queue.dequeue()).toBe(1);
            expect(queue).toIterateOver([2, 3]);
        },
        "should be able to peek items": () => {
            const queue = new PriorityQueue<number>((a, b) => Math.sign(a - b));
            queue.enqueue(3);
            queue.enqueue(1);
            queue.enqueue(2);
            expect(queue.peek()).toBe(1);
            expect(queue).toIterateOver([1, 2, 3]);
        },
        "should be able to clear items": () => {
            const queue = new PriorityQueue<number>((a, b) => Math.sign(a - b));
            queue.enqueue(3);
            queue.enqueue(1);
            queue.enqueue(2);
            queue.clear();
            expect(queue).toIterateOver([]);
        },
        "should be able to iterate over items": () => {
            const queue = new PriorityQueue<number>((a, b) => Math.sign(a - b));
            queue.enqueue(3);
            queue.enqueue(1);
            queue.enqueue(2);
            let items = [];
            for (const item of queue) {
                items.push(item);
            }
            expect(items).toEqual([1, 2, 3]);
        },
    }
}