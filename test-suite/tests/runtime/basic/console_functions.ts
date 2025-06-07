export default function(Myco: any) {
    // Test console.log
    console.log("Testing console.log:");
    console.log("Hello, World!");
    console.log("Number:", 42);
    console.log("Boolean:", true);
    console.log("Null:", null);
    console.log("Undefined:", undefined);
    console.log("Object:", { name: "test", value: 123 });
    console.log("Array:", [1, 2, 3]);
    console.log("Multiple arguments:", "string", 42, true, null);

    // Test console.error
    console.error("Testing console.error:");
    console.error("This is an error message");
    console.error("Error with number:", 404);

    // Test console.warn
    console.warn("Testing console.warn:");
    console.warn("This is a warning message");
    console.warn("Warning with data:", { level: "high" });

    // Test console.info
    console.info("Testing console.info:");
    console.info("This is an info message");
    console.info("Info with details:", "version", "1.0.0");

    // Test console.debug
    console.debug("Testing console.debug:");
    console.debug("This is a debug message");
    console.debug("Debug data:", { timestamp: "2024-01-01T00:00:00.000Z" });

    // Test console.trace
    console.trace("Testing console.trace:");
    console.trace("This is a trace message");
    
    function testFunction() {
        console.trace("Trace from inside function");
    }
    testFunction();

    // Test console.assert
    console.assert(true, "This assertion should pass");
    console.assert(1 === 1, "Math assertion should pass");
    console.assert(false, "This assertion should fail");
    console.assert(0, "Zero assertion should fail");
    console.assert("", "Empty string assertion should fail");
    console.assert(null, "Null assertion should fail");
    console.assert(undefined, "Undefined assertion should fail");
    console.assert(false, "Failed assertion with", "multiple", "arguments");
} 