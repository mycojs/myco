import {TestSuite, expect} from "vendor/@myco/test";
import {HashSet, setOf} from "../../src/collections";

export const setsTests: TestSuite = {
    "HashSet": {
        "should be able to add items": () => {
            const set = new HashSet<number>();
            set.add(1);
            set.add(2);
            set.add(3);
            expect(set).toIterateOver([1, 2, 3]);
        },
        "should be able to remove items": () => {
            const set = setOf(1, 2, 3);
            set.remove(2);
            expect(set).toIterateOver([1, 3]);
        },
        "should be able to clear items": () => {
            const set = setOf(1, 2, 3);
            set.clear();
            expect(set).toIterateOver([]);
        },
        "should be able to union two sets": () => {
            const set1 = setOf(1, 2, 3);
            const set2 = setOf(2, 3, 4);
            const union = set1.union(set2);
            expect(union).toIterateOver([1, 2, 3, 4]);
        },
        "should be able to intersect two sets": () => {
            const set1 = setOf(1, 2, 3);
            const set2 = setOf(2, 3, 4);
            const intersection = set1.intersection(set2);
            expect(intersection).toIterateOver([2, 3]);
        },
        "should be able to difference two sets": () => {
            const set1 = setOf(1, 2, 3);
            const set2 = setOf(2, 3, 4);
            const difference = set1.difference(set2);
            expect(difference).toIterateOver([1]);
        },
        "should be able to check if a set is a subset of another set": () => {
            const set1 = setOf(1, 2, 3);
            const set2 = setOf(2, 3);
            expect(set2.isSubsetOf(set1)).toBe(true);
            expect(set1.isSubsetOf(set2)).toBe(false);
        },
        "should be able to check if a set is a superset of another set": () => {
            const set1 = setOf(1, 2, 3);
            const set2 = setOf(2, 3);
            expect(set1.isSupersetOf(set2)).toBe(true);
            expect(set2.isSupersetOf(set1)).toBe(false);
        },
    }
}