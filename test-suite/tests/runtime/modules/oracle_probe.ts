// Refusals must not vary with what is on the other side of the symlink.
function reason(error: unknown, specifier: string): string {
    const message = error instanceof Error ? error.message : String(error);
    return message.split(specifier).join("<specifier>");
}

async function probe(specifier: string): Promise<string> {
    try {
        await import(specifier);
        return "LOADED";
    } catch (error) {
        return reason(error, specifier);
    }
}

export default async function() {
    console.log("Testing symlink escape and oracle");

    const existing = await probe("./escape_link/evil.ts");
    const missing = await probe("./escape_link/definitely-not-here-xyz.ts");
    const directory = await probe("./escape_link/plaindir");

    console.log(`Existing file rejected: ${existing !== "LOADED"}`);
    console.log(`Missing path rejected: ${missing !== "LOADED"}`);
    console.log(`Directory rejected: ${directory !== "LOADED"}`);
    console.log(`All three indistinguishable: ${existing === missing && missing === directory}`);

    // Above, an ancestor decides it; a symlinked file must be judged from the link.
    const liveFile = await probe("./live_file_link.ts");
    const deadFile = await probe("./dead_file_link.ts");
    console.log(`Live file symlink rejected: ${liveFile !== "LOADED"}`);
    console.log(`Dangling file symlink rejected: ${deadFile !== "LOADED"}`);
    console.log(`Both file symlinks indistinguishable: ${liveFile === deadFile}`);

    // Only the check after index resolution catches this.
    const indexed = await probe("./indexed_dir");
    console.log(`Symlinked index rejected: ${indexed !== "LOADED"}`);

    console.log("Symlink escape and oracle test completed");
}
