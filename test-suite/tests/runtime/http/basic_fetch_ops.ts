export default async function(Myco: any) {
    console.log("Starting basic HTTP fetch test");
    
    try {
        console.log("Fetching Myco README...");
        
        // Request a token to fetch the specific README URL
        const token = await Myco.http.requestFetch("https://raw.githubusercontent.com/mycojs/myco/d234d1876b760ed9fba3a2239890cdc4584d460b/README.md");
        
        // Fetch the content
        const content = await token.fetch();
        
        console.log(`Fetch successful, content length: ${content.length}`);
        
        // Validate the content
        const hasMycoTitle = content.includes("# Myco");
        const hasObjectCapability = content.includes("object-capability model");
        
        console.log(`Content includes '# Myco': ${hasMycoTitle}`);
        console.log(`Content includes 'object-capability model': ${hasObjectCapability}`);
        
        console.log("Basic HTTP fetch test completed");
        
    } catch (error) {
        console.error("Basic HTTP fetch test failed:", error);
        throw error;
    }
} 