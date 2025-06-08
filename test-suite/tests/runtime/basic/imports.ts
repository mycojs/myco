// Test importing TypeScript files with .ts extension
import defaultFunction, { greeting, add, numbers, config, TestClass } from "./import_fixture.ts";

export default function (_myco: Myco) {
    console.log("Testing imports:");

    // Test string export
    console.log("Imported greeting:", greeting);

    // Test function export
    console.log("add(10, 5):", add(10, 5));

    // Test array export  
    console.log("numbers.length:", numbers.length);
    console.log("numbers[2]:", numbers[2]);

    // Test object export
    console.log("config.name:", config.name);
    console.log("config.enabled:", config.enabled);

    // Test default export
    console.log("Default function:", defaultFunction());

    // Test class export
    const instance = new TestClass("World");
    console.log("Class instance greeting:", instance.greet());

    console.log("All import tests completed successfully!"); 
}