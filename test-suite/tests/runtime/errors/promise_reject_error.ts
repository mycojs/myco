export default async function(myco: Myco) {
    console.log("Testing Promise.reject");
    
    // This should throw an error and not be caught
    await Promise.reject(new Error("Immediate rejection"));
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 