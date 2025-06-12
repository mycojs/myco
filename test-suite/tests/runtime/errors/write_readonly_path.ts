export default async function(myco: Myco) {
    console.log("Testing write to read-only path");
    
    // First create a file and make it read-only
    const setupToken = await myco.files.requestWrite("./fixtures/tmp/readonly_test.txt");
    await setupToken.write("initial content");
    
    // Try to make it read-only using filesystem permissions (this might not work on all systems)
    // But we'll try to write to /dev/null or another protected location instead
    const writeToken = await myco.files.requestWrite("/dev/null/should_fail.txt");
    await writeToken.write("test content");
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 