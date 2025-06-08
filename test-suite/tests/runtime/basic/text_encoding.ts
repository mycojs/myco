export default function(Myco: any) {
    // Test TextEncoder
    console.log("Testing TextEncoder:");
    
    const encoder = new TextEncoder();
    
    // Test basic string encoding
    const basicString = "Hello, World!";
    const encoded1 = encoder.encode(basicString);
    console.log("Encoded basic string length:", encoded1.length);
    console.log("Encoded basic string type:", encoded1.constructor.name);
    
    // Test UTF-8 encoding with special characters
    const utf8String = "Hello, 世界! 🚀";
    const encoded2 = encoder.encode(utf8String);
    console.log("Encoded UTF-8 string length:", encoded2.length);
    
    // Test empty string
    const emptyString = "";
    const encoded3 = encoder.encode(emptyString);
    console.log("Encoded empty string length:", encoded3.length);
    
    // Test TextDecoder
    console.log("Testing TextDecoder:");
    
    const decoder = new TextDecoder();
    
    // Test basic string decoding
    const decoded1 = decoder.decode(encoded1);
    console.log("Decoded basic string:", decoded1);
    console.log("Basic string round-trip success:", decoded1 === basicString);
    
    // Test UTF-8 decoding
    const decoded2 = decoder.decode(encoded2);
    console.log("Decoded UTF-8 string:", decoded2);
    console.log("UTF-8 string round-trip success:", decoded2 === utf8String);
    
    // Test empty string decoding
    const decoded3 = decoder.decode(encoded3);
    console.log("Decoded empty string:", decoded3);
    console.log("Empty string round-trip success:", decoded3 === emptyString);
    
    // Test manual byte array
    const manualBytes = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"
    const decodedManual = decoder.decode(manualBytes);
    console.log("Decoded manual bytes:", decodedManual);
    
    console.log("TextEncoder/TextDecoder tests completed");
} 