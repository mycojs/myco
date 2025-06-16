export default async function(myco: Myco) {
    console.log("Starting PATH binary execution test");

    // Test execution of a binary in PATH without specifying full path
    console.log("Testing 'sh' binary from PATH");
    const shToken = await myco.files.requestExec("sh");
    console.log("Successfully requested 'sh' token from PATH");
    
    // Test simple command execution
    const result1 = await shToken.exec(["-c", "echo 'Hello from sh!'"]);
    console.log(`Exit code: ${result1.exit_code}`);
    console.log(`Stdout: ${result1.stdout()}`);
    console.log(`Stderr: ${result1.stderr()}`);
    
    if (result1.exit_code !== 0) {
        throw new Error("sh execution failed");
    }
    
    // Test sync execution
    console.log("Testing sync execution of PATH binary");
    const syncResult = shToken.sync.exec(["-c", "echo 'Sync hello from sh!'"]);
    console.log(`Sync exit code: ${syncResult.exit_code}`);
    console.log(`Sync stdout: ${syncResult.stdout()}`);
    
    if (syncResult.exit_code !== 0) {
        throw new Error("sh sync execution failed");
    }
    
    // Test that traditional path-based execution still works
    console.log("Testing traditional path-based execution still works");
    const pathBasedToken = await myco.files.requestExec("./fixtures/test_script.sh");
    const pathResult = await pathBasedToken.exec();
    console.log(`Path-based exit code: ${pathResult.exit_code}`);
    console.log(`Path-based stdout: ${pathResult.stdout()}`);
    
    if (pathResult.exit_code !== 0) {
        throw new Error("Path-based execution failed");
    }
    
    // Test error case: non-existent binary
    console.log("Testing non-existent binary error handling");
    try {
        await myco.files.requestExec("nonexistent_binary_12345");
        throw new Error("Should have thrown an error for non-existent binary");
    } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.log(`Expected error for non-existent binary: ${errorMessage}`);
        if (!errorMessage.includes("not found in PATH")) {
            throw new Error("Expected 'not found in PATH' error message");
        }
    }
    
    // Test that paths with separators still use traditional resolution
    console.log("Testing that paths with separators use traditional resolution");
    try {
        await myco.files.requestExec("nonexistent/binary");
        throw new Error("Should have thrown an error for non-existent path");
    } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.log(`Expected error for non-existent path: ${errorMessage}`);
        if (errorMessage.includes("not found in PATH")) {
            throw new Error("Should not have used PATH resolution for path with separators");
        }
    }
    
    console.log("PATH binary execution test completed successfully");
} 