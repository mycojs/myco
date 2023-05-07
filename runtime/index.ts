const {core} = Deno;

function argsToMessage(...args: any[]) {
    return args.map((arg) => JSON.stringify(arg)).join(" ");
}

const files: Myco.Files = {
    async requestRead(path: string): Promise<FileReadToken> {
        // TODO: Validate the permission first
        return {
            read() {
                return core.opAsync("op_read_file", path);
            }
        }
    },
    async requestWrite(path: string): Promise<FileWriteToken> {
        // TODO: Validate the permission first
        return {
            write(contents: string) {
                return core.opAsync("op_write_file", path, contents);
            },
            remove() {
                return core.opAsync("op_remove_file", path);
            },
        }
    },
    async requestReadWrite(path: string): Promise<FileReadWriteToken> {
        // TODO: Validate the permission first
        return {
            read() {
                return core.opAsync("op_read_file", path);
            },
            write(content: string) {
                return core.opAsync("op_write_file", path, content);
            },
            remove() {
                return core.opAsync("op_remove_file", path);
            },
        }
    },
}

const console: Myco.Console = {
    log(...args: any[]) {
        core.print(`[out]: ${argsToMessage(...args)}\n`, false);
    },

    error(...args: any[]) {
        core.print(`[err]: ${argsToMessage(...args)}\n`, true);
    },
}

const Myco: Myco = {
    console,
    files,

    fetch(url: string) {
        return core.opAsync("op_fetch", url);
    },

    setTimeout(callback: (value: any) => any, delay: number) {
        core.opAsync("op_set_timeout", delay).then(callback);
    },
};

(globalThis as any).Myco = Myco;
