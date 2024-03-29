const {core} = Deno;

(function () {
    function maybeDecode<T extends 'utf-8' | 'raw'>(bytes: Uint8Array, encoding: 'utf-8' | 'raw' = 'utf-8'): T extends 'raw' ? Uint8Array : string {
        if (encoding === 'utf-8') {
            return new TextDecoder().decode(bytes) as any;
        } else {
            return bytes as any;
        }
    }

    function maybeEncode(contents: Uint8Array | string): Uint8Array {
        if (typeof contents === 'string') {
            return new TextEncoder().encode(contents);
        } else {
            return contents;
        }
    }

    function argsToMessage(...args: any[]) {
        return args.map((arg) => {
            if (typeof arg === 'string') {
                return arg;
            } else {
                return JSON.stringify(arg, null, 2);
            }
        }).join(" ");
    }

    function fileExtension(path: string) {
        return path.split(".").pop()?.toLowerCase();
    }

    class TextEncoder {
        constructor() {
        }

        encode(str: string): Uint8Array {
            return core.ops.myco_op_encode_utf8_sync(str);
        }
    }

    class TextDecoder {
        constructor(private label: 'utf-8' = 'utf-8') {
        }

        decode(bytes: Uint8Array): string {
            return core.ops.myco_op_decode_utf8_sync(bytes);
        }
    }

    function filterListDir(options: Myco.Files.ListDirOptions | undefined, list: Myco.Files.File[]) {
        const extensions = options?.extensions?.map((ext) => ext.toLowerCase());

        const matchesExtensions = (file: Myco.Files.File) => {
            if (extensions == undefined) {
                return true;
            } else {
                const ext = fileExtension(file.name);
                return ext !== undefined && extensions.includes(ext);
            }
        }

        const matchesStat = (file: Myco.Files.File) =>
            options?.include_dirs !== false || !file.stats.is_dir &&
            options?.include_files !== false || file.stats?.is_dir &&
            options?.include_symlinks !== false || !file.stats?.is_symlink;

        return list.filter((file) =>
            matchesExtensions(file) && matchesStat(file)
        )
    }

    const files: Myco.Files = {
        async requestRead(path: string): Promise<Myco.Files.ReadToken> {
            const token = await core.opAsync("myco_op_request_read_file", path);
            return {
                async read(encoding: 'utf-8' | 'raw' = 'utf-8'): Promise<any> {
                    const raw = await core.opAsync("myco_op_read_file", token);
                    return maybeDecode(raw, encoding);
                },
                stat() {
                    return core.opAsync("myco_op_stat_file", token);
                },
                sync: {
                    read(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                        const raw = core.ops.myco_op_read_file_sync(token);
                        return maybeDecode(raw, encoding);
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
                write(contents: string | Uint8Array) {
                    return core.opAsync("myco_op_write_file", token, maybeEncode(contents));
                },
                remove() {
                    return core.opAsync("myco_op_remove_file", token);
                },
                sync: {
                    write(contents: string | Uint8Array) {
                        return core.ops.myco_op_write_file_sync(token, maybeEncode(contents));
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
                ...readToken,
                ...writeToken,
                sync: {
                    ...readToken.sync,
                    ...writeToken.sync,
                }
            } as Myco.Files.ReadWriteToken;
        },
        async requestExec(path: string): Promise<Myco.Files.ExecToken> {
            const token = await core.opAsync("myco_op_request_exec_file", path);
            return {
                async exec(args: readonly string[] = []): Promise<Myco.Files.ExecResult> {
                    const result = await core.opAsync("myco_op_exec_file", token, undefined, args);
                    return {
                        exit_code: result.exit_code,
                        stdout(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                            return maybeDecode(result.stdout, encoding);
                        },
                        stderr(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                            return maybeDecode(result.stderr, encoding);
                        },
                    }
                },
                stat() {
                    return core.opAsync("myco_op_stat_file", token);
                },
                sync: {
                    exec(args: string[] = []): Myco.Files.ExecResult {
                        const result = core.ops.myco_op_exec_file_sync(token, undefined, args);
                        return {
                            exit_code: result.exit_code,
                            stdout(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                return maybeDecode(result.stdout, encoding);
                            },
                            stderr(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                return maybeDecode(result.stderr, encoding);
                            },
                        }
                    },
                    stat() {
                        return core.ops.myco_op_stat_file_sync(token);
                    }
                },
            };
        },
        async requestReadDir(path: string): Promise<Myco.Files.ReadDirToken> {
            const rootDir = await core.opAsync("myco_op_request_read_dir", path);
            const token: Myco.Files.ReadDirToken = {
                async read(path: string, encoding: 'utf-8' | 'raw' = 'utf-8'): Promise<any> {
                    const raw = await core.opAsync("myco_op_read_file", rootDir, path);
                    return maybeDecode(raw, encoding);
                },
                stat(path: string) {
                    return core.opAsync("myco_op_stat_file", rootDir, path);
                },
                async list(path: string, options) {
                    let list = await core.opAsync("myco_op_list_dir", rootDir, path);
                    if (options?.recursive) {
                        const subdirs = list.filter((file) => file.stats.is_dir);
                        for (const subdir of subdirs) {
                            const subPath = `${path}/${subdir.name}`;
                            const subFiles = (await this.list(subPath, options)).map((file) => ({
                                ...file,
                                name: `${subdir.name}/${file.name}`,
                            }));
                            list.push(...subFiles);
                        }
                    }
                    return filterListDir(options, list);
                },
                sync: {
                    read(path: string, encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                        const raw = core.ops.myco_op_read_file_sync(rootDir, path);
                        return maybeDecode(raw, encoding);
                    },
                    stat(path: string) {
                        return core.ops.myco_op_stat_file_sync(rootDir, path);
                    },
                    list(path: string, options) {
                        let list = core.ops.myco_op_list_dir_sync(rootDir, path);
                        if (options?.recursive) {
                            const subdirs = list.filter((file) => file.stats.is_dir);
                            for (const subdir of subdirs) {
                                const subPath = `${path}/${subdir.name}`;
                                const subFiles = this.list(subPath, options).map((file) => ({
                                    ...file,
                                    name: `${subdir.name}/${file.name}`,
                                }));
                                list.push(...subFiles);
                            }
                        }
                        return filterListDir(options, list);
                    },
                },
            };
            return token;
        },
        async requestWriteDir(path: string): Promise<Myco.Files.WriteDirToken> {
            const token = await core.opAsync("myco_op_request_write_dir", path);
            return {
                write(path: string, contents: string | Uint8Array) {
                    return core.opAsync("myco_op_write_file", token, maybeEncode(contents), path);
                },
                remove(path: string) {
                    return core.opAsync("myco_op_remove_file", token, path);
                },
                mkdirp(path: string): Promise<void> {
                    return core.opAsync("myco_op_mkdirp", token, path);
                },
                rmdir(path: string): Promise<void> {
                    return core.opAsync("myco_op_rmdir", token, path);
                },
                sync: {
                    write(path: string, contents: string | Uint8Array) {
                        return core.ops.myco_op_write_file_sync(token, maybeEncode(contents), path);
                    },
                    remove(path: string) {
                        return core.ops.myco_op_remove_file_sync(token, path);
                    },
                    mkdirp(path: string) {
                        return core.ops.myco_op_mkdirp_sync(token, path);
                    },
                    rmdir(path: string) {
                        return core.ops.myco_op_rmdir_sync(token, path);
                    },
                },
            };
        },
        async requestReadWriteDir(path: string): Promise<Myco.Files.ReadWriteDirToken> {
            const readDirToken = await this.requestReadDir(path);
            const writeDirToken = await this.requestWriteDir(path);
            return {
                ...readDirToken,
                ...writeDirToken,
                sync: {
                    ...readDirToken.sync,
                    ...writeDirToken.sync,
                }
            } as Myco.Files.ReadWriteDirToken;
        },
        async requestExecDir(path: string): Promise<Myco.Files.ExecDirToken> {
            const token = await core.opAsync("myco_op_request_exec_file", path);
            return {
                async exec(path: string, args: readonly string[] = []): Promise<Myco.Files.ExecResult> {
                    const result = await core.opAsync("myco_op_exec_file", token, path, args);
                    return {
                        exit_code: result.exit_code,
                        stdout(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                            return maybeDecode(result.stdout, encoding);
                        },
                        stderr(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                            return maybeDecode(result.stderr, encoding);
                        },
                    }
                },
                stat(path: string) {
                    return core.opAsync("myco_op_stat_file", token, path);
                },
                sync: {
                    exec(path: string, args: string[] = []): Myco.Files.ExecResult {
                        const result = core.ops.myco_op_exec_file_sync(token, path, args);
                        return {
                            exit_code: result.exit_code,
                            stdout(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                return maybeDecode(result.stdout, encoding);
                            },
                            stderr(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                return maybeDecode(result.stderr, encoding);
                            },
                        }
                    },
                    stat(path: string) {
                        return core.ops.myco_op_stat_file_sync(token, path);
                    }
                },
            };
        },
    }

    const http: Myco.Http = {
        async fetch(url: string, encoding: 'utf-8' | 'raw' = 'utf-8'): Promise<any> {
            const token = await core.opAsync("myco_op_request_fetch_url", url);
            const raw = await core.opAsync("myco_op_fetch_url", token);
            return maybeDecode(raw, encoding);
        }
    }

    const tcp: Myco.Tcp = {
        async bind(addr: string): Promise<Myco.Tcp.Server> {
            const listener = await core.opAsync("myco_op_bind_tcp_listener", addr);
            return {
                addr,
                async accept(): Promise<Myco.Tcp.Connection> {
                    const stream = await core.opAsync("myco_op_accept_tcp_stream", listener);
                    return {
                        async read(encoding: 'utf-8' | 'raw' = 'utf-8'): Promise<any> {
                            const raw = await core.opAsync("myco_op_read_all_tcp_stream", stream);
                            return maybeDecode(raw, encoding);
                        },
                        async write(data: string | Uint8Array): Promise<void> {
                            await core.opAsync("myco_op_write_all_tcp_stream", stream, maybeEncode(data));
                        },
                        close(): Promise<void> {
                            return core.opAsync("myco_op_close_tcp_stream", stream);
                        }
                    };
                },
                close(): Promise<void> {
                    return core.opAsync("myco_op_close_tcp_listener", listener);
                },
            }
        }
    }

    let memoized_argv: string[] | null = null;

    const Myco: Myco = {
        files,
        http,
        tcp,
        argv(): string[] {
            if (memoized_argv === null) {
                memoized_argv = core.ops.myco_op_argv_sync();
            }
            return memoized_argv!;
        },

        setTimeout(callback: (value: any) => any, delay: number) {
            core.opAsync("myco_op_set_timeout", delay).then(callback);
        },
    };

    function setTimeout(callback: (value: any) => any, delay: number) {
        core.opAsync("myco_op_set_timeout", delay).then(callback);
    }

    (globalThis as any).setTimeout = setTimeout;
    (globalThis as any).Myco = Myco;
    (globalThis as any).TextEncoder = TextEncoder;
    (globalThis as any).TextDecoder = TextDecoder;
    (Error as any).prepareStackTrace = (core as any).prepareStackTrace;
})();
