export default async function (myco: Myco) {
    console.log("Starting timeout behavior test");
    
    const logs: string[] = [];
    
    // Helper to add timestamped logs
    function addLog(message: string) {
        logs.push(message);
        console.log(message);
    }
    
    // Test basic setTimeout
    addLog("Setting timeout");
    await new Promise<void>((resolve) => {
        myco.setTimeout(() => {
            addLog("Timeout executed");
            resolve();
        }, 100);
        addLog("Timeout set, continuing");
    });
    
    // Test multiple setTimeout with different delays
    addLog("Testing multiple timeouts");
    const promises: Promise<void>[] = [];
    
    promises.push(new Promise<void>((resolve) => {
        myco.setTimeout(() => {
            addLog("Timeout 3 (300ms)");
            resolve();
        }, 300);
    }));
    
    promises.push(new Promise<void>((resolve) => {
        myco.setTimeout(() => {
            addLog("Timeout 1 (100ms)");
            resolve();
        }, 100);
    }));
    
    promises.push(new Promise<void>((resolve) => {
        myco.setTimeout(() => {
            addLog("Timeout 2 (200ms)");
            resolve();
        }, 200);
    }));
    
    await Promise.all(promises);
    addLog("All timeouts completed");
    
    // Test nested timeouts
    addLog("Testing nested timeouts");
    await new Promise<void>((resolve) => {
        myco.setTimeout(() => {
            addLog("Outer timeout start");
            myco.setTimeout(() => {
                addLog("Inner timeout executed");
                resolve();
            }, 50);
            addLog("Outer timeout end");
        }, 100);
    });
    
    // Test setTimeout with 0 delay
    addLog("Testing zero delay timeout");
    await new Promise<void>((resolve) => {
        addLog("Before zero timeout");
        myco.setTimeout(() => {
            addLog("Zero delay timeout executed");
            resolve();
        }, 0);
        addLog("After zero timeout set");
    });
    
    // Test timeout order with immediate operations
    addLog("Testing timeout vs immediate");
    const orderTest: string[] = [];
    
    // This should execute first (synchronous)
    orderTest.push("sync1");
    
    // This should execute after current synchronous code
    myco.setTimeout(() => {
        orderTest.push("timeout");
        console.log("Order test result:", orderTest.join(","));
    }, 0);
    
    // This should execute second (synchronous)
    orderTest.push("sync2");
    
    // Wait for the timeout to complete
    await new Promise<void>((resolve) => {
        myco.setTimeout(() => {
            resolve();
        }, 10);
    });
    
    // Test async function timing
    addLog("Testing async function timing");
    
    async function timedFunction(): Promise<string> {
        addLog("Async function start");
        
        await new Promise<void>((resolve) => {
            myco.setTimeout(() => {
                addLog("Async function timeout");
                resolve();
            }, 150);
        });
        
        addLog("Async function end");
        return "async result";
    }
    
    const result = await timedFunction();
    addLog(`Got result: ${result}`);
    
    console.log("Timeout behavior test completed");
} 