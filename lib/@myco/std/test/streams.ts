import {TestSuite, expect} from "vendor/@myco/test";
import {listOf} from "../src/collections";

export const streamsTest: TestSuite = {
    "Stream": {
        "should be able to map items": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .map(x => x * 2)
                    .toList()
            ).toIterateOver([2, 4, 6]);
        },
        "should be able to filter items": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .filter(x => x % 2 === 0)
                    .toList()
            ).toIterateOver([2]);
        },
        "should be able to peek items": () => {
            const items: number[] = [];
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .peek(x => items.push(x))
                    .toList()
            ).toIterateOver([1, 2, 3]);
            expect(items).toIterateOver([1, 2, 3]);
        },
        "should be able to chain operators": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .filter(x => x % 2 === 0)
                    .map(x => x * 2)
                    .toList()
            ).toIterateOver([4]);
        },
        "should be able to iterate over items": () => {
            const items: number[] = [];
            listOf(1, 2, 3)
                .stream()
                .forEach(x => items.push(x));
            expect(items).toIterateOver([1, 2, 3]);
        },
        "should be able to fold items": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .fold(0, (a, b) => a + b)
            ).toBe(6);
        },
        "should be able to reduce items": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .reduce<number>((a, b) => a + b)
            ).toBe(6);
            expect(
                listOf<number>()
                    .stream()
                    .reduce<number>((a, b) => a + b)
            ).toBe(null);
        },
        "should be able to find items": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .find(x => x % 2 === 0)
            ).toBe(2);
            expect(
                listOf(1, 3)
                    .stream()
                    .find(x => x % 2 === 0)
            ).toBe(null);
        },
        "should be able to find items or else": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .findOrElse(x => x % 2 === 0, 4)
            ).toBe(2);
            expect(
                listOf(1, 3)
                    .stream()
                    .findOrElse(x => x % 2 === 0, 4)
            ).toBe(4);
        },
        "should be able to check if any item matches": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .any(x => x % 2 === 0)
            ).toBe(true);
            expect(
                listOf(1, 3, 5)
                    .stream()
                    .any(x => x % 2 === 0)
            ).toBe(false);
        },
        "should be able to check if all items match": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .all(x => x % 2 === 0)
            ).toBe(false);
            expect(
                listOf(2, 4, 6)
                    .stream()
                    .all(x => x % 2 === 0)
            ).toBe(true);
        },
        "should be able to count items": () => {
            expect(
                listOf(1, 2, 3)
                    .stream()
                    .count()
            ).toBe(3);
        },
    },
    "AsyncStream": {
        "should be able to map items": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .map(x => x * 2)
                    .toList()
            ).toIterateOver([2, 4, 6]);
        },
        "should be able to filter items": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .filter(x => x % 2 === 0)
                    .toList()
            ).toIterateOver([2]);
        },
        "should be able to peek items": async () => {
            const items: number[] = [];
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .peek(x => items.push(x))
                    .toList()
            ).toIterateOver([1, 2, 3]);
            expect(items).toIterateOver([1, 2, 3]);
        },
        "should be able to chain operators": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .filter(x => x % 2 === 0)
                    .map(x => x * 2)
                    .toList()
            ).toIterateOver([4]);
        },
        "should be able to iterate over items": async () => {
            const items: number[] = [];
            await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                .asyncStream()
                .forEach(x => items.push(x));
            expect(items).toIterateOver([1, 2, 3]);
        },
        "should be able to fold items": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .fold(0, (a, b) => a + b)
            ).toBe(6);
        },
        "should be able to reduce items": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .reduce<number>((a, b) => a + b)
            ).toBe(6);
            expect(
                await listOf<Promise<number>>()
                    .asyncStream()
                    .reduce<number>((a, b) => a + b)
            ).toBe(null);
        },
        "should be able to find items": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .find(x => x % 2 === 0)
            ).toBe(2);
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(3))
                    .asyncStream()
                    .find(x => x % 2 === 0)
            ).toBe(null);
        },
        "should be able to find items or else": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .findOrElse(x => x % 2 === 0, 4)
            ).toBe(2);
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(3))
                    .asyncStream()
                    .findOrElse(x => x % 2 === 0, 4)
            ).toBe(4);
        },
        "should be able to check if any item matches": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .any(x => x % 2 === 0)
            ).toBe(true);
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(3), Promise.resolve(5))
                    .asyncStream()
                    .any(x => x % 2 === 0)
            ).toBe(false);
        },
        "should be able to check if all items match": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .all(x => x % 2 === 0)
            ).toBe(false);
            expect(
                await listOf(Promise.resolve(2), Promise.resolve(4), Promise.resolve(6))
                    .asyncStream()
                    .all(x => x % 2 === 0)
            ).toBe(true);
        },
        "should be able to count items": async () => {
            expect(
                await listOf(Promise.resolve(1), Promise.resolve(2), Promise.resolve(3))
                    .asyncStream()
                    .count()
            ).toBe(3);
        },
    },
}