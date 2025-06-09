export default async function(myco: Myco) {
    console.log("Testing path traversal attack");
    
    // This should throw an error and not be caught
    const prefixToken = await myco.http.requestFetchPrefix("https://raw.githubusercontent.com/mycojs/myco");
    await prefixToken.fetch("/../../../etc/passwd");
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 