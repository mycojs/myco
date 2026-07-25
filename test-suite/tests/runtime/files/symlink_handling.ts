// A directory token has to deal with three kinds of symlink inside its own
// directory: one that resolves inside it, one that dangles, and one that points
// out. Only the third is a confinement question, and only when the operation
// follows it -- `remove` unlinks the link itself and touches nothing at the far
// end, so it is ordinary cleanup.

// The distinction the whole test turns on: a path refused by the token's
// confinement, versus one it let through that simply is not there.
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

    // There is no symlink capability, so the fixture is built with `ln`. From
    // fixtures/tmp/links, `../../test_script.sh` lands outside the token's
    // directory, which is what makes it the escaping case.
    const sh = await myco.files.requestExec("sh");
    const build = await sh.exec([
        "-c",
        "cd fixtures/tmp/links && printf inside > target.txt" +
            " && ln -s target.txt live" +
            " && ln -s nowhere.txt broken" +
            " && ln -s ../../test_script.sh escaping" +
            // `out` leaves the token's directory, and `via_out` names a target
            // that spells a path inside it but is reached through `out`. The
            // target does not exist, so nothing on the way resolves.
            " && ln -s ../.. out" +
            " && ln -s out/planted.txt via_out",
    ]);
    if (build.exit_code !== 0) {
        throw new Error(`Failed to build fixture: ${build.stderr()}`);
    }

    // A link that resolves inside is followed as usual.
    console.log(`Read through a live link: ${await dir.read("links/live")}`);

    // A link that dangles inside the directory is not an escape: where it
    // points is in scope, it is just not there yet. So these reach the
    // filesystem and answer for the target, rather than being refused.
    await describe("Stat of a dangling link", () => dir.stat("links/broken"));
    await dir.write("links/broken", "written through");
    console.log(`Write through a dangling link: ${await dir.read("links/nowhere.txt")}`);

    // Following a link out of the directory is refused, whichever way it is
    // followed. The far end exists, so absence cannot be what refuses these.
    await describe("Read through an escaping link", () => dir.read("links/escaping"));
    await describe("Write through an escaping link", () =>
        dir.write("links/escaping", "should not land"));
    await describe("Stat through an escaping link", () => dir.stat("links/escaping"));

    // A target is judged by where it lands, not by the path it spells: writing
    // through this one would create a file outside the token's directory, at a
    // name the caller chose.
    await describe("Write through a link whose target crosses one", () =>
        dir.write("links/via_out", "should not land"));
    const fixtures = await myco.files.requestReadDir("./fixtures");
    const outside = (await fixtures.list("/")).map((file) => file.name);
    console.log(`Nothing planted outside the token: ${!outside.includes("planted.txt")}`);

    // Unlinking one is not following it. Both of these are removals a
    // WriteDir holder has to be able to make to tidy its own directory.
    await dir.remove("links/escaping");
    await dir.remove("links/broken");
    const left = (await dir.list("links")).map((file) => file.name).sort();
    console.log(`Left after cleanup: ${left.join(", ")}`);

    // What the escaping link pointed at is still there: `remove` unlinked the
    // link, not its target.
    const target = await myco.files.requestRead("./fixtures/test_script.sh");
    console.log(`Escaping link's target survived: ${(await target.stat()) !== null}`);

    await dir.rmdirRecursive("links");
    console.log("Symlink handling test completed");
}
