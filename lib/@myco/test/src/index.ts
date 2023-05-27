import {equals} from "vendor/@myco/std/core";

export interface TestSuite {
    [name: string]: TestSuite | (() => void);
}

export interface TestResults {
    [name: string]: TestResults | TestResult;
}

export class TestSuccess {
    constructor() {
    }
}

export class TestFailure extends Error {
    constructor(public explanation: string) {
        super(explanation);
    }

    get failureExplanation() {
        return `${this.explanation}${this.stack ? `\n${this.stack}` : ''}`
    }
}

export class TestException {
    constructor(public error: any) {
    }

    get failureExplanation() {
        return `${this.error}${this.error.stack ? `\n${this.error.stack}` : ''}`
    }
}

type TestResult =
    | TestSuccess
    | TestFailure
    | TestException
    ;

export function run(suite: TestSuite): void {
    const results = runTestSuite('root', suite);
    console.log(resultsToString(results));
}

function prependIndent(str: string, indentation: string) {
    return str.split('\n').map(line => `${indentation}${line}`).join('\n');
}

export function isSuccess(results: TestResults): boolean {
    for (const value of Object.values(results)) {
        if (value instanceof TestFailure || value instanceof TestException) {
            return false;
        } else if (value instanceof TestSuccess) {
            // Keep going
        } else {
            if (!isSuccess(value)) {
                return false;
            }
        }
    }
    return true;
}

export function resultsToString(results: TestResults, depth: number = 0): string {
    let lines: string[] = [];
    for (const [key, value] of Object.entries(results)) {
        let indentation = ' '.repeat(depth * 4);
        if (value instanceof TestSuccess) {
            lines.push(`${indentation}✅ ${key}`);
        } else if (value instanceof TestFailure || value instanceof TestException) {
            lines.push(`${indentation}❌ ${key}`);
            lines.push(prependIndent(value.failureExplanation, indentation + '    '));
        } else {
            const success = isSuccess(value);
            lines.push(`${indentation}${success ? '✅' : '❌'} ${key}`);
            lines.push(resultsToString(value, depth + 1));
        }
    }
    return lines.join('\n');
}

function runTestSuite(name: string, suite: TestSuite): TestResults {
    const results: TestResults = {};
    for (const [key, value] of Object.entries(suite)) {
        if (typeof value === "function") {
            try {
                value();
                results[key] = new TestSuccess();
            } catch (e) {
                if (e instanceof AssertionError) {
                    results[key] = new TestFailure(e.message);
                } else {
                    results[key] = new TestException(e);
                }
            }
        } else {
            results[key] = runTestSuite(`${name} | ${key}`, value);
        }
    }
    return results;
}

export function expect<T>(value: T): Expect<T> {
    return new Expect(value);
}

export class AssertionError extends Error {
    constructor(message: string) {
        super(message);
    }
}

export class Expect<T> {
    constructor(private value: T) {}

    toBe(value: T): void {
        if (this.value !== value) {
            throw new AssertionError(`Expected ${this.value} to be ${value}`);
        }
    }

    toBeTruthy(): void {
        if (!this.value) {
            throw new AssertionError(`Expected ${this.value} to be truthy`);
        }
    }

    toBeFalsy(): void {
        if (this.value) {
            throw new AssertionError(`Expected ${this.value} to be falsy`);
        }
    }

    toBeDefined(): void {
        if (this.value === undefined) {
            throw new AssertionError(`Expected ${this.value} to be defined`);
        }
    }

    toBeUndefined(): void {
        if (this.value !== undefined) {
            throw new AssertionError(`Expected ${this.value} to be undefined`);
        }
    }

    toBeNull(): void {
        if (this.value !== null) {
            throw new AssertionError(`Expected ${this.value} to be null`);
        }
    }

    toBeNotNull(): void {
        if (this.value === null) {
            throw new AssertionError(`Expected ${this.value} to be not null`);
        }
    }

    toBeInstanceOf(type: any): void {
        if (!(this.value instanceof type)) {
            throw new AssertionError(`Expected ${this.value} to be instance of ${type}`);
        }
    }

    toBeGreaterThan(value: number): void {
        if (!(this.value > value)) {
            throw new AssertionError(`Expected ${this.value} to be greater than ${value}`);
        }
    }

    toBeGreaterThanOrEqual(value: number): void {
        if (!(this.value >= value)) {
            throw new AssertionError(`Expected ${this.value} to be greater than or equal to ${value}`);
        }
    }

    toBeLessThan(value: number): void {
        if (!(this.value < value)) {
            throw new AssertionError(`Expected ${this.value} to be less than ${value}`);
        }
    }

    toBeLessThanOrEqual(value: number): void {
        if (!(this.value <= value)) {
            throw new AssertionError(`Expected ${this.value} to be less than or equal to ${value}`);
        }
    }

    toBeCloseTo(value: number | bigint, delta: number): void {
        if (typeof this.value !== 'number' && typeof this.value !== 'bigint') {
            throw new AssertionError(`Expected ${this.value} and ${value} to be numbers`);
        }
        if (typeof this.value === 'number' && typeof value === 'number') {
            if (!(Math.abs(this.value - value) <= delta)) {
                throw new AssertionError(`Expected ${this.value} to be close to ${value} within ${delta}`);
            }
        } else if (typeof this.value === 'bigint' && typeof value === 'bigint') {
            if (!(Math.abs(Number(this.value) - Number(value)) <= delta)) {
                throw new AssertionError(`Expected ${this.value} to be close to ${value} within ${delta}`);
            }
        } else {
            throw new AssertionError(`Expected ${this.value} and ${value} to be both numbers or both bigints`);
        }
    }

    toIterateOver(expected: Iterable<any>): T extends Iterable<infer U> ? void : never {
        if (typeof (this.value as any)[Symbol.iterator] !== 'function') {
            throw new AssertionError(`Expected ${this.value} to be iterable`);
        }
        const actual = Array.from(this.value as Iterable<T>);
        const expectedArray = Array.from(expected);
        if (!equals(actual, expectedArray)) {
            throw new AssertionError(`Expected ${actual} to equal ${expectedArray}`);
        }
        return undefined as T extends Iterable<infer U> ? void : never;
    }
}
