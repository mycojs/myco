import { CliArgs, parseArgs } from "./CliArgs.ts";
import { findMycoBinary, findTestSuites, listTests } from "./finders.ts";
import { TestReporter } from "./TestReporter.ts";
import { TestResult, TestRunner } from "./TestRunner.ts";

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
