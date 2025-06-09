export default async function(myco: Myco) {
    console.log("Testing network error (non-existent domain)");
    
    // This should throw an error and not be caught
    const token = await myco.http.requestFetch("https://non-existent-domain-12345.com/");
    await token.fetch();
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 