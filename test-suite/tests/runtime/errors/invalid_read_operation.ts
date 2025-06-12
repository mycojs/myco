export default async function(myco: Myco) {
    console.log("Testing invalid read operation");
    
    // Try to read a directory as if it were a file
    const readToken = await myco.files.requestRead("./fixtures");
    await readToken.read();
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 