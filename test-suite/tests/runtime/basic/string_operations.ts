export default function (_myco: Myco) {
    // Basic string operations
    const str1 = "Hello";
    const str2 = "World";
    const combined = str1 + " " + str2;
    
    console.log("Combined:", combined);
    console.log("Length:", combined.length);
    
    // String methods
    const text = "JavaScript is awesome";
    console.log("toUpperCase:", text.toUpperCase());
    console.log("toLowerCase:", text.toLowerCase());
    console.log("indexOf 'is':", text.indexOf("is"));
    console.log("substring(0, 10):", text.substring(0, 10));
    console.log("slice(-7):", text.slice(-7));
    
    // String splitting and joining
    const words = text.split(" ");
    console.log("split words:", words.join(","));
    
    // String searching and replacing
    const sentence = "The quick brown fox jumps over the lazy dog";
    console.log("includes 'fox':", sentence.includes("fox"));
    console.log("startsWith 'The':", sentence.startsWith("The"));
    console.log("endsWith 'dog':", sentence.endsWith("dog"));
    console.log("replace 'fox' with 'cat':", sentence.replace("fox", "cat"));
    
    // Template literals
    const name = "Alice";
    const age = 25;
    const greeting = `Hello, my name is ${name} and I am ${age} years old.`;
    console.log("Template literal:", greeting);
    
    // Multi-line strings
    const multiline = `This is
a multi-line
string`;
    console.log("Multiline length:", multiline.length);
    
    // String trimming
    const padded = "  spaced  ";
    console.log("trimmed:", `"${padded.trim()}"`);
    console.log("trimStart:", `"${padded.trimStart()}"`);
    console.log("trimEnd:", `"${padded.trimEnd()}"`);
    
    // Character access
    const word = "TypeScript";
    console.log("charAt(0):", word.charAt(0));
    console.log("charAt(4):", word.charAt(4));
    console.log("charCodeAt(0):", word.charCodeAt(0));
    
    // String repetition and padding
    console.log("repeat:", "ha".repeat(3));
    console.log("padStart:", "5".padStart(3, "0"));
    console.log("padEnd:", "5".padEnd(3, "0"));
} 