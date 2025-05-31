export default async function (myco: Myco) {
    console.log("Starting promise chaining test");
    
    // Helper function to create delayed promises
    function delay(ms: number, value: any): Promise<any> {
        return new Promise((resolve) => {
            myco.setTimeout(() => {
                resolve(value);
            }, ms);
        });
    }
    
    // Test Promise chaining with .then()
    console.log("Testing promise chaining");
    await delay(50, "initial")
        .then((value) => {
            console.log("First then:", value);
            return delay(50, "second");
        })
        .then((value) => {
            console.log("Second then:", value);
            return delay(50, "final");
        })
        .then((value) => {
            console.log("Third then:", value);
        });
    
    // Test Promise.all - wait for all promises to complete
    console.log("Testing Promise.all");
    const promises = [
        delay(100, "A"),
        delay(50, "B"),
        delay(75, "C")
    ];
    
    const allResults = await Promise.all(promises);
    console.log("Promise.all results:", allResults.join(","));
    
    // Test Promise.race - first promise to complete wins
    console.log("Testing Promise.race");
    const racePromises = [
        delay(100, "slow"),
        delay(50, "fast"),
        delay(75, "medium")
    ];
    
    const raceResult = await Promise.race(racePromises);
    console.log("Promise.race winner:", raceResult);
    
    // Test nested async operations
    console.log("Testing nested async");
    
    async function processData(data: string): Promise<string> {
        const step1 = await delay(50, `processed-${data}`);
        const step2 = await delay(50, `validated-${step1}`);
        return step2;
    }
    
    const processed = await processData("input");
    console.log("Processed data:", processed);
    
    // Test concurrent operations
    console.log("Testing concurrent operations");
    const start = Date.now();
    
    const [result1, result2, result3] = await Promise.all([
        delay(100, "concurrent1"),
        delay(100, "concurrent2"),
        delay(100, "concurrent3")
    ]);
    
    const elapsed = Date.now() - start;
    console.log("Concurrent results:", result1, result2, result3);
    console.log("Elapsed time < 200ms:", elapsed < 200);
    
    console.log("Promise chaining test completed");
} 