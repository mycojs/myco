declare interface Myco {
    files: Myco.Files;
    console: Myco.Console;
    http: Myco.Http;
    argv(): string[];
    setTimeout(callback: (value: any) => any, delay: number): void;
}

declare namespace Myco {
    interface Files {
        requestRead(path: string): Promise<Files.ReadToken>;

        requestWrite(path: string): Promise<Files.WriteToken>;

        requestReadWrite(path: string): Promise<Files.ReadWriteToken>;

        requestReadDir(path: string): Promise<Files.ReadDirToken>;

        requestWriteDir(path: string): Promise<Files.WriteDirToken>;

        requestReadWriteDir(path: string): Promise<Files.ReadWriteDirToken>;
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
            stat: Stats;
        }

        interface ReadToken {
            read(): Promise<string>;
            stat(): Promise<Stats | null>;
            sync: {
                read(): string;
                stat(): Stats | null;
            }
        }

        interface WriteToken {
            write(contents: string): Promise<void>;
            remove(): Promise<void>;
            sync: {
                write(contents: string): void;
                remove(): void;
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
            stat(path: string): Promise<Stats | null>;
            list(path: string, options?: ListDirOptions): Promise<File[]>;
            sync: {
                read(path: string): string;
                stat(path: string): Stats | null;
                list(path: string, options?: ListDirOptions): File[];
            }
        }

        interface WriteDirToken {
            write(path: string, contents: string): Promise<void>;
            remove(path: string): Promise<void>;
            mkdirp(path: string): Promise<void>;
            sync: {
                write(path: string, contents: string): void;
                remove(path: string): void;
                mkdirp(path: string): void;
            }
        }

        type ReadWriteDirToken =
            & ReadDirToken
            & WriteDirToken;
    }

    interface Console {
        log(...args: any[]): void;
        error(...args: any[]): void;
    }

    interface Http {
        fetch(url: string): Promise<string>;
        fetch<T extends 'utf-8' | 'raw'>(url: string, encoding: T): Promise<T extends 'raw' ? Uint8Array : string>;
        fetch(url: string, encoding: 'utf-8' | 'raw'): Promise<string | Uint8Array>;
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
