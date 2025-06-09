export default async function(myco: Myco) {
    console.log("Testing Promise.race with rejection");
    
    // Helper function to create rejecting promise
    function rejectAfter(ms: number, error: string): Promise<never> {
        return new Promise((_, reject) => {
            myco.setTimeout(() => {
                reject(new Error(error));
            }, ms);
        });
    }
    
    // Helper function to create resolving promise  
    function resolveAfter(ms: number, value: any): Promise<any> {
        return new Promise((resolve) => {
            myco.setTimeout(() => {
                resolve(value);
            }, ms);
        });
    }
    
    // This should throw an error and not be caught (fast rejection wins)
    await Promise.race([
        rejectAfter(50, "Fast rejection"),
        resolveAfter(100, "Slow success")
    ]);
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 