import { generateGlobDiff, globToRegex } from "./glob.ts";
import { TestCase } from "./TestManifest.ts";
import { TestOutput } from "./TestRunner.ts";
import { indent } from "./stringUtils.ts";

export type StreamExpectation = 
    | { type: 'glob'; pattern: string }
    | { type: 'none' };

export type OutputExpectation = {
    stdout: StreamExpectation;
    stderr: StreamExpectation;
    exit_code: number;
};

export function testCaseToOutputExpectation(testCase: TestCase): OutputExpectation {
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

export function matchesExpectation(output: TestOutput, expectation: OutputExpectation): { success: boolean; reason?: string; brief_reason?: string; } {
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

export function matchesStreamExpectation(actualOutput: string, expectation: StreamExpectation, streamName: string): { success: boolean; reason?: string; brief_reason?: string; } {
    switch (expectation.type) {
        case 'glob':
            const regex = globToRegex(expectation.pattern);
            if (!regex.test(actualOutput)) {
                const diff = generateGlobDiff(expectation.pattern, actualOutput);
                return {
                    success: false,
                    reason: `${streamName} mismatch:\n${indent(diff, 4)}`,
                    brief_reason: `${streamName} mismatch`
                };
            }
            return { success: true };

        case 'none':
            // No expectation specified, always pass
            return { success: true };
    }
}
