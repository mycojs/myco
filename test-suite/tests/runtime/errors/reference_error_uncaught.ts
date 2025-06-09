export default function(myco: Myco) {
    console.log("Testing ReferenceError uncaught");
    
    // This should throw a ReferenceError and not be caught
    throw new ReferenceError("Reference error uncaught");
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 