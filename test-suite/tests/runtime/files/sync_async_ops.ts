export default async function(myco: Myco) {
    console.log("Starting sync vs async operations test");
    
    console.log("Testing async operations");
    const asyncWriteToken = await myco.files.requestWrite("./tests/runtime/files/fixtures/tmp/async_test.txt");
    await asyncWriteToken.write("Async content");
    console.log("Async write completed");
    
    const asyncReadToken = await myco.files.requestRead("./tests/runtime/files/fixtures/tmp/async_test.txt");
    const asyncContent = await asyncReadToken.read();
    console.log(`Async read result: ${asyncContent}`);
    
    const asyncStats = await asyncReadToken.stat();
    if (asyncStats) {
        console.log(`Async stats - size: ${asyncStats.size}`);
    }
    
    console.log("Testing sync operations");
    const syncWriteToken = await myco.files.requestWrite("./tests/runtime/files/fixtures/tmp/sync_test.txt");
    syncWriteToken.sync.write("Sync content");
    console.log("Sync write completed");
    
    const syncReadToken = await myco.files.requestRead("./tests/runtime/files/fixtures/tmp/sync_test.txt");
    const syncContent = syncReadToken.sync.read();
    console.log(`Sync read result: ${syncContent}`);
    
    const syncStats = syncReadToken.sync.stat();
    if (syncStats) {
        console.log(`Sync stats - size: ${syncStats.size}`);
    }
    
    console.log("Testing mixed operations");
    // Write async, read sync
    const mixedWriteToken = await myco.files.requestWrite("./tests/runtime/files/fixtures/tmp/mixed_test.txt");
    await mixedWriteToken.write("Mixed async/sync content");
    
    const mixedReadToken = await myco.files.requestRead("./tests/runtime/files/fixtures/tmp/mixed_test.txt");
    const mixedContent = mixedReadToken.sync.read();
    console.log(`Mixed content: ${mixedContent}`);
    
    // Cleanup
    await asyncWriteToken.remove();
    await syncWriteToken.remove();
    await mixedWriteToken.remove();
    
    console.log("Sync vs async operations test completed");
} 