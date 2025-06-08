export default function (_myco: Myco) {
    // Regular function declaration
    function add(a: number, b: number): number {
        return a + b;
    }
    
    // Arrow function
    const multiply = (a: number, b: number): number => a * b;
    
    // Function with closure
    function createCounter(): () => number {
        let count = 0;
        return () => {
            count++;
            return count;
        };
    }
    
    // Higher-order function
    function applyTwice(fn: (x: number) => number, x: number): number {
        return fn(fn(x));
    }
    
    // Test the functions
    console.log("add(5, 3):", add(5, 3));
    console.log("multiply(4, 7):", multiply(4, 7));
    
    const counter = createCounter();
    console.log("counter():", counter());
    console.log("counter():", counter());
    console.log("counter():", counter());
    
    const double = (x: number) => x * 2;
    console.log("applyTwice(double, 3):", applyTwice(double, 3));
} 