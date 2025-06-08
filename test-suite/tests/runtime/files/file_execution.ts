export default async function(myco: Myco) {
    console.log("Starting file execution test");
    
    // Create an executable script
    const writeToken = await myco.files.requestWrite("/tmp/test_script.sh");
    const scriptContent = `#!/bin/bash
if [ $# -eq 0 ]; then
    echo "Hello from script!"
else
    echo "Script args: $*"
fi
`;
    await writeToken.write(scriptContent);
    console.log("Created executable script");
    
    // Test execution with no arguments
    const execToken = await myco.files.requestExec("/tmp/test_script.sh");
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
    const writeDirToken = await myco.files.requestWriteDir("/tmp/exec_test_dir");
    await writeDirToken.write("dir_script.sh", scriptContent);
    
    const execDirToken = await myco.files.requestExecDir("/tmp/exec_test_dir");
    const dirResult = await execDirToken.exec("dir_script.sh");
    console.log(`Dir exec exit code: ${dirResult.exit_code}`);
    console.log(`Dir exec stdout: ${dirResult.stdout()}`);
    
    // Cleanup
    await writeToken.remove();
    await writeDirToken.remove("dir_script.sh");
    // Note: We don't remove the directory itself as it may cause issues
    
    console.log("File execution test completed");
} 