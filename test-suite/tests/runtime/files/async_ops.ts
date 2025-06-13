export default async function(myco: Myco) {
    const workingDir = await myco.files.requestExec("./fixtures/async_test_script.sh");

    const execStartTime = Date.now();
    const execPromise = workingDir.exec();
    const execEndTime = Date.now();
    const execDuration = execEndTime - execStartTime;
    if (execDuration > 90) {
        console.log(`Creating exec promise took longer than expected`);
    } else {
        console.log(`Creating exec promise was fast`);
    }
    
    const awaitExecStartTime = Date.now();
    await execPromise;
    const awaitExecEndTime = Date.now();
    const awaitExecDuration = awaitExecEndTime - awaitExecStartTime;
    if (awaitExecDuration > 90) {
        console.log(`Awaiting exec promise took the expected amount of time`);
    } else {
        console.log(`Awaiting exec promise took less time than expected`);
    }
}
