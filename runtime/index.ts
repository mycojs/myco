const {core} = Deno;

function argsToMessage(...args: any[]) {
    return args.map((arg) => JSON.stringify(arg)).join(" ");
}

const files: Myco.Files = {
    async requestRead(path: string): Promise<FileReadToken> {
        const token = await core.opAsync("op_request_read_file", path);
        return {
            read() {
                return core.opAsync("op_read_file", token);
            }
        }
    },
    async requestWrite(path: string): Promise<FileWriteToken> {
        const token = await core.opAsync("op_request_write_file", path);
        return {
            write(contents: string) {
                return core.opAsync("op_write_file", token, contents);
            },
            remove() {
                return core.opAsync("op_remove_file", token);
            },
        }
    },
    async requestReadWrite(path: string): Promise<FileReadWriteToken> {
        const readToken = await core.opAsync("op_request_read_file", path);
        const writeToken = await core.opAsync("op_request_write_file", path);
        return {
            read() {
                return core.opAsync("op_read_file", readToken);
            },
            write(content: string) {
                return core.opAsync("op_write_file", writeToken, content);
            },
            remove() {
                return core.opAsync("op_remove_file", writeToken);
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

const http: Myco.Http = {
    async request_fetch(url: string): Promise<string> {
        const token = await core.opAsync("op_request_fetch_url", url);
        return core.opAsync("op_fetch_url", token);
    },

    async fetch(url: string): Promise<string> {
        return core.opAsync("op_fetch_url", url);
    }
}

const Myco: Myco = {
    console,
    files,
    http,

    setTimeout(callback: (value: any) => any, delay: number) {
        core.opAsync("op_set_timeout", delay).then(callback);
    },
};

(globalThis as any).Myco = Myco;
