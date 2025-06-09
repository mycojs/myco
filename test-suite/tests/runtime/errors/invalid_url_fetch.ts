export default async function(myco: Myco) {
    console.log("Testing invalid URL fetch");
    
    // This should throw an error and not be caught
    const token = await myco.http.requestFetch("not-a-valid-url");
    await token.fetch();
    
    // This line should never be reached
    console.log("ERROR: Should have thrown an error");
} 