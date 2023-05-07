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
        type ReadToken = {
            read(): Promise<string>;
        };

        type WriteToken = {
            write(contents: string): Promise<void>;
            remove(): Promise<void>;
        };

        type ReadWriteToken =
            & ReadToken
            & WriteToken;

        type ReadDirToken = {
            read(path: string): Promise<string>;
        }

        type WriteDirToken = {
            write(path: string, contents: string): Promise<void>;
            remove(path: string): Promise<void>;
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
