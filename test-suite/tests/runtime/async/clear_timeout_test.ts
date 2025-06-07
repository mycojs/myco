export default function(Myco: any) {
    console.log("Testing clearTimeout functionality");
    
    let executed = false;
    
    // Set a timeout that should be cancelled
    const timerId = Myco.setTimeout(() => {
        executed = true;
        console.log("This should NOT execute");
    }, 100);
    
    console.log("Timer set with ID:", timerId);
    
    // Clear the timeout immediately
    Myco.clearTimeout(timerId);
    console.log("Timer cleared");
    
    // Set another timeout to check if the first one was really cancelled
    Myco.setTimeout(() => {
        if (executed) {
            console.log("FAIL: First timer executed despite being cleared");
        } else {
            console.log("SUCCESS: First timer was properly cancelled");
        }
    }, 200);
    
    console.log("clearTimeout test completed");
} 