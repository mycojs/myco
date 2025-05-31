export default async function (myco: Myco) {
    console.log("Starting async error handling test");
    
    // Helper function to create promises that reject
    function rejectAfter(ms: number, error: string): Promise<never> {
        return new Promise((_, reject) => {
            myco.setTimeout(() => {
                reject(new Error(error));
            }, ms);
        });
    }
    
    // Helper function to create successful promises
    function resolveAfter(ms: number, value: any): Promise<any> {
        return new Promise((resolve) => {
            myco.setTimeout(() => {
                resolve(value);
            }, ms);
        });
    }
    
    // Test basic async/await error handling
    console.log("Testing basic async error handling");
    try {
        await rejectAfter(50, "Async error");
        console.log("This should not print");
    } catch (error: any) {
        console.log("Caught async error:", error.message);
    }
    
    // Test Promise.reject
    console.log("Testing Promise.reject");
    try {
        await Promise.reject(new Error("Immediate rejection"));
        console.log("This should not print");
    } catch (error: any) {
        console.log("Caught Promise.reject:", error.message);
    }
    
    // Test async function that throws
    async function throwingFunction(): Promise<string> {
        await resolveAfter(50, "delay");
        throw new Error("Function threw error");
    }
    
    console.log("Testing async function that throws");
    try {
        await throwingFunction();
        console.log("This should not print");
    } catch (error: any) {
        console.log("Caught function error:", error.message);
    }
    
    // Test Promise.all with one rejection
    console.log("Testing Promise.all with rejection");
    try {
        await Promise.all([
            resolveAfter(50, "success1"),
            rejectAfter(75, "Promise.all error"),
            resolveAfter(100, "success2")
        ]);
        console.log("This should not print");
    } catch (error: any) {
        console.log("Caught Promise.all error:", error.message);
    }
    
    // Test Promise.race with first being rejection
    console.log("Testing Promise.race with rejection");
    try {
        await Promise.race([
            rejectAfter(50, "Fast rejection"),
            resolveAfter(100, "Slow success")
        ]);
        console.log("This should not print");
    } catch (error: any) {
        console.log("Caught Promise.race error:", error.message);
    }
    
    // Test nested async error handling
    console.log("Testing nested async errors");
    async function outerFunction(): Promise<string> {
        try {
            return await throwingFunction();
        } catch (error: any) {
            console.log("Inner catch:", error.message);
            throw new Error("Outer error");
        }
    }
    
    try {
        await outerFunction();
        console.log("This should not print");
    } catch (error: any) {
        console.log("Outer catch:", error.message);
    }
    
    // Test error in then() chain
    console.log("Testing error in promise chain");
    try {
        await resolveAfter(50, "initial")
            .then((value) => {
                console.log("Chain step 1:", value);
                throw new Error("Chain error");
            })
            .then((value) => {
                console.log("This should not print");
                return value;
            });
        console.log("This should not print");
    } catch (error: any) {
        console.log("Caught chain error:", error.message);
    }
    
    // Test finally block
    console.log("Testing finally block");
    try {
        await rejectAfter(50, "Finally test error");
    } catch (error: any) {
        console.log("Caught finally test error:", error.message);
    } finally {
        console.log("Finally block executed");
    }
    
    console.log("Async error handling test completed");
} 