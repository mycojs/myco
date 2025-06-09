export default async function(myco: Myco) {
    console.log("Testing Promise.all with rejection");
    
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
    
    // This should throw an error and not be caught
    await Promise.all([
        resolveAfter(50, "success1"),
        rejectAfter(75, "Promise.all error"),
        resolveAfter(100, "success2")
    ]);
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 