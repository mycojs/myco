export default async function(myco: Myco) {
    console.log("Testing invalid directory operation");
    
    // Try to remove a non-existent directory - this should throw an error
    const dirToken = await myco.files.requestWriteDir("./tests/runtime/errors/fixtures/tmp");
    await dirToken.rmdir("nonexistent_directory");
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 