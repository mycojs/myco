export default async function(myco: Myco) {
    console.log("Starting directory operations test");
    
    // Set up test directory structure
    const writeDirToken = await myco.files.requestWriteDir("./fixtures/tmp");
    
    // Create a test file
    await writeDirToken.write("test.txt", "Test content");
    console.log("Created test file: test.txt");
    
    // Create a subdirectory
    await writeDirToken.mkdirp("subdir");
    console.log("Created subdirectory");
    
    // Create a file in the subdirectory
    await writeDirToken.write("subdir/nested.txt", "Nested content");
    console.log("Created nested file: nested.txt");
    
    // List directory contents
    const readDirToken = await myco.files.requestReadDir("./fixtures/tmp");
    const files = await readDirToken.list(".");
    
    console.log("Directory listing:");
    // sort by type and then by name
    const filesSorted = files.sort((a, b) => {
        if (a.stats.is_file && !b.stats.is_file) return -1;
        if (!a.stats.is_file && b.stats.is_file) return 1;
        return a.name.localeCompare(b.name);
    }); 
    for (const file of filesSorted) {
        if (file.stats.is_file) {
            console.log(`  File: ${file.name} (size: ${file.stats.size})`);
        } else if (file.stats.is_dir) {
            console.log(`  Dir: ${file.name}`);
        }
    }
    
    // List subdirectory contents
    const subFiles = await readDirToken.list("subdir");
    console.log("Subdirectory listing:");
    // sort by type and then by name
    const subFilesSorted = subFiles.sort((a, b) => {
        if (a.stats.is_file && !b.stats.is_file) return -1;
        if (!a.stats.is_file && b.stats.is_file) return 1;
        return a.name.localeCompare(b.name);
    });
    for (const file of subFilesSorted) {
        if (file.stats.is_file) {
            console.log(`  File: ${file.name} (size: ${file.stats.size})`);
        }
    }
    
    console.log("Testing list options");
    
    // Test filtering by extension
    const txtFiles = await readDirToken.list(".", { 
        extensions: ["txt"],
        recursive: true 
    });
    const txtFileNames = txtFiles.map(f => f.name).sort();
    console.log(`Filtered files (*.txt): ${txtFileNames.join(', ')}`);
    
    // Test files only
    const filesOnly = await readDirToken.list(".", { 
        include_dirs: false,
        recursive: true 
    });
    const fileNames = filesOnly.map(f => f.name).sort();
    console.log(`Files only: ${fileNames.join(', ')}`);
    
    // Test directories only
    const dirsOnly = await readDirToken.list(".", { 
        include_files: false 
    });
    const dirNames = dirsOnly.map(f => f.name);
    console.log(`Dirs only: ${dirNames.join(', ')}`);
    
    console.log("Testing recursive listing");
    const recursiveFiles = await readDirToken.list(".", { 
        recursive: true,
        include_dirs: false 
    });
    const recursiveFileNames = recursiveFiles.map(f => f.name).sort();
    console.log(`Recursive files: ${recursiveFileNames.join(', ')}`);
    
    console.log("Testing recursive directory removal");
    
    // Create a complex nested directory structure for testing rmdirRecursive
    await writeDirToken.mkdirp("deep/nested/structure");
    await writeDirToken.write("deep/file1.txt", "Deep file 1");
    await writeDirToken.write("deep/nested/file2.txt", "Nested file 2");
    await writeDirToken.write("deep/nested/structure/file3.txt", "Deep nested file 3");
    await writeDirToken.mkdirp("deep/another/branch");
    await writeDirToken.write("deep/another/file4.txt", "Another branch file");
    await writeDirToken.write("deep/another/branch/file5.txt", "Branch file 5");
    
    console.log("Created complex nested directory structure");
    
    // Verify the structure exists
    const deepFiles = await readDirToken.list("deep", { recursive: true, include_dirs: false });
    const deepFileNames = deepFiles.map(f => f.name).sort();
    console.log(`Files in deep structure: ${deepFileNames.join(', ')}`);
    
    // Test recursive removal
    await writeDirToken.rmdirRecursive("deep");
    console.log("Recursively removed deep directory structure");
    
    // Verify the directory was completely removed
    try {
        await readDirToken.list("deep");
        console.log("ERROR: deep directory still exists after recursive removal");
    } catch (error) {
        console.log("Confirmed: deep directory completely removed");
    }
    
    // Test rmdirRecursive on a directory with mixed content
    await writeDirToken.mkdirp("mixed/sub1");
    await writeDirToken.mkdirp("mixed/sub2/subsub");
    await writeDirToken.write("mixed/root.txt", "Root file");
    await writeDirToken.write("mixed/sub1/file1.txt", "Sub1 file");
    await writeDirToken.write("mixed/sub2/file2.txt", "Sub2 file");
    await writeDirToken.write("mixed/sub2/subsub/file3.txt", "Deep file");
    
    console.log("Created mixed content directory");
    
    // Remove it recursively
    await writeDirToken.rmdirRecursive("mixed");
    console.log("Recursively removed mixed content directory");
    
    console.log("Cleaning up");
    
    // Remove files and directories
    await writeDirToken.remove("subdir/nested.txt");
    await writeDirToken.rmdir("subdir");
    await writeDirToken.remove("test.txt");
    
    console.log("Removed files and directories");
    
    console.log("Directory operations test completed");
} 