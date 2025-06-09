export default async function(myco: Myco) {
    console.log("Starting current working directory test");
    
    // Test basic cwd functionality
    const currentDir = myco.files.cwd();
    console.log(`Current working directory: ${currentDir}`);
    
    // Verify cwd returns a string
    if (typeof currentDir !== 'string') {
        throw new Error(`Expected cwd() to return string, got ${typeof currentDir}`);
    }
    console.log("cwd() returns string type: true");
    
    // Verify cwd returns an absolute path (should start with '/')
    const isAbsolute = currentDir.startsWith('/');
    console.log(`cwd() returns absolute path: ${isAbsolute}`);
    
    // Test that cwd is consistent across multiple calls
    const currentDir2 = myco.files.cwd();
    const consistent = currentDir === currentDir2;
    console.log(`cwd() is consistent: ${consistent}`);
    
    // Test cwd in context of file operations
    console.log("Testing cwd in context of file operations");
    
    // Create a file using relative path
    const writeToken = await myco.files.requestWrite("test_cwd_file.txt");
    await writeToken.write("Testing cwd context");
    console.log("Created file with relative path");
    
    // Read it back to verify it was created in the current directory
    const readToken = await myco.files.requestRead("test_cwd_file.txt");
    const content = await readToken.read();
    console.log(`File content: ${content}`);
    
    // Clean up
    await writeToken.remove();
    console.log("Cleaned up test file");
    
    // Test that cwd doesn't change during file operations
    const finalCwd = myco.files.cwd();
    const cwdUnchanged = currentDir === finalCwd;
    console.log(`cwd unchanged after operations: ${cwdUnchanged}`);
    
    console.log("Current working directory test completed");
} 