// Named by myco-local.toml, so it stays reachable.
import { allowed } from "@fixture/outside";

// Strip the specifier so only it differs between probes.
function reason(error: unknown, specifier: string): string {
    const message = error instanceof Error ? error.message : String(error);
    return message.split(`'${specifier}'`).join("<specifier>");
}

export default async function() {
    console.log("Testing module root confinement");

    try {
        const outside = await import("../outside_module_root/evil.ts");
        console.log(`LOADED FROM OUTSIDE THE PROJECT: ${outside.loot}`);
    } catch (error) {
        console.log(`Relative escape: ${reason(error, "../outside_module_root/evil.ts")}`);
    }

    // An absolute specifier is the same escape written differently.
    try {
        await import("/etc/passwd");
        console.log("LOADED /etc/passwd");
    } catch (error) {
        console.log(`Absolute escape: ${reason(error, "/etc/passwd")}`);
    }

    // Distinguishing these would make the rejection a filesystem probe.
    let present = "";
    let absent = "";
    try {
        await import("/etc/passwd");
    } catch (error) {
        present = reason(error, "/etc/passwd");
    }
    try {
        await import("/etc/definitely-not-here-xyz");
    } catch (error) {
        absent = reason(error, "/etc/definitely-not-here-xyz");
    }
    console.log(`Existing and missing paths are indistinguishable: ${present === absent}`);

    allowed();

    console.log("Module root confinement test completed");
}
