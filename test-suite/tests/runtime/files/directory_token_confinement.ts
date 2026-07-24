async function expectRejected(label: string, action: () => Promise<unknown>) {
    try {
        await action();
        console.log(`${label}: NOT REJECTED`);
        return;
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (message.includes("outside of the token's scope")) {
            console.log(`${label}: rejected`);
        } else {
            console.log(`${label}: wrong error: ${message}`);
        }
    }
}

function expectRejectedSync(label: string, action: () => unknown) {
    return expectRejected(label, async () => action());
}

export default async function(myco: Myco) {
    console.log("Starting directory token confinement test");

    const writeDir = await myco.files.requestWriteDir("./fixtures/tmp");
    const readDir = await myco.files.requestReadDir("./fixtures/tmp");
    const execDir = await myco.files.requestExecDir("./fixtures/tmp");

    await writeDir.write("inside.txt", "inside the token's directory");
    await writeDir.mkdirp("sub");

    console.log(`In-scope read: ${await readDir.read("inside.txt")}`);
    // `..` is applied to the path rather than left in it, so traversal that
    // lands back inside the directory resolves normally.
    console.log(`Traversal staying inside: ${await readDir.read("sub/../inside.txt")}`);
    console.log(`Directory root as "/": ${(await readDir.list("/")).length} entries`);

    // Escapes reach nothing: none of these paths may resolve outside
    // ./fixtures/tmp, which is what the token was issued for.
    await expectRejected("Read ../test_script.sh", () => readDir.read("../test_script.sh"));
    await expectRejected("Read /../test_script.sh", () => readDir.read("/../test_script.sh"));
    await expectRejected("Read sub/../../test_script.sh", () => readDir.read("sub/../../test_script.sh"));
    await expectRejected("Read ../../../../../../etc/passwd", () => readDir.read("../../../../../../etc/passwd"));
    await expectRejected("Stat ../test_script.sh", () => readDir.stat("../test_script.sh"));
    await expectRejected("List ..", () => readDir.list(".."));

    await expectRejected("Write ../escaped.txt", () => writeDir.write("../escaped.txt", "escaped"));
    await expectRejected("Remove ../escaped.txt", () => writeDir.remove("../escaped.txt"));
    await expectRejected("Mkdirp ../escaped_dir", () => writeDir.mkdirp("../escaped_dir"));
    await expectRejected("Rmdir ../escaped_dir", () => writeDir.rmdir("../escaped_dir"));

    await expectRejected("Exec ../test_script.sh", () => execDir.exec("../test_script.sh"));

    // The synchronous ops resolve paths the same way.
    await expectRejectedSync("Sync read ../test_script.sh", () => readDir.sync.read("../test_script.sh"));
    await expectRejectedSync("Sync write ../escaped.txt", () => writeDir.sync.write("../escaped.txt", "escaped"));
    await expectRejectedSync("Sync exec ../test_script.sh", () => execDir.sync.exec("../test_script.sh"));

    console.log("Directory token confinement test completed");
}
