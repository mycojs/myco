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

        const monorepoDir = await myco.files.requestReadWriteDir(".");

        // Clean up any existing myco-local.toml files and tsconfig.json files and .myco directories
        await monorepoDir.rmdirRecursive("./cli/.myco");
        await monorepoDir.rmdirRecursive("./test-suite/.myco");
        await monorepoDir.remove("./cli/tsconfig.json");
        await monorepoDir.remove("./test-suite/tsconfig.json");
        await monorepoDir.remove("./cli/myco-local.toml");
        await monorepoDir.remove("./test-suite/myco-local.toml");
        
        // Execute workspace install command
        const result = await mycoExec.exec(["ws", "install", "--save"]);
        
        if (result.exit_code !== 0) {
            console.error("Command failed:", result.stderr());
            throw new Error(`Command failed with exit code ${result.exit_code}`);
        }
        
        console.log("Workspace install completed successfully");
        
        // Check that myco-local.toml files were created for packages with workspace dependencies
        await checkMycoLocal(myco, "cli");
        await checkMycoLocal(myco, "test-suite");
        
        // Check that tsconfig.json files were generated with correct path mappings
        await checkTsconfig(myco, "cli");
        await checkTsconfig(myco, "test-suite");
        
        console.log("All workspace dependency files generated correctly");
        
    } finally {
        // Restore the original working directory
        myco.files.chdir(originalCwd);
    }
}

async function checkMycoLocal(myco: Myco, packageName: string) {
    try {
        const readToken = await myco.files.requestRead(`${packageName}/.myco/myco-local.toml`);
        const content = await readToken.read();
        const parsed = TOML.parse(content);
        
        console.log(`${packageName} myco-local.toml:`, content);
        
        // Check that resolve mappings exist
        if (!parsed.resolve) {
            throw new Error(`No resolve section found in ${packageName}/.myco/myco-local.toml`);
        }
        
        // Check for the @local/lib-std dependency
        if (!parsed.resolve["@local/lib-std"]) {
            throw new Error(`No @local/lib-std mapping found in ${packageName}/.myco/myco-local.toml`);
        }
        
        const libStdPath = parsed.resolve["@local/lib-std"][0];
        console.log(`${packageName} maps @local/lib-std to: ${libStdPath}`);
        
        // Verify the path looks correct (should be relative path to lib/std)
        if (!libStdPath.includes("lib/std") && !libStdPath.includes("../lib/std")) {
            throw new Error(`Unexpected path mapping for @local/lib-std: ${libStdPath}`);
        }
        
    } catch (error: any) {
        if (error.message && error.message.includes("No such file")) {
            throw new Error(`myco-local.toml not found for ${packageName}`);
        }
        throw error;
    }
}

async function checkTsconfig(myco: Myco, packageName: string) {
    try {
        const readToken = await myco.files.requestRead(`${packageName}/tsconfig.json`);
        const content = await readToken.read();
        
        // Strip the comment from the beginning of the JSON file
        const jsonContent = content.replace(/^\/\/.*\n/, '');
        const parsed = JSON.parse(jsonContent);
        
        console.log(`${packageName} tsconfig.json paths:`, JSON.stringify(parsed.compilerOptions?.paths, null, 2));
        
        // Check that path mappings exist
        if (!parsed.compilerOptions?.paths) {
            throw new Error(`No compilerOptions.paths found in ${packageName}/tsconfig.json`);
        }
        
        // Check for the @local/lib-std dependency mapping
        const libStdPath = parsed.compilerOptions.paths["@local/lib-std"];
        if (!libStdPath) {
            throw new Error(`No @local/lib-std path mapping found in ${packageName}/tsconfig.json`);
        }
        
        console.log(`${packageName} tsconfig maps @local/lib-std to: ${libStdPath}`);
        
        // Check that wildcard mapping also exists
        const libStdWildcardPath = parsed.compilerOptions.paths["@local/lib-std/*"];
        if (!libStdWildcardPath) {
            throw new Error(`No @local/lib-std/* wildcard path mapping found in ${packageName}/tsconfig.json`);
        }
        
        console.log(`${packageName} tsconfig maps @local/lib-std/* to: ${libStdWildcardPath}`);
        
    } catch (error: any) {
        if (error.message && error.message.includes("No such file")) {
            throw new Error(`tsconfig.json not found for ${packageName}`);
        }
        throw error;
    }
} 