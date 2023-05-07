const {core} = Deno;

function argsToMessage(...args: any[]) {
    return args.map((arg) => JSON.stringify(arg)).join(" ");
}

const files: Myco.Files = {
    async requestRead(path: string): Promise<Myco.Files.ReadToken> {
        const token = await core.opAsync("op_request_read_file", path);
        return {
            read() {
                return core.opAsync("op_read_file", token);
            }
        }
    },
    async requestWrite(path: string): Promise<Myco.Files.WriteToken> {
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
    async requestReadWrite(path: string): Promise<Myco.Files.ReadWriteToken> {
        return {
            ...await this.requestReadDir(path),
            ...await this.requestWriteDir(path),
        }
    },
    async requestReadDir(path: string): Promise<Myco.Files.ReadDirToken> {
        const token = await core.opAsync("op_request_read_dir", path);
        return {
            read(path: string) {
                return core.opAsync("op_read_file_in_dir", token, path);
            }
        }
    },
    async requestWriteDir(path: string): Promise<Myco.Files.WriteDirToken> {
        const token = await core.opAsync("op_request_write_dir", path);
        return {
            write(path: string, contents: string) {
                return core.opAsync("op_write_file_in_dir", token, path, contents);
            },
            remove(path: string) {
                return core.opAsync("op_remove_file_in_dir", token, path);
            },
        }
    },
    async requestReadWriteDir(path: string): Promise<Myco.Files.ReadWriteDirToken> {
        return {
            ...await this.requestReadDir(path),
            ...await this.requestWriteDir(path),
        }
    }
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
