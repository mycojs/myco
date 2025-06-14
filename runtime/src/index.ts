// Simple V8-compatible runtime for Myco
// This will be expanded as we migrate the ops

(function () {
    // Capture MycoOps before we delete it from global scope
    const MycoOps: MycoOps = (globalThis as any).MycoOps;
    if (!MycoOps) {
        throw new Error("MycoOps not found on globalThis");
    }

    // Wrap each MycoOps function in a try/catch and print the stack trace
    for (const key in MycoOps) {
        if (typeof MycoOps[key as keyof typeof MycoOps] === 'function' && !key.endsWith('_sync')) {
            const originalFn = MycoOps[key as keyof typeof MycoOps] as Function;
            const newFn: Function = async function(...args: any[]) {
                try {
                    return await originalFn(...args);
                } catch (e: any) {
                    let errorMessage = e.toString();
                    if (errorMessage.includes("Error: ")) {
                        errorMessage = errorMessage.slice(7);
                    }
                    const error = new Error(errorMessage);
                    // Omit this frame from the stack trace
                    error.stack = error.stack?.replace(/ +at async <internal> [^\n]*\n/, '');
                    throw error;
                }
            };
            Object.defineProperty(newFn, 'name', {
                value: 'async <internal>',
                writable: false,
                configurable: false,
            });
            (MycoOps[key as keyof typeof MycoOps] as any) = newFn;
        }
    }

    // Delete MycoOps from globalThis so it's not accessible to user code
    delete (globalThis as any).MycoOps;
    
    // Helper function to format multiple arguments like console does
    function formatArgs(...args: any[]): string {
        return args.map(arg => {
            if (typeof arg === 'string') {
                return arg;
            } else if (typeof arg === 'number' || typeof arg === 'boolean') {
                return String(arg);
            } else if (arg === null) {
                return 'null';
            } else if (arg === undefined) {
                return 'undefined';
            } else {
                try {
                    return JSON.stringify(arg);
                } catch {
                    return '[object Object]';
                }
            }
        }).join(' ');
    }
    

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

    function fileExtension(path: string) {
        return path.split(".").pop()?.toLowerCase();
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

        const matchesStat = (file: Myco.Files.File) => {
            // If include_dirs is false, exclude directories
            if (options?.include_dirs === false && file.stats.is_dir) {
                return false;
            }
            // If include_files is false, exclude files
            if (options?.include_files === false && file.stats.is_file) {
                return false;
            }
            // If include_symlinks is false, exclude symlinks
            if (options?.include_symlinks === false && file.stats.is_symlink) {
                return false;
            }
            return true;
        };

        return list.filter((file) =>
            matchesExtensions(file) && matchesStat(file)
        )
    }

    // Helper function to check truthiness like JavaScript
    function isTruthy(value: any): boolean {
        if (typeof value === 'boolean') return value;
        if (typeof value === 'number') return value !== 0;
        if (typeof value === 'string') return value !== '';
        if (value === null || value === undefined) return false;
        return true; // Objects are truthy
    }
    
    // Create console object using MycoOps
    const console = {
        log(...args: any[]) {
            const message = formatArgs(...args);
            MycoOps.print_sync(message + '\n');
        },
        
        error(...args: any[]) {
            const message = formatArgs(...args);
            MycoOps.eprint_sync(message + '\n');
        },
        
        warn(...args: any[]) {
            const message = formatArgs(...args);
            MycoOps.eprint_sync(message + '\n');
        },
        
        info(...args: any[]) {
            const message = formatArgs(...args);
            MycoOps.print_sync(message + '\n');
        },
        
        debug(...args: any[]) {
            const message = formatArgs(...args);
            MycoOps.print_sync(message + '\n');
        },
        
        trace(...args: any[]) {
            const stackTrace = MycoOps.trace_sync();
            if (args.length > 0) {
                const message = formatArgs(...args);
                MycoOps.print_sync(message + '\n');
            }
            MycoOps.print_sync(stackTrace + '\n');
        },
        
        assert(condition: any, ...args: any[]) {
            if (!isTruthy(condition)) {
                if (args.length > 0) {
                    const message = formatArgs(...args);
                    MycoOps.eprint_sync('Assertion failed: ' + message + '\n');
                } else {
                    MycoOps.eprint_sync('Assertion failed\n');
                }
            }
        }
    };
    
    // Set console on globalThis
    (globalThis as any).console = console;
    
    // Create TextEncoder class using MycoOps
    class TextEncoder {
        constructor(encoding?: 'utf-8') {
            if (encoding && encoding !== 'utf-8') {
                throw new Error('Only utf-8 encoding is supported');
            }
        }
        
        encode(text: string): Uint8Array {
            return MycoOps.encode_utf8_sync(text);
        }
    }
    
    // Create TextDecoder class using MycoOps
    class TextDecoder {
        constructor(encoding?: 'utf-8') {
            if (encoding && encoding !== 'utf-8') {
                throw new Error('Only utf-8 encoding is supported');
            }
        }
        
        decode(bytes: Uint8Array): string {
            return MycoOps.decode_utf8_sync(bytes);
        }
    }
    
    // Set TextEncoder and TextDecoder on globalThis
    (globalThis as any).TextEncoder = TextEncoder;
    (globalThis as any).TextDecoder = TextDecoder;
    
    // Create TOML namespace using MycoOps
    const TOML = {
        parse(text: string): any {
            return MycoOps.toml_parse_sync(text);
        },
        
        stringify(value: any): string {
            return MycoOps.toml_stringify_sync(value);
        }
    };
    
    // Set TOML on globalThis
    (globalThis as any).TOML = TOML;
    
    // Timer callback storage
    const timerCallbacks = new Map<number, () => void>();
    
    // Global timer completion handler (called by Rust when timers fire)
    (globalThis as any).__mycoTimerComplete = function(timerId: number) {
        const callback = timerCallbacks.get(timerId);
        if (callback) {
            timerCallbacks.delete(timerId);
            callback();
        }
    };
    
    // Get any existing Myco object (which may have been set by Rust code)
    const existingMyco = (globalThis as any).Myco || {};
    
    // Create a basic Myco object structure, preserving existing properties
    const myco: any = {
        ...existingMyco, // Preserve any existing properties like setTimeout
        setTimeout(callback: () => void, delay: number): number {
            const timerId = MycoOps.set_timeout_sync(delay);
            timerCallbacks.set(timerId, callback);
            return timerId;
        },
        clearTimeout(timerId: number): void {
            timerCallbacks.delete(timerId);
            MycoOps.clear_timeout_sync(timerId);
        },
        http: {
            async requestFetch(url: string): Promise<Myco.Http.FetchToken> {
                const token = await MycoOps.request_fetch_url(url);
                return {
                    async fetch(encoding: 'utf-8' | 'raw' = 'utf-8'): Promise<any> {
                        const raw = await MycoOps.fetch_url(token);
                        return maybeDecode(raw, encoding);
                    }
                };
            },
            async requestFetchPrefix(urlPrefix: string): Promise<Myco.Http.FetchPrefixToken> {
                const token = await MycoOps.request_fetch_prefix(urlPrefix);
                return {
                    async fetch(path: string, encoding: 'utf-8' | 'raw' = 'utf-8'): Promise<any> {
                        const raw = await MycoOps.fetch_url(token, path);
                        return maybeDecode(raw, encoding);
                    }
                };
            }
        },
        files: {
            async requestRead(path: string): Promise<Myco.Files.ReadToken> {
                const token = await MycoOps.request_read_file(path);
                return {
                    async read(encoding: 'utf-8' | 'raw' = 'utf-8'): Promise<any> {
                        const raw = await MycoOps.read_file(token);
                        return maybeDecode(raw, encoding);
                    },
                    async stat(): Promise<Myco.Files.Stats | null> {
                        return await MycoOps.stat_file(token);
                    },
                    sync: {
                        read(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                            const raw = MycoOps.read_file_sync(token);
                            return maybeDecode(raw, encoding);
                        },
                        stat() {
                            return MycoOps.stat_file_sync(token);
                        }
                    },
                };
            },
            async requestWrite(path: string): Promise<Myco.Files.WriteToken> {
                const token = await MycoOps.request_write_file(path);
                return {
                    async write(contents: string | Uint8Array) {
                        return await MycoOps.write_file(token, maybeEncode(contents));
                    },
                    async remove() {
                        return await MycoOps.remove_file(token);
                    },
                    sync: {
                        write(contents: string | Uint8Array) {
                            return MycoOps.write_file_sync(token, maybeEncode(contents));
                        },
                        remove() {
                            return MycoOps.remove_file_sync(token);
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
                const token = await MycoOps.request_exec_file(path);
                return {
                    async exec(args: readonly string[] = []): Promise<Myco.Files.ExecResult> {
                        const result = await MycoOps.exec_file(token, undefined, args);
                        return {
                            exit_code: result.exit_code,
                            stdout(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                const stdoutBytes = new Uint8Array(result.stdout);
                                return maybeDecode(stdoutBytes, encoding);
                            },
                            stderr(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                const stderrBytes = new Uint8Array(result.stderr);
                                return maybeDecode(stderrBytes, encoding);
                            },
                        }
                    },
                    async stat(): Promise<Myco.Files.Stats | null> {
                        return await MycoOps.stat_file(token);
                    },
                    sync: {
                        exec(args: string[] = []): Myco.Files.ExecResult {
                            const result = MycoOps.exec_file_sync(token, undefined, args);
                            return {
                                exit_code: result.exit_code,
                                stdout(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                    const stdoutBytes = new Uint8Array(result.stdout);
                                    return maybeDecode(stdoutBytes, encoding);
                                },
                                stderr(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                    const stderrBytes = new Uint8Array(result.stderr);
                                    return maybeDecode(stderrBytes, encoding);
                                },
                            }
                        },
                        stat() {
                            return MycoOps.stat_file_sync(token);
                        }
                    },
                };
            },
            async requestReadDir(path: string): Promise<Myco.Files.ReadDirToken> {
                const rootDir = await MycoOps.request_read_dir(path);
                const token: Myco.Files.ReadDirToken = {
                    async read(path: string, encoding: 'utf-8' | 'raw' = 'utf-8'): Promise<any> {
                        const raw = await MycoOps.read_file(rootDir, path);
                        return maybeDecode(raw, encoding);
                    },
                    async stat(path: string): Promise<Myco.Files.Stats | null> {
                        return await MycoOps.stat_file(rootDir, path);
                    },
                    async list(path: string, options) {
                        let list = await MycoOps.list_dir(rootDir, path);
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
                            const raw = MycoOps.read_file_sync(rootDir, path);
                            return maybeDecode(raw, encoding);
                        },
                        stat(path: string) {
                            return MycoOps.stat_file_sync(rootDir, path);
                        },
                        list(path: string, options) {
                            let list = MycoOps.list_dir_sync(rootDir, path);
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
                const token = await MycoOps.request_write_dir(path);
                return {
                    async write(path: string, contents: string | Uint8Array): Promise<void> {
                        return await MycoOps.write_file(token, maybeEncode(contents), path);
                    },
                    async remove(path: string): Promise<void> {
                        return await MycoOps.remove_file(token, path);
                    },
                    async mkdirp(path: string): Promise<void> {
                        return await MycoOps.mkdirp(token, path);
                    },
                    async rmdir(path: string): Promise<void> {
                        return await MycoOps.rmdir(token, path);
                    },
                    async rmdirRecursive(path: string): Promise<void> {
                        return await MycoOps.rmdir_recursive(token, path);
                    },
                    sync: {
                        write(path: string, contents: string | Uint8Array) {
                            return MycoOps.write_file_sync(token, maybeEncode(contents), path);
                        },
                        remove(path: string) {
                            return MycoOps.remove_file_sync(token, path);
                        },
                        mkdirp(path: string) {
                            return MycoOps.mkdirp_sync(token, path);
                        },
                        rmdir(path: string) {
                            return MycoOps.rmdir_sync(token, path);
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
                const token = await MycoOps.request_exec_dir(path);
                return {
                    async exec(path: string, args: readonly string[] = []): Promise<Myco.Files.ExecResult> {
                        const result = await MycoOps.exec_file(token, path, args);
                        return {
                            exit_code: result.exit_code,
                            stdout(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                const stdoutBytes = new Uint8Array(result.stdout);
                                return maybeDecode(stdoutBytes, encoding);
                            },
                            stderr(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                const stderrBytes = new Uint8Array(result.stderr);
                                return maybeDecode(stderrBytes, encoding);
                            },
                        }
                    },
                    async stat(path: string): Promise<Myco.Files.Stats | null> {
                        return await MycoOps.stat_file(token, path);
                    },
                    sync: {
                        exec(path: string, args: string[] = []): Myco.Files.ExecResult {
                            const result = MycoOps.exec_file_sync(token, path, args);
                            return {
                                exit_code: result.exit_code,
                                stdout(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                    const stdoutBytes = new Uint8Array(result.stdout);
                                    return maybeDecode(stdoutBytes, encoding);
                                },
                                stderr(encoding: 'utf-8' | 'raw' = 'utf-8'): any {
                                    const stderrBytes = new Uint8Array(result.stderr);
                                    return maybeDecode(stderrBytes, encoding);
                                },
                            }
                        },
                        stat(path: string) {
                            return MycoOps.stat_file_sync(token, path);
                        }
                    },
                };
            },
            cwd(): string {
                return MycoOps.cwd_sync();
            },
            async chdir(path: string): Promise<void> {
                await MycoOps.chdir(path);
            }
        }
    };

    // Set the merged Myco object on globalThis so it can be accessed
    (globalThis as any).Myco = myco;
})();
