// Exec tokens advertise `stat`, and it has to work: knowing the size and mtime
// of something you may run is not authority beyond running it, and
// `requestExec` already stats the path to check the executable bit.

export default async function(myco: Myco) {
    console.log("Starting exec token stat test");

    const execFile = await myco.files.requestExec("./fixtures/test_script.sh");
    const fileStats = await execFile.stat();
    console.log(`Exec file stat is_file: ${fileStats?.is_file}`);
    console.log(`Exec file stat has size: ${(fileStats?.size ?? 0) > 0}`);
    console.log(`Exec file sync stat is_file: ${execFile.sync.stat()?.is_file}`);

    const execDir = await myco.files.requestExecDir("./fixtures");
    const dirStats = await execDir.stat("test_script.sh");
    console.log(`Exec dir stat is_file: ${dirStats?.is_file}`);
    console.log(`Exec dir sync stat is_file: ${execDir.sync.stat("test_script.sh")?.is_file}`);

    // Confinement still applies to the path a stat is asked for.
    try {
        await execDir.stat("../../../etc/passwd");
        console.log("Escaping stat: NOT REJECTED");
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        console.log(
            `Escaping stat: ${message.includes("outside of the token's scope") ? "rejected" : `wrong error: ${message}`}`,
        );
    }

    console.log("Exec token stat test completed");
}
