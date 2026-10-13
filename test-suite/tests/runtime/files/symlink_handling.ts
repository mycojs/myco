// Following a link checks its target; unlinking checks only its ancestors.
async function describe(label: string, action: () => Promise<unknown>) {
    try {
        await action();
        console.log(`${label}: allowed`);
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (message.includes("outside of the token's scope")) {
            console.log(`${label}: refused as an escape`);
        } else if (message.includes("No such file or directory")) {
            console.log(`${label}: allowed, target absent`);
        } else {
            console.log(`${label}: unexpected error: ${message}`);
        }
    }
}

export default async function(myco: Myco) {
    console.log("Starting symlink handling test");

    const dir = await myco.files.requestReadWriteDir("./fixtures/tmp");
    await dir.rmdirRecursive("links");
    await dir.mkdirp("links");

    // Symlinks require exec; ../../test_script.sh is outside the directory grant.
    const sh = await myco.files.requestExec("sh");
    const build = await sh.exec([
        "-c",
        "cd fixtures/tmp/links && printf inside > target.txt" +
            " && ln -s target.txt live" +
            " && ln -s nowhere.txt broken" +
            " && ln -s ../../test_script.sh escaping" +
            // The target looks local but is reached through an escaping link.
            " && ln -s ../.. out" +
            " && ln -s out/planted.txt via_out" +
            " && mkdir -p sub/deep && ln -s sub/deep deepin" +
            " && ln -s ../../../test_script.sh sub/unlink-me" +
            " && ln -s ../.. rmdir_async && ln -s ../.. rmdir_sync",
    ]);
    if (build.exit_code !== 0) {
        throw new Error(`Failed to build fixture: ${build.stderr()}`);
    }

    console.log(`Read through a live link: ${await dir.read("links/live")}`);

    // A dangling link within the grant can create its missing target.
    await describe("Stat of a dangling link", () => dir.stat("links/broken"));
    await dir.write("links/broken", "written through");
    console.log(`Write through a dangling link: ${await dir.read("links/nowhere.txt")}`);

    await dir.write("links/x.txt", "root-original");
    await dir.write("links/sub/x.txt", "sub-original");
    await dir.write("links/deepin/../x.txt", "changed");
    console.log(`Write after a link and parent traversal: ${await dir.read("links/sub/x.txt")}`);
    console.log(`Lexical sibling unchanged: ${await dir.read("links/x.txt")}`);
    console.log(`Two parents after a deep link: ${await dir.read("links/deepin/../../x.txt")}`);
    const traversed = await myco.files.requestReadWrite("./fixtures/tmp/links/deepin/../x.txt");
    await traversed.write("pinned target");
    console.log(`File grant after parent traversal: ${await dir.read("links/sub/x.txt")}`);
    await dir.remove("links/deepin/../unlink-me");
    console.log(`Unlink after parent traversal: ${!(await dir.list("links/sub")).some((f) => f.name === "unlink-me")}`);

    await dir.rmdir("links/rmdir_async");
    dir.sync.rmdir("links/rmdir_sync");
    console.log(`Async and sync rmdir unlink outward links: ${!(await dir.list("links")).some((f) => f.name.startsWith("rmdir_"))}`);

    await describe("Read through an escaping link", () => dir.read("links/escaping"));
    await describe("Write through an escaping link", () =>
        dir.write("links/escaping", "should not land"));
    await describe("Stat through an escaping link", () => dir.stat("links/escaping"));
    for (const suffix of ["/", "/.", "/.."] as const) {
        await describe(`Remove through an escaping link ending in ${suffix}`, () =>
            dir.remove(`links/out${suffix}`));
    }

    await describe("Write through a link whose target crosses one", () =>
        dir.write("links/via_out", "should not land"));
    const fixtures = await myco.files.requestReadDir("./fixtures");
    const outside = (await fixtures.list("/")).map((file) => file.name);
    console.log(`Nothing planted outside the token: ${!outside.includes("planted.txt")}`);

    await dir.write("links/token-file", "inside");
    const file = await myco.files.requestReadWrite("./fixtures/tmp/links/token-file");
    await dir.remove("links/token-file");
    const repoint = await sh.exec(["-c", "ln -s ../../test_script.sh fixtures/tmp/links/token-file"]);
    if (repoint.exit_code !== 0) throw new Error(repoint.stderr());
    await describe("Read through a repointed file token", () => file.read());
    await describe("Write through a repointed file token", () => file.write("should not land"));
    await describe("Stat through a repointed file token", () => file.stat());
    await describe("Sync read through a repointed file token", async () => file.sync.read());
    await describe("Sync write through a repointed file token", async () => file.sync.write("should not land"));
    await file.remove();

    await dir.remove("links/escaping");
    await dir.remove("links/broken");
    const left = (await dir.list("links")).map((file) => file.name).sort();
    console.log(`Left after cleanup: ${left.join(", ")}`);

    const target = await myco.files.requestRead("./fixtures/test_script.sh");
    console.log(`Escaping link's target survived: ${(await target.stat()) !== null}`);

    await dir.rmdirRecursive("links");
    console.log("Symlink handling test completed");
}
