export default async function(myco: Myco) {
    console.log("Testing URL injection attack");
    
    // This should throw an error and not be caught
    const prefixToken = await myco.http.requestFetchPrefix("https://raw.githubusercontent.com/mycojs/myco");
    await prefixToken.fetch("https://example.com/malicious");
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 