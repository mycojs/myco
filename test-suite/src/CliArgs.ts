export interface CliArgs {
    command: 'run' | 'list';
    verbose: boolean;
    category?: string;
    suite?: string;
    mycoBinary?: string;
    timeout: number;
}

export function parseArgs(args: string[]): CliArgs {
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

