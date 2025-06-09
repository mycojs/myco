export default async function(myco: Myco) {
    console.log("Testing read non-existent file");
    
    // This should throw an error and not be caught
    const readToken = await myco.files.requestRead("./tests/runtime/errors/fixtures/tmp/nonexistent_file.txt");
    await readToken.read();
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 