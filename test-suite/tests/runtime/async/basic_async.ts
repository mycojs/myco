export default async function (myco: Myco) {
    console.log("Starting async test");
    
    // Basic async function
    async function delayedGreeting(name: string): Promise<string> {
        // Simulate async work with setTimeout
        return new Promise((resolve) => {
            myco.setTimeout(() => {
                resolve(`Hello, ${name}!`);
            }, 100);
        });
    }
    
    // Test awaiting async function
    const greeting = await delayedGreeting("Alice");
    console.log("Delayed greeting:", greeting);
    
    // Test multiple awaits
    console.log("Before first await");
    const result1 = await delayedGreeting("Bob");
    console.log("First result:", result1);
    
    console.log("Before second await");
    const result2 = await delayedGreeting("Charlie");
    console.log("Second result:", result2);
    
    // Test async function that returns immediately
    async function immediateValue(): Promise<number> {
        return 42;
    }
    
    const immediate = await immediateValue();
    console.log("Immediate value:", immediate);
    
    // Test Promise.resolve
    const resolved = await Promise.resolve("Already resolved");
    console.log("Promise.resolve:", resolved);
    
    console.log("Async test completed");
} 