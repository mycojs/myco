declare interface Myco {
    files: Myco.Files;
    http: Myco.Http;

    argv: string[];

    setTimeout(callback: () => void, delay: number): number;
    clearTimeout(timerId: number): void;
}

declare namespace Myco {
    interface Files {
        requestRead(path: string): Promise<Files.ReadToken>;

        requestWrite(path: string): Promise<Files.WriteToken>;

        requestReadWrite(path: string): Promise<Files.ReadWriteToken>;

        requestExec(path: string): Promise<Files.ExecToken>;

        requestReadDir(path: string): Promise<Files.ReadDirToken>;

        requestWriteDir(path: string): Promise<Files.WriteDirToken>;

        requestReadWriteDir(path: string): Promise<Files.ReadWriteDirToken>;

        requestExecDir(path: string): Promise<Files.ExecDirToken>;

        cwd(): string;

        chdir(path: string): Promise<void>;
    }

    interface Http {
        requestFetch(url: string): Promise<Http.FetchToken>;
        
        requestFetchPrefix(urlPrefix: string): Promise<Http.FetchPrefixToken>;
    }

    namespace Files {
        interface Stats {
            is_file: boolean;
            is_dir: boolean;
            is_symlink: boolean;
            size: number;
            readonly: boolean;
            modified?: number;
            accessed?: number;
            created?: number;
        }

        interface File {
            name: string;
            stats: Stats;
        }

        interface ReadToken {
            read(): Promise<string>;

            read<T extends 'utf-8' | 'raw'>(encoding: T): Promise<T extends 'raw' ? Uint8Array : string>;

            read(encoding: 'utf-8' | 'raw'): Promise<string | Uint8Array>;

            stat(): Promise<Stats | null>;

            sync: {
                read(): string;
                read<T extends 'utf-8' | 'raw'>(encoding: T): T extends 'raw' ? Uint8Array : string;
                read(encoding: 'utf-8' | 'raw'): string | Uint8Array;
                stat(): Stats | null;
            }
        }

        interface WriteToken {
            write(contents: string | Uint8Array): Promise<void>;

            remove(): Promise<void>;

            sync: {
                write(contents: string | Uint8Array): void;
                remove(): void;
            }
        }

        interface ExecToken {
            exec(args?: readonly string[]): Promise<ExecResult>;

            stat(): Promise<Stats | null>;

            sync: {
                exec(args?: readonly string[]): ExecResult;
                stat(): Stats | null;
            }
        }

        type ReadWriteToken =
            & ReadToken
            & WriteToken;

        interface ListDirOptions {
            /**
             * Whether to recurse into subdirectories. Defaults to false.
             */
            readonly recursive?: boolean;
            /**
             * A list of extensions to filter by. Defaults to all files.
             */
            readonly extensions?: readonly string[];
            /**
             * Whether to include directories in the results. Defaults to true.
             */
            readonly include_dirs?: boolean;
            /**
             * Whether to include files in the results. Defaults to true.
             */
            readonly include_files?: boolean;
            /**
             * Whether to include symlinks in the results. Defaults to true.
             */
            readonly include_symlinks?: boolean;
        }

        interface ReadDirToken {
            read(path: string): Promise<string>;

            read<T extends 'utf-8' | 'raw'>(path: string, encoding: T): Promise<T extends 'raw' ? Uint8Array : string>;

            read(path: string, encoding: 'utf-8' | 'raw'): Promise<string | Uint8Array>;

            stat(path: string): Promise<Stats | null>;

            list(path: string, options?: ListDirOptions): Promise<File[]>;

            sync: {
                read(path: string): string;
                read<T extends 'utf-8' | 'raw'>(path: string, encoding: T): T extends 'raw' ? Uint8Array : string;
                read(path: string, encoding: 'utf-8' | 'raw'): string | Uint8Array;
                stat(path: string): Stats | null;
                list(path: string, options?: ListDirOptions): File[];
            }
        }

        interface WriteDirToken {
            write(path: string, contents: string | Uint8Array): Promise<void>;

            remove(path: string): Promise<void>;

            mkdirp(path: string): Promise<void>;

            rmdir(path: string): Promise<void>;

            rmdirRecursive(path: string): Promise<void>;

            sync: {
                write(path: string, contents: string | Uint8Array): void;
                remove(path: string): void;
                mkdirp(path: string): void;
                rmdir(path: string): void;
            }
        }

        interface ExecDirToken {
            exec(path: string, args?: readonly string[]): Promise<ExecResult>;

            stat(path: string): Promise<Stats | null>;

            sync: {
                exec(path: string, args?: readonly string[]): ExecResult;
                stat(path: string): Stats | null;
            }
        }

        type ReadWriteDirToken =
            & ReadDirToken
            & WriteDirToken;

        interface ExecResult {
            readonly exit_code: number;

            stdout(): string;

            stdout<T extends 'utf-8' | 'raw'>(encoding: T): T extends 'raw' ? Uint8Array : string;

            stdout(encoding: 'utf-8' | 'raw'): string | Uint8Array;

            stderr(): string;

            stderr<T extends 'utf-8' | 'raw'>(encoding: T): T extends 'raw' ? Uint8Array : string;

            stderr(encoding: 'utf-8' | 'raw'): string | Uint8Array;
        }
    }

    namespace Http {
        interface FetchToken {
            fetch(): Promise<string>;

            fetch<T extends 'utf-8' | 'raw'>(encoding: T): Promise<T extends 'raw' ? Uint8Array : string>;

            fetch(encoding: 'utf-8' | 'raw'): Promise<string | Uint8Array>;
        }

        interface FetchPrefixToken {
            fetch(path: string): Promise<string>;

            fetch<T extends 'utf-8' | 'raw'>(path: string, encoding: T): Promise<T extends 'raw' ? Uint8Array : string>;

            fetch(path: string, encoding: 'utf-8' | 'raw'): Promise<string | Uint8Array>;
        }
    }
}

declare class TextEncoder {
    constructor(encoding?: 'utf-8');

    encode(text: string): Uint8Array;
}

declare class TextDecoder {
    constructor(encoding?: 'utf-8');

    decode(bytes: Uint8Array): string;
}

declare namespace console {
    function log(...args: any[]): void;

    function error(...args: any[]): void;

    function warn(...args: any[]): void;

    function info(...args: any[]): void;

    function debug(...args: any[]): void;

    function trace(...args: any[]): void;

    function assert(condition: any, ...args: any[]): void;
}

declare namespace TOML {
    function parse(text: string): any;
    function stringify(value: any): string;
}

declare function setTimeout(callback: () => void, delay: number): void;
