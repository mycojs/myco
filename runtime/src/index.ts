const {core} = Deno;

function argsToMessage(...args: any[]) {
    return args.map((arg) => JSON.stringify(arg)).join(" ");
}

const files: Myco.Files = {
    async requestRead(path: string): Promise<Myco.Files.ReadToken> {
        const token = await core.opAsync("myco_op_request_read_file", path);
        return {
            read() {
                return core.opAsync("myco_op_read_file", token);
            },
            sync: {
                read() {
                    return core.op("myco_op_read_file_sync", token);
                },
            },
        };
    },
    async requestWrite(path: string): Promise<Myco.Files.WriteToken> {
        const token = await core.opAsync("myco_op_request_write_file", path);
        return {
            write(contents: string) {
                return core.opAsync("myco_op_write_file", token, contents);
            },
            remove() {
                return core.opAsync("myco_op_remove_file", token);
            },
            sync: {
                write(contents: string) {
                    return core.op("myco_op_write_file_sync", token, contents);
                },
                remove() {
                    return core.op("myco_op_remove_file_sync", token);
                },
            },
        };
    },
    async requestReadWrite(path: string): Promise<Myco.Files.ReadWriteToken> {
        return {
            ...await this.requestReadDir(path),
            ...await this.requestWriteDir(path),
        }
    },
    async requestReadDir(path: string): Promise<Myco.Files.ReadDirToken> {
        const token = await core.opAsync("myco_op_request_read_dir", path);
        return {
            read(path: string) {
                return core.opAsync("myco_op_read_file_in_dir", token, path);
            },
            sync: {
                read(path: string) {
                    return core.op("myco_op_read_file_in_dir_sync", token, path);
                },
            },
        };
    },
    async requestWriteDir(path: string): Promise<Myco.Files.WriteDirToken> {
        const token = await core.opAsync("myco_op_request_write_dir", path);
        return {
            write(path: string, contents: string) {
                return core.opAsync("myco_op_write_file_in_dir", token, path, contents);
            },
            remove(path: string) {
                return core.opAsync("myco_op_remove_file_in_dir", token, path);
            },
            sync: {
                write(path: string, contents: string) {
                    return core.op("myco_op_write_file_in_dir_sync", token, path, contents);
                },
                remove(path: string) {
                    return core.op("myco_op_remove_file_in_dir_sync", token, path);
                },
            },
        };
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
        const token = await core.opAsync("myco_op_request_fetch_url", url);
        return core.opAsync("myco_op_fetch_url", token);
    },

    async fetch(url: string): Promise<string> {
        return core.opAsync("myco_op_fetch_url", url);
    }
}

const Myco: Myco = {
    console,
    files,
    http,

    setTimeout(callback: (value: any) => any, delay: number) {
        core.opAsync("myco_op_set_timeout", delay).then(callback);
    },
};

(globalThis as any).Myco = Myco;
(Error as any).prepareStackTrace = (core as any).prepareStackTrace;
