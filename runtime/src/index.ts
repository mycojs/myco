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
            stat() {
                return core.opAsync("myco_op_stat_file", token);
            },
            sync: {
                read() {
                    return core.ops.myco_op_read_file_sync(token);
                },
                stat() {
                    return core.ops.myco_op_stat_file_sync(token);
                }
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
                    return core.ops.myco_op_write_file_sync(token, contents);
                },
                remove() {
                    return core.ops.myco_op_remove_file_sync(token);
                },
            },
        };
    },
    async requestReadWrite(path: string): Promise<Myco.Files.ReadWriteToken> {
        const readToken = await this.requestRead(path);
        const writeToken = await this.requestWrite(path);
        return {
            read: readToken.read,
            stat: readToken.stat,
            write: writeToken.write,
            remove: writeToken.remove,
            sync: {
                read: readToken.sync.read,
                stat: readToken.sync.stat,
                write: writeToken.sync.write,
                remove: writeToken.sync.remove,
            }
        };
    },
    async requestReadDir(path: string): Promise<Myco.Files.ReadDirToken> {
        const token = await core.opAsync("myco_op_request_read_dir", path);
        return {
            read(path: string) {
                return core.opAsync("myco_op_read_file", token, path);
            },
            stat(path: string) {
                return core.opAsync("myco_op_stat_file", token, path);
            },
            sync: {
                read(path: string) {
                    return core.ops.myco_op_read_file_sync(token, path);
                },
                stat(path: string) {
                    return core.ops.myco_op_stat_file_sync(token, path);
                }
            },
        };
    },
    async requestWriteDir(path: string): Promise<Myco.Files.WriteDirToken> {
        const token = await core.opAsync("myco_op_request_write_dir", path);
        return {
            write(path: string, contents: string) {
                return core.opAsync("myco_op_write_file", token, contents, path);
            },
            remove(path: string) {
                return core.opAsync("myco_op_remove_file", token, path);
            },
            mkdirp(path: string): Promise<void> {
                return core.opAsync("myco_op_mkdirp", token, path);
            },
            sync: {
                write(path: string, contents: string) {
                    return core.ops.myco_op_write_file_sync(token, contents, path);
                },
                remove(path: string) {
                    return core.ops.myco_op_remove_file_sync(token, path);
                },
                mkdirp(path: string) {
                    return core.ops.myco_op_mkdirp_sync(token, path);
                },
            },
        };
    },
    async requestReadWriteDir(path: string): Promise<Myco.Files.ReadWriteDirToken> {
        const readDirToken = await this.requestReadDir(path);
        const writeDirToken = await this.requestWriteDir(path);
        return {
            read: readDirToken.read,
            stat: readDirToken.stat,
            write: writeDirToken.write,
            remove: writeDirToken.remove,
            mkdirp: writeDirToken.mkdirp,
            sync: {
                read: readDirToken.sync.read,
                stat: readDirToken.sync.stat,
                write: writeDirToken.sync.write,
                remove: writeDirToken.sync.remove,
                mkdirp: writeDirToken.sync.mkdirp,
            }
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
