export default async function(myco: Myco) {
    console.log("Starting file execution test");

    // Test execution with no arguments
    const execToken = await myco.files.requestExec("./fixtures/test_script.sh");
    console.log("Executing script with no args");
    const result1 = await execToken.exec();
    console.log(`Exit code: ${result1.exit_code}`);
    console.log(`Stdout: ${result1.stdout()}`);
    console.log(`Stderr: ${result1.stderr()}`);
    
    // Test execution with arguments
    console.log("Executing script with args");
    const result2 = await execToken.exec(["arg1", "arg2"]);
    console.log(`Exit code: ${result2.exit_code}`);
    console.log(`Stdout: ${result2.stdout()}`);
    console.log(`Stderr: ${result2.stderr()}`);
    
    // Test sync execution
    console.log("Testing sync execution");
    const syncResult = execToken.sync.exec();
    console.log(`Sync exit code: ${syncResult.exit_code}`);
    console.log(`Sync stdout: ${syncResult.stdout()}`);
    
    // Test directory execution
    console.log("Testing directory execution");
    const execDirToken = await myco.files.requestExecDir("./fixtures");
    const dirResult = await execDirToken.exec("test_script.sh");
    console.log(`Dir exec exit code: ${dirResult.exit_code}`);
    console.log(`Dir exec stdout: ${dirResult.stdout()}`);
    
    console.log("File execution test completed");
} 