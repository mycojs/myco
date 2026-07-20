export interface CliArgs {
    command: 'run' | 'list';
    verbose: boolean;
    category?: string;
    suite?: string;
    mycoBinary?: string;
    timeout: number;
}

// Validates the value given for a flag, whether it came from the space form
// (`--myco-binary /path`) or the equals form (`--myco-binary=/path`). A flag
// that is present but has no usable value (a trailing `--myco-binary`, or an
// empty `--myco-binary=`) is a hard error rather than a silent default: for
// `--myco-binary` in particular, falling back would report results for a
// different binary than the one the caller asked to test.
function requireValue(value: string | undefined, flag: string): string {
    if (value === undefined || value === '') {
        throw new Error(`${flag} requires a value, but none was given.`);
    }
    return value;
}

export function parseArgs(args: string[]): CliArgs {
    const cliArgs: CliArgs = {
        command: 'run',
        verbose: false,
        timeout: 10000
    };

    for (let i = 0; i < args.length; i++) {
        const arg = args[i];

        // Support the GNU `--flag=value` form by splitting on the first `=`.
        // Only arguments that look like flags are split, so a positional
        // argument containing `=` is left alone.
        let name = arg;
        let inlineValue: string | undefined;
        if (arg.startsWith('-')) {
            const equals = arg.indexOf('=');
            if (equals !== -1) {
                name = arg.slice(0, equals);
                inlineValue = arg.slice(equals + 1);
            }
        }

        // Takes the inline value when the equals form was used, otherwise the
        // following argument.
        const takeValue = () => inlineValue !== undefined ? inlineValue : args[++i];

        // Boolean flags take no value, so an inline one was silently discarded --
        // `--verbose=false` used to set verbose to true, the opposite of the request.
        // Rejecting is deliberate over parsing true/false: the flag has no value form
        // to be right about.
        const rejectValue = () => {
            if (inlineValue !== undefined) {
                throw new Error(`${name} does not take a value, but got "${inlineValue}".`);
            }
        };

        switch (name) {
            case 'list':
                cliArgs.command = 'list';
                break;
            case 'run':
                cliArgs.command = 'run';
                break;
            case '-v':
            case '--verbose':
                rejectValue();
                cliArgs.verbose = true;
                break;
            case '-c':
            case '--category':
                cliArgs.category = requireValue(takeValue(), name);
                break;
            case '-s':
            case '--suite':
                cliArgs.suite = requireValue(takeValue(), name);
                break;
            case '--myco-binary':
                cliArgs.mycoBinary = requireValue(takeValue(), name);
                break;
            case '--timeout': {
                // A non-numeric or non-positive timeout is an error rather than
                // a silent fall back to the default.
                const raw = requireValue(takeValue(), name);
                const timeout = Number(raw);
                if (!Number.isInteger(timeout) || timeout <= 0) {
                    throw new Error(`${name} requires a positive whole number of milliseconds, but got "${raw}".`);
                }
                cliArgs.timeout = timeout;
                break;
            }
            default:
                // Anything flag-shaped that got this far is a typo. Silently
                // ignoring it is how a wrong `--myco-binary` spelling used to
                // produce a confident pass against the wrong binary.
                if (arg.startsWith('-')) {
                    throw new Error(`Unrecognised argument "${arg}".`);
                }
                break;
        }
    }

    return cliArgs;
}

