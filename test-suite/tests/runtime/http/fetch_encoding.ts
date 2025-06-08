export default async function(Myco: any) {
    console.log("Starting HTTP fetch encoding test");
    
    try {
        const url = "https://raw.githubusercontent.com/mycojs/myco/d234d1876b760ed9fba3a2239890cdc4584d460b/README.md";
        
        console.log("Testing UTF-8 encoding");
        const token1 = await Myco.http.requestFetch(url);
        const utf8Content = await token1.fetch('utf-8');
        console.log(`UTF-8 fetch successful, length: ${utf8Content.length}`);
        
        console.log("Testing raw encoding");
        const token2 = await Myco.http.requestFetch(url);
        const rawContent = await token2.fetch('raw');
        console.log(`Raw fetch successful, length: ${rawContent.length}`);
        
        // Convert raw to UTF-8 and compare
        const decoder = new TextDecoder();
        const decodedRaw = decoder.decode(rawContent);
        const matches = utf8Content === decodedRaw;
        
        console.log(`Both results match: ${matches}`);
        
        console.log("HTTP fetch encoding test completed");
        
    } catch (error) {
        console.error("HTTP fetch encoding test failed:", error);
        throw error;
    }
} 