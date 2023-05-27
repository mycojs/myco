import {TestSuite, expect} from "vendor/@myco/test";
import {ArrayList} from "../../src/collections";

export const listsTests: TestSuite = {
    "ArrayList": {
        "should be able to add items": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            expect(list).toIterateOver([1, 2, 3]);
        },
        "should be able to remove items": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            list.remove(2);
            expect(list).toIterateOver([1, 3]);
        },
        "should be able to clear items": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            list.clear();
            expect(list).toIterateOver([]);
        },
        "should be able to get items by index": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            expect(list.get(1)).toBe(2);
        },
        "should be able to set items by index": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            list.set(1, 4);
            expect(list.get(1)).toBe(4);
        },
        "should be able to insert items by index": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            list.insert(1, 4);
            expect(list).toIterateOver([1, 4, 2, 3]);
        },
        "should be able to remove items by index": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            list.removeAt(1);
            expect(list).toIterateOver([1, 3]);
        },
        "should be able to get the index of an item": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            expect(list.indexOf(2)).toBe(1);
        },
        "should be able to get the last index of an item": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            list.add(2);
            expect(list.lastIndexOf(2)).toBe(3);
        },
        "should be able to get a sub list": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            list.add(4);
            list.add(5);
            expect(list.subList(1, 4)).toIterateOver([2, 3, 4]);
        },
        "should be able to iterate over items": () => {
            const list = new ArrayList<number>();
            list.add(1);
            list.add(2);
            list.add(3);
            let i = 0;
            for (const item of list) {
                expect(item).toBe(list.get(i));
                i++;
            }
        },
    }
}