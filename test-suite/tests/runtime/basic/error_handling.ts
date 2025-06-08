export default function (_myco: Myco) {
    // Basic try/catch
    try {
        console.log("Before error");
        throw new Error("Test error");
        console.log("This should not print");
    } catch (error: any) {
        console.log("Caught error:", error.message);
    }
    
    // Try/catch/finally
    try {
        console.log("In try block");
        throw new Error("Another error");
    } catch (error: any) {
        console.log("In catch block:", error.message);
    } finally {
        console.log("In finally block");
    }
    
    // Different error types
    try {
        throw new TypeError("Type error");
    } catch (error: any) {
        console.log("Caught TypeError:", error.name);
    }
    
    try {
        throw new ReferenceError("Reference error");
    } catch (error: any) {
        console.log("Caught ReferenceError:", error.name);
    }
    
    // Function that throws
    function riskyFunction(): void {
        throw new Error("Function error");
    }
    
    try {
        riskyFunction();
    } catch (error: any) {
        console.log("Function threw:", error.message);
    }
    
    // Nested try/catch
    try {
        try {
            throw new Error("Inner error");
        } catch (innerError: any) {
            console.log("Inner catch:", innerError.message);
            throw new Error("Outer error");
        }
    } catch (outerError: any) {
        console.log("Outer catch:", outerError.message);
    }
    
    // No error case
    try {
        console.log("No error thrown");
    } catch (error: any) {
        console.log("This should not print");
    }
    
    console.log("End of function");
} 