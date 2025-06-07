export default async function(myco: Myco) {
    console.log("Starting file error handling test");
    
    console.log("Testing read non-existent file");
    try {
        const readToken = await myco.files.requestRead("/tmp/nonexistent_file.txt");
        await readToken.read();
        console.log("ERROR: Should have thrown an error");
    } catch (error) {
        console.log("Caught read error as expected");
    }
    
    console.log("Testing write to read-only path");
    try {
        // Try to write to a path that might not be writable
        const writeToken = await myco.files.requestWrite("/tmp/test_readonly.txt");
        await writeToken.write("test content");
        console.log("Write operation handled");
        
        // Try to remove the file we just created
        await writeToken.remove();
    } catch (error) {
        console.log("Write error handled as expected");
    }
    
    console.log("Testing invalid operations");
    try {
        // Create a file and try some operations
        const testWriteToken = await myco.files.requestWrite("/tmp/error_test.txt");
        await testWriteToken.write("test content");
        
        // Test reading from a write token (should work via the interface)
        const testReadToken = await myco.files.requestRead("/tmp/error_test.txt");
        const content = await testReadToken.read();
        
        // Clean up
        await testWriteToken.remove();
        console.log("Invalid operation handled");
    } catch (error) {
        console.log("Caught invalid operation error");
    }
    
    console.log("Testing directory creation errors");
    try {
        // Test creating directory structure
        const writeDirToken = await myco.files.requestWriteDir("/tmp/error_test_dir");
        await writeDirToken.mkdirp("subdir1/subdir2");
        await writeDirToken.write("subdir1/test.txt", "test");
        
        // Clean up
        await writeDirToken.remove("subdir1/test.txt");
        await writeDirToken.rmdir("subdir1/subdir2");
        await writeDirToken.rmdir("subdir1");
        
        console.log("Directory operation handled");
    } catch (error) {
        console.log("Directory error handled");
    }
    
    console.log("Testing file stats on non-existent file");
    try {
        const readToken = await myco.files.requestRead("/tmp/definitely_nonexistent.txt");
        const stats = await readToken.stat();
        console.log(`Non-existent file stats: ${stats}`);
    } catch (error) {
        console.log("Stats error handled");
    }
    
    console.log("File error handling test completed");
} 