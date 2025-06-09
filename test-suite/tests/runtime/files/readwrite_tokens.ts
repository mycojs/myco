export default async function(myco: Myco) {
    console.log("Starting read-write tokens test");
    
    console.log("Testing read-write token");
    const rwToken = await myco.files.requestReadWrite("./tests/runtime/files/fixtures/tmp/readwrite_test.txt");
    
    // Write initial content
    await rwToken.write("Initial content for read-write test");
    console.log("Initial content written");
    
    // Read it back
    const initialContent = await rwToken.read();
    console.log(`Content read back: ${initialContent}`);
    
    // Update the content
    await rwToken.write("Updated content for read-write test");
    console.log("Content updated");
    
    // Read updated content
    const updatedContent = await rwToken.read();
    console.log(`Updated content: ${updatedContent}`);
    
    console.log("Testing read-write directory token");
    const rwDirToken = await myco.files.requestReadWriteDir("./tests/runtime/files/fixtures/tmp");
    
    // Write a file in the directory
    await rwDirToken.write("testfile.txt", "Directory file content");
    console.log("Directory file written");
    
    // Read it back
    const dirFileContent = await rwDirToken.read("testfile.txt");
    console.log(`Directory file read: ${dirFileContent}`);
    
    // Update directory file
    await rwDirToken.write("testfile.txt", "Updated directory content");
    console.log("Directory file updated");
    
    // Read updated content
    const updatedDirContent = await rwDirToken.read("testfile.txt");
    console.log(`Updated directory content: ${updatedDirContent}`);
    
    // Cleanup
    await rwToken.remove();
    await rwDirToken.remove("testfile.txt");
    // Note: We don't remove the directory itself as it may cause issues
    
    console.log("Read-write tokens test completed");
} 