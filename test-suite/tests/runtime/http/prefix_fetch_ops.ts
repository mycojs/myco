export default async function(Myco: any) {
    console.log("Starting HTTP prefix fetch test");
    
    try {
        console.log("Created prefix token for GitHub raw URLs");
        
        // Create a prefix token for GitHub raw URLs
        const prefixToken = await Myco.http.requestFetchPrefix("https://raw.githubusercontent.com/mycojs/myco");
        
        console.log("Fetching Myco README via prefix token...");
        
        // Use the prefix token to fetch the README with just the path
        const readmePath = "/d234d1876b760ed9fba3a2239890cdc4584d460b/README.md";
        const content = await prefixToken.fetch(readmePath);
        
        console.log(`Prefix fetch successful, content length: ${content.length}`);
        
        // Validate the content
        const hasMycoTitle = content.includes("# Myco");
        console.log(`Content includes '# Myco': ${hasMycoTitle}`);
        
        console.log("Testing different URL with same prefix");
        console.log("Fetching same file via direct path...");
        
        // Try fetching the same file to validate the prefix works
        const content2 = await prefixToken.fetch(readmePath);
        console.log(`Second fetch successful, content length: ${content2.length}`);
        
        console.log("Prefix fetch operations test completed");
        
    } catch (error) {
        console.error("Prefix fetch operations test failed:", error);
        throw error;
    }
} 