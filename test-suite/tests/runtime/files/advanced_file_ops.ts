export default async function(myco: Myco) {
    console.log("Starting advanced file operations test");
    
    console.log("Testing large file operations");
    // Create a large file (1KB)
    const largeContent = "x".repeat(1000);
    const largeWriteToken = await myco.files.requestWrite("./fixtures/tmp/large_file.txt");
    await largeWriteToken.write(largeContent);
    console.log(`Large file written (${largeContent.length} bytes)`);
    
    const largeReadToken = await myco.files.requestRead("./fixtures/tmp/large_file.txt");
    const readLargeContent = await largeReadToken.read();
    console.log("Large file read successfully");
    console.log(`Content matches: ${largeContent === readLargeContent}`);
    
    console.log("Testing binary data");
    // Create binary data (all byte values 0-255)
    const binaryData = new Uint8Array(256);
    for (let i = 0; i < 256; i++) {
        binaryData[i] = i;
    }
    
    const binaryWriteToken = await myco.files.requestWrite("./fixtures/tmp/binary_file.bin");
    await binaryWriteToken.write(binaryData);
    console.log(`Binary data written (${binaryData.length} bytes)`);
    
    const binaryReadToken = await myco.files.requestRead("./fixtures/tmp/binary_file.bin");
    const readBinaryData = await binaryReadToken.read('raw') as Uint8Array;
    console.log("Binary data read successfully");
    
    // Compare binary data
    let binaryMatches = binaryData.length === readBinaryData.length;
    if (binaryMatches) {
        for (let i = 0; i < binaryData.length; i++) {
            if (binaryData[i] !== readBinaryData[i]) {
                binaryMatches = false;
                break;
            }
        }
    }
    console.log(`Binary data matches: ${binaryMatches}`);
    
    console.log("Testing concurrent operations");
    // Create multiple files concurrently
    const concurrentPromises = [];
    const concurrentCount = 5;
    
    for (let i = 0; i < concurrentCount; i++) {
        const promise = (async () => {
            const writeToken = await myco.files.requestWrite(`./fixtures/tmp/concurrent_${i}.txt`);
            await writeToken.write(`Concurrent file ${i} content`);
            return i;
        })();
        concurrentPromises.push(promise);
    }
    
    const concurrentResults = await Promise.all(concurrentPromises);
    console.log(`Concurrent files written: ${concurrentResults.length}`);
    
    // Read them back concurrently
    const readPromises = [];
    for (let i = 0; i < concurrentCount; i++) {
        const promise = (async () => {
            const readToken = await myco.files.requestRead(`./fixtures/tmp/concurrent_${i}.txt`);
            const content = await readToken.read();
            return { index: i, content };
        })();
        readPromises.push(promise);
    }
    
    const readResults = await Promise.all(readPromises);
    console.log(`Concurrent files read: ${readResults.length}`);
    
    // Verify content
    let allMatch = true;
    for (const result of readResults) {
        const expected = `Concurrent file ${result.index} content`;
        if (result.content !== expected) {
            allMatch = false;
            break;
        }
    }
    console.log(`All contents match: ${allMatch}`);
    
    console.log("Testing file metadata");
    const metadataWriteToken = await myco.files.requestWrite("./fixtures/tmp/metadata_test.txt");
    await metadataWriteToken.write("Metadata test content");
    console.log("File created");
    
    const metadataReadToken = await myco.files.requestRead("./fixtures/tmp/metadata_test.txt");
    const metadata = await metadataReadToken.stat();
    
    if (metadata) {
        console.log(`Modified time exists: ${metadata.modified !== undefined}`);
        console.log(`File is_file: ${metadata.is_file}`);
        console.log(`File is_dir: ${metadata.is_dir}`);
    }
    
    // Cleanup
    await largeWriteToken.remove();
    await binaryWriteToken.remove();
    await metadataWriteToken.remove();
    
    for (let i = 0; i < concurrentCount; i++) {
        const cleanupToken = await myco.files.requestWrite(`./fixtures/tmp/concurrent_${i}.txt`);
        await cleanupToken.remove();
    }
    
    console.log("Advanced file operations test completed");
} 