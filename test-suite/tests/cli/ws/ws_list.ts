export default async function(myco: Myco) {
    // Find the path to the myco binary
    const mycoBinaryPath = myco.argv[3];
    
    // Get the current working directory to restore later
    const originalCwd = myco.files.cwd();
                
    // Get an exec token for the myco binary
    const mycoExec = await myco.files.requestExec(mycoBinaryPath);
    
    try {
        // Change to the test fixture directory
        myco.files.chdir("./fixtures/monorepo");
        
        // Execute workspace list command
        const result = await mycoExec.exec(["ws", "list"]);
        
        if (result.exit_code !== 0) {
            console.error("Command failed:", result.stderr());
            throw new Error(`Command failed with exit code ${result.exit_code}`);
        }
        
        // Output the result which should match the expected output
        const stdout = result.stdout();
        console.log(stdout);
        
    } finally {
        // Restore the original working directory
        myco.files.chdir(originalCwd);
    }
} 