import { TestResult } from "./TestRunner.ts";
import { indent } from "./stringUtils.ts";

export class TestReporter {
    private failedTests: Array<TestResult & { type: 'failed'; }> = [];

    constructor(private verbose: boolean) { }

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
                    console.log(`  Stdout:\n${indent(result.output.stdout, 4)}`);
                    console.log(`  Stderr:\n${indent(result.output.stderr, 4)}`);
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
