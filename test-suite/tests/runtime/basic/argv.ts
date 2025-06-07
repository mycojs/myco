export default function main(Myco: any) {
    console.log("Testing Myco.argv:");
    
    // Test that argv exists
    if (!Myco.argv) {
        console.error("Myco.argv is not defined");
        return;
    }
    
    // Test that argv is an array
    if (!Array.isArray(Myco.argv)) {
        console.error("Myco.argv is not an array");
        return;
    }
    
    console.log("argv is array:", Array.isArray(Myco.argv));
    console.log("argv length:", Myco.argv.length);
    
    // Test that argv contains at least the program name
    if (Myco.argv.length === 0) {
        console.error("Myco.argv is empty");
        return;
    }
    
    console.log("First arg (program):", Myco.argv[0]);
    
    // Print all arguments with their indices
    for (let i = 0; i < Myco.argv.length; i++) {
        console.log(`argv[${i}]:`, Myco.argv[i]);
    }
    
    // Test with custom arguments if provided
    if (Myco.argv.length > 3) {
        console.log("Custom arguments found:");
        for (let i = 3; i < Myco.argv.length; i++) {
            console.log(`  Custom arg ${i - 3}:`, Myco.argv[i]);
        }
    } else {
        console.log("No custom arguments provided");
    }
    
    console.log("Argv test completed");
} 