function hashNumber(number: number): number {
    return number;
}

function hashString(string: string): number {
    let hash = 0;
    for (let i = 0; i < string.length; i++) {
        hash = ((hash << 5) - hash) + string.charCodeAt(i);
        hash |= 0;
    }
    return hash;
}

function hashBoolean(boolean: boolean): number {
    return boolean ? 1 : 0;
}

export enum Comparison {
    LessThan = -1,
    EqualTo = 0,
    GreaterThan = 1
}

export type Comparator<T> = (value1: T, value2: T) => Comparison;

export function compareNumbers(n1: number, n2: number): Comparison {
    return n1 < n2
        ? Comparison.LessThan
        : n1 > n2
            ? Comparison.GreaterThan
            : Comparison.EqualTo;
}

export function compareStrings(s1: string, s2: string): Comparison {
    return s1 < s2
        ? Comparison.LessThan
        : s1 > s2
            ? Comparison.GreaterThan
            : Comparison.EqualTo;
}

export function compareBooleans(b1: boolean, b2: boolean): Comparison {
    return b1 === b2
        ? Comparison.EqualTo
        : b1
            ? Comparison.GreaterThan
            : Comparison.LessThan;
}

export const symbols = {
    equals: Symbol("equals"),
    hashCode: Symbol("hashCode"),
}

export function hashCode(value: unknown): number {
    if (value == null) {
        return 0;
    }
    if (typeof (value as any)[symbols.hashCode] == "function") {
        return (value as any)[symbols.hashCode].call(value);
    } else if (typeof value == "number") {
        return hashNumber(value);
    } else if (typeof value == "string") {
        return hashString(value);
    } else if (typeof value == "boolean") {
        return hashBoolean(value);
    } else {
        return hashString(JSON.stringify(value));
    }
}

export function equals(value1: unknown, value2: unknown): boolean {
    if (value1 === value2) {
        return true;
    }
    if (value1 == null || value2 == null) {
        return false;
    }
    if (typeof (value1 as any)[symbols.equals] == "function") {
        return (value1 as any)[symbols.equals](value2);
    }
    if (typeof (value1 as any)[symbols.equals] == "function") {
        return (value1 as any)[symbols.equals](value1);
    }
    if (Array.isArray(value1) && Array.isArray(value2)) {
        if (value1.length != value2.length) {
            return false;
        }
        for (let i = 0; i < value1.length; i++) {
            if (!equals(value1[i], value2[i])) {
                return false;
            }
        }
        return true;
    }
    if (typeof value1 === 'object' && typeof value2 === 'object') {
        const keys1 = Object.keys(value1);
        const keys2 = Object.keys(value2);
        if (keys1.length != keys2.length) {
            return false;
        }
        for (const key of keys1) {
            if (!equals((value1 as any)[key], (value2 as any)[key])) {
                return false;
            }
        }
        return true;
    }
    return false;
}
