import {TestSuite, expect} from "vendor/@myco/test";
import {HashMap, mapOf} from "../src/maps";

export const mapsTest: TestSuite = {
    "HashMap": {
        "should be able to add items": () => {
            const map = new HashMap<string, number>();
            map.set("a", 1);
            map.set("b", 2);
            map.set("c", 3);
            expect(map).toIterateOver([["a", 1], ["b", 2], ["c", 3]]);
        },
        "should be able to remove items": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            map.remove("b");
            expect(map).toIterateOver([["a", 1], ["c", 3]]);
        },
        "should be able to clear items": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            map.clear();
            expect(map).toIterateOver([]);
        },
        "should be able to get items by key": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            expect(map.get("b")).toBe(2);
        },
        "should be able to set items by key": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            map.set("b", 4);
            expect(map.get("b")).toBe(4);
        },
        "should be able to check if a key exists": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            expect(map.containsKey("b")).toBe(true);
            expect(map.containsKey("d")).toBe(false);
        },
        "should be able to check if a value exists": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            expect(map.containsValue(2)).toBe(true);
            expect(map.containsValue(4)).toBe(false);
        },
        "should be able to get the keys": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            expect(map.keys()).toIterateOver(["a", "b", "c"]);
        },
        "should be able to get the values": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            expect(map.values()).toIterateOver([1, 2, 3]);
        },
        "should be able to get the entries": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            expect(map.entries()).toIterateOver([["a", 1], ["b", 2], ["c", 3]]);
        },
        "should be able to get the size": () => {
            const map = mapOf(
                ["a", 1],
                ["b", 2],
                ["c", 3]
            );
            expect(map.size()).toBe(3);
        },
    },
}