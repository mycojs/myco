export default async function(myco: Myco) {
    console.log("Starting basic file operations test");
    
    // Test basic write and read operations
    const writeToken = await myco.files.requestWrite("./fixtures/tmp/test_file.txt");
    await writeToken.write("Hello, Myco Files!");
    console.log("File written successfully");
    
    const readToken = await myco.files.requestRead("./fixtures/tmp/test_file.txt");
    const content = await readToken.read();
    console.log(`File content read: ${content}`);
    
    // Test file stats
    const stats = await readToken.stat();
    if (stats) {
        console.log(`File stats - is_file: ${stats.is_file}, size: ${stats.size}`);
    }
    
    // Test file removal
    await writeToken.remove();
    console.log("File removed successfully");
    
    // Test UTF-8 encoding with unicode characters
    console.log("Testing UTF-8 encoding");
    const utf8WriteToken = await myco.files.requestWrite("./fixtures/tmp/utf8_test.txt");
    const utf8Content = "Hello, 世界! 🚀";
    await utf8WriteToken.write(utf8Content);
    
    const utf8ReadToken = await myco.files.requestRead("./fixtures/tmp/utf8_test.txt");
    const readUtf8Content = await utf8ReadToken.read('utf-8');
    console.log(`UTF-8 content: ${readUtf8Content}`);
    
    // Test raw encoding
    console.log("Testing raw encoding");
    const rawContent = await utf8ReadToken.read('raw');
    console.log(`Raw content length: ${rawContent.length}`);

    // Test file ops after changing directory
    console.log("Testing relative path after changing directory");
    const relativeReadToken = await myco.files.requestRead("./fixtures/tmp/utf8_test.txt");
    const relativeWriteToken = await myco.files.requestWrite("./fixtures/tmp/utf8_test.txt");
    await myco.files.chdir("./fixtures");
    const relativeReadResult = await relativeReadToken.read();
    console.log(`Relative read content: ${relativeReadResult}`);
    await relativeWriteToken.write("new text");
    const relativeReadResult2 = await relativeReadToken.read();
    console.log(`Relative read content: ${relativeReadResult2}`);
    
    // Cleanup
    await utf8WriteToken.remove();
    
    console.log("Basic file operations test completed");
} 