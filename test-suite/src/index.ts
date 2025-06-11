export default async function(myco: Myco): Promise<number> {
    const args = myco.argv.slice(3); // Skip program, 'run', and script name
    
    // Parse command line arguments
    const cliArgs = parseArgs(args);
    
    // Find myco binary
    const mycoBinary = await findMycoBinary(myco);
    
    // Handle commands
    if (cliArgs.command === 'list') {
        await listTests(cliArgs, myco);
        return 0;
    }
    
    // Default to run tests
    return await runTests(cliArgs, mycoBinary, myco);
}

interface CliArgs {
    command: 'run' | 'list';
    verbose: boolean;
    category?: string;
    suite?: string;
    mycoBinary?: string;
    timeout: number;
}

interface TestManifest {
    name: string;
    description: string;
    tests: TestCase[];
}

interface TestMeta {
    suite: string;
    name: string;
}

interface TestCase {
    name: string;
    script: string;
    args?: string[];
    working_directory?: string;
    environment_variables?: Record<string, string>;
    timeout_ms?: number;
    expected_stdout?: string;
    expected_stderr?: string;
    expected_exit_code?: number;
}

interface TestOutput {
    stdout: string;
    stderr: string;
    exit_code: number;
    duration: number;
}

type TestResult = 
    | { testCase: TestMeta, type: 'passed'; duration: number }
    | { testCase: TestMeta, type: 'failed'; reason: string; brief_reason: string; output: TestOutput }
    | { testCase: TestMeta, type: 'timeout'; duration: number }
    | { testCase: TestMeta, type: 'error'; error: string };

type StreamExpectation = 
    | { type: 'glob'; pattern: string }
    | { type: 'none' };

type OutputExpectation = {
    stdout: StreamExpectation;
    stderr: StreamExpectation;
    exit_code: number;
};

function parseArgs(args: string[]): CliArgs {
    const cliArgs: CliArgs = {
        command: 'run',
        verbose: false,
        timeout: 10000
    };
    
    for (let i = 0; i < args.length; i++) {
        const arg = args[i];
        switch (arg) {
            case 'list':
                cliArgs.command = 'list';
                break;
            case 'run':
                cliArgs.command = 'run';
                break;
            case '-v':
            case '--verbose':
                cliArgs.verbose = true;
                break;
            case '-c':
            case '--category':
                cliArgs.category = args[++i];
                break;
            case '-s':
            case '--suite':
                cliArgs.suite = args[++i];
                break;
            case '--myco-binary':
                cliArgs.mycoBinary = args[++i];
                break;
            case '--timeout':
                cliArgs.timeout = parseInt(args[++i]) || 10000;
                break;
        }
    }
    
    return cliArgs;
}

async function findMycoBinary(myco: Myco): Promise<Myco.Files.ExecToken> {
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

async function findTestSuites(cliArgs: CliArgs, myco: Myco): Promise<string[]> {
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

async function listTests(cliArgs: CliArgs, myco: Myco): Promise<void> {
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

async function runTests(cliArgs: CliArgs, mycoBinary: Myco.Files.ExecToken, myco: Myco): Promise<number> {
    const testSuites = await findTestSuites(cliArgs, myco);
    
    if (testSuites.length === 0) {
        console.log("No test suites found.");
        return 0;
    }
    
    const reporter = new TestReporter(cliArgs.verbose);
    const runner = new TestRunner(mycoBinary, myco);
    let allResults: Array<TestResult> = [];
    
    for (const suitePath of testSuites) {
        const separator = "=".repeat(60);
        console.log(separator);
        console.log(`Test suite: ${suitePath}`);
        console.log(separator);
        
        const results = await runner.runTestSuite(suitePath, reporter);
        
        allResults.push(...results);
        console.log();
    }
    
    reporter.reportSuiteSummary(allResults);
    
    // Exit with non-zero code if any tests failed
    const hasFailures = allResults.some((result) => result.type !== 'passed');
    if (hasFailures) {
        return 1;
    }
    return 0;
}

class TestRunner {
    constructor(private mycoBinary: Myco.Files.ExecToken, private myco: Myco) {}
    
    async runTestSuite(suitePath: string, reporter: TestReporter): Promise<Array<TestResult>> {
        const manifestPath = `${suitePath}/test.toml`;
        
        try {
            const token = await this.myco.files.requestRead(manifestPath);
            const manifestContent = await token.read();
            const manifest: TestManifest = TOML.parse(manifestContent);
            
            console.log(`Running test suite: ${manifest.name}`);
            console.log(`Description: ${manifest.description}`);
            console.log(`Tests: ${manifest.tests.length}`);
            console.log();
            
            const results: Array<TestResult> = [];
            
            for (const testCase of manifest.tests) {
                const result = await this.runTestCase(testCase, suitePath);
                results.push(result);
                reporter.reportTestResult(result);
            }
            
            return results;
        } catch (e) {
            return [{
                type: 'error',
                error: `Failed to load test manifest: ${e}`,
                testCase: {
                    suite: suitePath,
                    name: "test_manifest_error"
                }
            }];
        }
    }
    
    async runTestCase(testCase: TestCase, testDir: string): Promise<TestResult> {
        const startTime = Date.now();
        
        // Construct script path
        const scriptPath = `${testDir}/${testCase.script}`;
        try {
            const token = await this.myco.files.requestRead(scriptPath);
            await token.stat();
        } catch (e) {
            return {
                type: 'error',
                error: `Test script not found: ${scriptPath}`,
                testCase: {
                    suite: testDir,
                    name: testCase.name
                }
            };
        }
        
        // Ensure myco.toml exists in the test directory
        const mycoTomlPath = `${testDir}/myco.toml`;
        let createdToml = false;
        try {
            const token = await this.myco.files.requestRead(mycoTomlPath);
            await token.stat();
        } catch (e) {
            // Create a minimal myco.toml temporarily
            const minimalToml = `[project]
name = "test"
version = "0.1.0"
`;
            try {
                const writeToken = await this.myco.files.requestWrite(mycoTomlPath);
                await writeToken.write(minimalToml);
                createdToml = true;
            } catch (writeErr) {
                return {
                    type: 'error',
                    error: `Failed to create myco.toml in test directory: ${writeErr}`,
                    testCase: {
                        suite: testDir,
                        name: testCase.name
                    }
                };
            }
        }
        
        try {
            // Build command arguments - use full relative path for the script
            const scriptRelativePath = scriptPath;
            const args = ["run", scriptRelativePath, ...(testCase.args || [])];
            
            // Execute with timeout
            const testTimeout = testCase.timeout_ms || 5000;
            let timeoutId: number | null = null;
            let timedOut = false;
            
            // Set up timeout
            const timeoutPromise = new Promise<TestResult>((resolve) => {
                timeoutId = this.myco.setTimeout(() => {
                    timedOut = true;
                    const duration = Date.now() - startTime;
                    resolve({ type: 'timeout', duration, testCase: {
                        suite: testDir,
                        name: testCase.name
                    }})
                }, testTimeout);
            });
            
            // Execute the test
            const execPromise = this.executeTest(testDir, args, testCase);
            
            const result = await Promise.race([execPromise, timeoutPromise]);
            
            // Clear timeout if it's still pending
            if (timeoutId !== null) {
                this.myco.clearTimeout(timeoutId);
            }
            
            return result;
        } finally {
            // Clean up the created myco.toml if we created it
            if (createdToml) {
                try {
                    const writeToken = await this.myco.files.requestWrite(mycoTomlPath);
                    await writeToken.remove();
                } catch (e) {
                    // Ignore cleanup errors
                }
            }
        }
    }
    
    private async executeTest(testDir: string, args: string[], testCase: TestCase): Promise<TestResult> {
        const startTime = Date.now();
        
        try {
            const result = await this.mycoBinary.exec(args);
            
            const duration = Date.now() - startTime;
            const testOutput: TestOutput = {
                stdout: result.stdout(),
                stderr: result.stderr(),
                exit_code: result.exit_code,
                duration
            };
            
            // Validate output
            const expectation = testCaseToOutputExpectation(testCase);
            const matchResult = matchesExpectation(testOutput, expectation);
            
            if (matchResult.success) {
                return { type: 'passed', duration, testCase: {
                    suite: testDir,
                    name: testCase.name
                } };
            } else {
                return {
                    type: 'failed',
                    reason: matchResult.reason!,
                    brief_reason: matchResult.brief_reason!,
                    output: testOutput,
                    testCase: {
                        suite: testDir,
                        name: testCase.name
                    }
                };
            }
        } catch (e: any) {
            return {
                type: 'error',
                error: `Failed to execute command: ${e}`,
                testCase: {
                    suite: testDir,
                    name: testCase.name
                }
            };
        }
    }
}

function testCaseToOutputExpectation(testCase: TestCase): OutputExpectation {
    // Determine stdout expectation
    let stdoutExpectation: StreamExpectation;
    if (testCase.expected_stdout !== undefined) {
        stdoutExpectation = { type: 'glob', pattern: testCase.expected_stdout };
    } else {
        stdoutExpectation = { type: 'none' };
    }
    
    // Determine stderr expectation
    let stderrExpectation: StreamExpectation;
    if (testCase.expected_stderr !== undefined) {
        stderrExpectation = { type: 'glob', pattern: testCase.expected_stderr };
    } else {
        stderrExpectation = { type: 'none' };
    }
    
    return {
        stdout: stdoutExpectation,
        stderr: stderrExpectation,
        exit_code: testCase.expected_exit_code || 0
    };
}

function matchesExpectation(output: TestOutput, expectation: OutputExpectation): { success: boolean; reason?: string; brief_reason?: string } {    
    // Check stderr expectation
    const stderrResult = matchesStreamExpectation(output.stderr, expectation.stderr, 'stderr');
    if (!stderrResult.success) {
        return stderrResult;
    }
    
    // Check stdout expectation
    const stdoutResult = matchesStreamExpectation(output.stdout, expectation.stdout, 'stdout');
    if (!stdoutResult.success) {
        return stdoutResult;
    }

    // Check exit code
    if (output.exit_code !== expectation.exit_code) {
        return {
            success: false,
            reason: `Exit code mismatch: expected ${expectation.exit_code}, got ${output.exit_code}`,
            brief_reason: 'exit code mismatch'
        };
    }
    
    return { success: true };
}

function indent(text: string, indent: number): string {
    return text.split('\n').map(line => ' '.repeat(indent) + line).join('\n');
}

function globToRegex(pattern: string): RegExp {
    let result = '';
    let i = 0;
    
    while (i < pattern.length) {
        const char = pattern[i];
        
        if (char === '\\' && i + 1 < pattern.length) {
            // Handle escaped characters
            const nextChar = pattern[i + 1];
            if (nextChar === '*' || nextChar === '?') {
                // Escape the literal character
                result += '\\' + nextChar;
                i += 2;
            } else {
                // Regular escape - escape the backslash and the character
                result += '\\\\' + nextChar.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                i += 2;
            }
        } else if (char === '*') {
            // Wildcard - match 0 or more characters
            result += '[^\\n]*';
            i++;
        } else if (char === '?') {
            // Single character wildcard
            result += '[^\\n]';
            i++;
        } else {
            // Regular character - escape special regex characters
            result += char.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
            i++;
        }
    }
    
    return new RegExp('^' + result + '$', 's'); // 's' flag for dotall mode
}

function matchesStreamExpectation(actualOutput: string, expectation: StreamExpectation, streamName: string): { success: boolean; reason?: string } {
    switch (expectation.type) {
        case 'glob':
            const regex = globToRegex(expectation.pattern);
            if (!regex.test(actualOutput)) {
                return {
                    success: false,
                    reason: `${streamName} mismatch:\n    Expected:\n${indent(expectation.pattern, 8)}\n    Actual:\n${indent(actualOutput, 8)}`
                    brief_reason: `${streamName} mismatch`
                };
            }
            return { success: true };
            
        case 'none':
            // No expectation specified, always pass
            return { success: true };
    }
}

class TestReporter {
    private failedTests: Array<TestResult & { type: 'failed' }> = [];
    
    constructor(private verbose: boolean) {}
    
    reportTestResult(result: TestResult): void {
        switch (result.type) {
            case 'passed':
                console.log(`  ✓ ${result.testCase.name} (${result.duration}ms)`);
                break;
            case 'failed':
                // Store failed test for detailed reporting later
                this.failedTests.push(result);
                
                // Show brief summary
                console.log(`  ✗ ${result.testCase.name}`);
                console.log(`    ! ${result.brief_reason}`);
                break;
            case 'timeout':
                console.log(`  ⏱ ${result.testCase.name} (timeout after ${result.duration}ms)`);
                break;
            case 'error':
                console.log(`  ! ${result.testCase.name} (error: ${result.error})`);
                break;
        }
    }
    
    reportSuiteSummary(results: Array<TestResult>): void {
        const total = results.length;
        const passed = results.filter((r) => r.type === 'passed').length;
        const failed = results.filter((r) => r.type === 'failed').length;
        const timeout = results.filter((r) => r.type === 'timeout').length;
        const error = results.filter((r) => r.type === 'error').length;
        
        console.log();
        
        // Show detailed failure information at the end
        if (this.failedTests.length > 0) {
            console.log();
            console.log("Failed Test Details:");
            console.log("=".repeat(60));
            
            for (const result of this.failedTests) {
                console.log(`\n✗ ${result.testCase.suite} > ${result.testCase.name}`);
                if (this.verbose) {
                    console.log(`  Reason: ${result.reason}`);
                    console.log(`  Stdout: ${JSON.stringify(result.output.stdout)}`);
                    console.log(`  Stderr: ${JSON.stringify(result.output.stderr)}`);
                    console.log(`  Exit code: ${result.output.exit_code}`);
                    console.log(`  Duration: ${result.output.duration}ms`);
                } else {
                    const indentedReason = indent(result.reason, 2);
                    console.log(indentedReason);
                }
            }
        }
        
        console.log();
        console.log("=".repeat(60));
        console.log();
        console.log("Test Summary:");

        const totalDuration = results.reduce((sum, result) => {
            switch (result.type) {
                case 'passed':
                case 'timeout':
                    return sum + result.duration;
                case 'failed':
                    return sum + result.output.duration;
                case 'error':
                default:
                    return sum;
            }
        }, 0);
        
        console.log(`  Total duration: ${totalDuration}ms`);
        console.log(`  Total: ${total}`);
        console.log(`  ✓ Passed: ${passed}`);
        
        if (failed > 0) {
            console.log(`  ✗ Failed: ${failed}`);
        }
        if (timeout > 0) {
            console.log(`  ⏱ Timeout: ${timeout}`);
        }
        if (error > 0) {
            console.log(`  ! Error: ${error}`);
        }
    }
} 