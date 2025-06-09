// Fixture file for testing imports
export const greeting = "Hello from imported module!";

export function add(a: number, b: number): number {
    return a + b;
}

export const numbers = [1, 2, 3, 4, 5];

export const config = {
    name: "test-config",
    version: "1.0.0",
    enabled: true
};

export default function defaultFunction(): string {
    return "This is the default export";
}

export class TestClass {
    constructor(public name: string) {}
    
    greet(): string {
        return `Hello, ${this.name}!`;
    }
} 