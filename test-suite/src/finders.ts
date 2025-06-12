import { CliArgs } from "./CliArgs.ts";
import { TestManifest } from "./TestManifest.ts";

export async function findMycoBinary(myco: Myco): Promise<Myco.Files.ExecToken> {
    // First try to find in target/debug
    const debugPath = "../target/debug/myco";
    try {
        const token = await myco.files.requestRead(debugPath);
        await token.stat();
        return await myco.files.requestExec(debugPath);
    } catch (e) {
        // Not found, continue
    }

    // Try target/release
    const releasePath = "../target/release/myco";
    try {
        const token = await myco.files.requestRead(releasePath);
        await token.stat();
        return await myco.files.requestExec(releasePath);
    } catch (e) {
        // Not found, continue
    }

    throw new Error("Could not find myco binary. Please build with 'cargo build' first.");
}

export async function findTestSuites(cliArgs: CliArgs, myco: Myco): Promise<string[]> {
    const testDir = "tests";
    const suites: string[] = [];

    async function walkDirectory(dir: string): Promise<void> {
        try {
            const dirToken = await myco.files.requestReadDir(dir);
            const entries = await dirToken.list(".");

            for (const entry of entries) {
                const entryPath = `${dir}/${entry.name}`;

                if (entry.name === "test.toml") {
                    // Found a test manifest, this is a test suite
                    const suitePath = dir;

                    // Apply filters
                    if (cliArgs.category) {
                        const suiteRelative = suitePath.replace(`${testDir}/`, '');
                        if (!suiteRelative.startsWith(cliArgs.category)) {
                            continue;
                        }
                    }

                    if (cliArgs.suite) {
                        const suiteRelative = suitePath.replace(`${testDir}/`, '');
                        if (suiteRelative !== cliArgs.suite) {
                            continue;
                        }
                    }

                    suites.push(suitePath);
                } else if (entry.stats.is_dir) {
                    await walkDirectory(entryPath);
                }
            }
        } catch (e) {
            // Directory doesn't exist or can't be read
        }
    }

    await walkDirectory(testDir);
    return suites;
}

export async function listTests(cliArgs: CliArgs, myco: Myco): Promise<void> {
    const testSuites = await findTestSuites(cliArgs, myco);

    for (const suitePath of testSuites) {
        const manifestPath = `${suitePath}/test.toml`;
        try {
            const token = await myco.files.requestRead(manifestPath);
            const manifestContent = await token.read();
            const manifest: TestManifest = TOML.parse(manifestContent);

            console.log(`Suite: ${manifest.name} (${suitePath})`);
            console.log(`  Description: ${manifest.description}`);
            for (const testCase of manifest.tests) {
                console.log(`  - ${testCase.name}`);
            }
            console.log();
        } catch (e) {
            continue;
        }
    }
}
