export default async function(myco: Myco) {
    console.log("Testing file stats on non-existent file");
    
    // This should throw an error and not be caught
    const readToken = await myco.files.requestRead("./tests/runtime/errors/fixtures/tmp/definitely_nonexistent.txt");
    await readToken.stat();
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 