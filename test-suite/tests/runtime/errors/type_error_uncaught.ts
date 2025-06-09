export default function(myco: Myco) {
    console.log("Testing TypeError uncaught");
    
    // This should throw a TypeError and not be caught
    throw new TypeError("Type error uncaught");
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 