import { matchesExpectation, testCaseToOutputExpectation } from "./expectations.ts";
import { TestManifest, TestCase, TestMeta } from "./TestManifest.ts";
import { TestReporter } from "./TestReporter.ts";

export class TestRunner {
    constructor(private mycoBinary: Myco.Files.ExecToken, private myco: Myco) { }

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

        // Clean up fixtures/tmp directories before each test
        await this.cleanupFixtures(testDir);

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
                resolve({
                    type: 'timeout', duration, testCase: {
                        suite: testDir,
                        name: testCase.name
                    }
                });
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
    }

    private async cleanupFixtures(testDir: string): Promise<void> {
        try {
            // Look for fixtures/tmp directories in the test directory
            const testDirToken = await this.myco.files.requestReadWriteDir(testDir);
            
            try {
                await testDirToken.rmdirRecursive("./fixtures/tmp");
            } catch (e) {
                // tmp directory doesn't exist, which is fine
            }
                
            // Recreate the tmp directory
            await testDirToken.mkdirp("./fixtures/tmp");
        } catch (e) {
            throw new Error(`Warning: Failed to cleanup fixtures for test directory ${testDir}: ${e}`);
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
                return {
                    type: 'passed', duration, testCase: {
                        suite: testDir,
                        name: testCase.name
                    }
                };
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

export type TestResult = 
    | { testCase: TestMeta; type: 'passed'; duration: number; }
    | { testCase: TestMeta; type: 'failed'; reason: string; brief_reason: string; output: TestOutput; } 
    | { testCase: TestMeta; type: 'timeout'; duration: number; } 
    | { testCase: TestMeta; type: 'error'; error: string; };

export interface TestOutput {
    stdout: string;
    stderr: string;
    exit_code: number;
    duration: number;
}
