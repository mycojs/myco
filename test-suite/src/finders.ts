import { CliArgs } from "./CliArgs.ts";
import { TestManifest } from "./TestManifest.ts";

// The binary under test, paired with the absolute path it was resolved from. The path is
// carried alongside the token because an ExecToken does not expose where it came from, and
// the run output has to be able to name the binary it actually exercised.
export interface MycoBinary {
    token: Myco.Files.ExecToken;
    path: string;
}

// Resolves a possibly-relative path against the current working directory and collapses
// `.` and `..` segments, so the reported path is unambiguous. Relative paths such as
// `../target/debug/myco` otherwise only make sense next to the cwd they were resolved in.
function toAbsolutePath(path: string, cwd: string): string {
    const combined = path.startsWith('/') ? path : `${cwd}/${path}`;
    const resolved: string[] = [];
    for (const segment of combined.split('/')) {
        if (segment === '' || segment === '.') {
            continue;
        }
        if (segment === '..') {
            // A leading `..` cannot be collapsed any further; keep it rather than
            // silently walking above the root.
            if (resolved.length > 0 && resolved[resolved.length - 1] !== '..') {
                resolved.pop();
                continue;
            }
        }
        resolved.push(segment);
    }
    return `/${resolved.join('/')}`;
}

export async function findMycoBinary(cliArgs: CliArgs, myco: Myco): Promise<MycoBinary> {
    const cwd = myco.files.cwd();

    // Explicit override wins over the search below. Without it the harness always tests
    // ../target/debug/myco regardless of which binary was used to launch the harness, so
    // e.g. running the suite with a musl build would silently still exercise the glibc build.
    // Failing loudly here is deliberate: a silent fallback would report results for the
    // wrong binary.
    if (cliArgs.mycoBinary !== undefined) {
        try {
            const token = await myco.files.requestRead(cliArgs.mycoBinary);
            await token.stat();
        } catch (e) {
            // Report the underlying cause: the path may exist but be unreadable, or the
            // read capability may have been denied, and calling that "no such file" misleads.
            throw new Error(`--myco-binary was set to '${cliArgs.mycoBinary}', but it could not be read: ${e}`);
        }
        return {
            token: await myco.files.requestExec(cliArgs.mycoBinary),
            path: toAbsolutePath(cliArgs.mycoBinary, cwd),
        };
    }

    // First try to find in target/debug
    const debugPath = "../target/debug/myco";
    try {
        const token = await myco.files.requestRead(debugPath);
        await token.stat();
        return {
            token: await myco.files.requestExec(debugPath),
            path: toAbsolutePath(debugPath, cwd),
        };
    } catch (e) {
        // Not found, continue
    }

    // Try target/release
    const releasePath = "../target/release/myco";
    try {
        const token = await myco.files.requestRead(releasePath);
        await token.stat();
        return {
            token: await myco.files.requestExec(releasePath),
            path: toAbsolutePath(releasePath, cwd),
        };
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
