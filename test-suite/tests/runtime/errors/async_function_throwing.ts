export default async function(myco: Myco) {
    console.log("Testing async function throwing");
    
    // Helper function that throws after a delay
    async function throwingFunction(): Promise<string> {
        await new Promise(resolve => myco.setTimeout(() => resolve(undefined), 50));
        throw new Error("Async function threw error");
    }
    
    // This should throw an error and not be caught
    await throwingFunction();
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 