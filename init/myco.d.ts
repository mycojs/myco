declare interface Myco {
    files: Myco.Files;
    console: Myco.Console;
    http: Myco.Http;
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

        interface ReadDirToken {
            read(path: string): Promise<string>;
            stat(path: string): Promise<Stats | null>;
            sync: {
                read(path: string): string;
                stat(path: string): Stats | null;
            }
        }

        interface WriteDirToken {
            write(path: string, contents: string): Promise<void>;
            remove(path: string): Promise<void>;
            sync: {
                write(path: string, contents: string): void;
                remove(path: string): void;
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
        request_fetch(url: string): Promise<string>;
        fetch(url: string): Promise<string>;
    }
}
