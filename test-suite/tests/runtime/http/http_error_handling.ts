export default async function(Myco: any) {
    console.log("Starting HTTP error handling test");
    
    try {
        console.log("Testing invalid URL");
        try {
            const token = await Myco.http.requestFetch("not-a-valid-url");
            await token.fetch();
            console.error("Expected error for invalid URL but none occurred");
        } catch (error) {
            console.log("Caught invalid URL error as expected");
        }
        
        console.log("Testing path traversal attack");
        try {
            const prefixToken = await Myco.http.requestFetchPrefix("https://raw.githubusercontent.com/mycojs/myco");
            // Try to use path traversal to escape the prefix
            await prefixToken.fetch("/../../../etc/passwd");
            console.error("Expected error for path traversal but none occurred");
        } catch (error) {
            console.log("Caught path traversal error as expected");
        }
        
        console.log("Testing path traversal with relative segments");
        try {
            const prefixToken = await Myco.http.requestFetchPrefix("https://raw.githubusercontent.com/mycojs/myco");
            // Try to use path traversal within a valid-looking path
            await prefixToken.fetch("/valid/path/../../../etc/passwd");
            console.error("Expected error for path traversal but none occurred");
        } catch (error) {
            console.log("Caught path traversal error as expected");
        }
        
        console.log("Testing URL injection");
        try {
            const prefixToken = await Myco.http.requestFetchPrefix("https://raw.githubusercontent.com/mycojs/myco");
            // Try to inject a full URL to escape the prefix
            await prefixToken.fetch("https://example.com/malicious");
            console.error("Expected error for URL injection but none occurred");
        } catch (error) {
            console.log("Caught URL injection error as expected");
        }
        
        console.log("Testing protocol injection");
        try {
            const prefixToken = await Myco.http.requestFetchPrefix("https://raw.githubusercontent.com/mycojs/myco");
            // Try to inject a different protocol
            await prefixToken.fetch("file:///etc/passwd");
            console.error("Expected error for protocol injection but none occurred");
        } catch (error) {
            console.log("Caught protocol injection error as expected");
        }
        
        console.log("Testing network error (non-existent domain)");
        try {
            const token = await Myco.http.requestFetch("https://non-existent-domain-12345.com/");
            await token.fetch();
            console.error("Expected network error but none occurred");
        } catch (error) {
            console.log("Caught network error as expected");
        }
        
        console.log("HTTP error handling test completed");
        
    } catch (error) {
        console.error("HTTP error handling test failed:", error);
        throw error;
    }
} 