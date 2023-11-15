import {TestSuite, expect} from "vendor/@myco/test";
import {ArrayStack, stackOf} from "../../src/collections";

export const stacksTest: TestSuite = {
    "ArrayStack": {
        "should be able to push items": () => {
            const stack = new ArrayStack<number>();
            stack.push(1);
            stack.push(2);
            stack.push(3);
            expect(stack).toIterateOver([3, 2, 1]);
        },
        "should be able to pop items": () => {
            const stack = stackOf(1, 2, 3);
            expect(stack.pop()).toBe(3);
            expect(stack.pop()).toBe(2);
            expect(stack.pop()).toBe(1);
            expect(stack).toIterateOver([]);
        },
        "should be able to peek items": () => {
            const stack = stackOf(1, 2, 3);
            expect(stack.peek()).toBe(3);
            expect(stack.peek()).toBe(3);
            expect(stack).toIterateOver([3, 2, 1]);
        },
        "should be able to clear items": () => {
            const stack = stackOf(1, 2, 3);
            stack.clear();
            expect(stack).toIterateOver([]);
        },
        "should be able to iterate over the items": () => {
            const stack = stackOf(1, 2, 3);
            const items = [];
            for (const item of stack) {
                items.push(item);
            }
            expect(items).toEqual([3, 2, 1]);
        },
    }
}