declare interface Myco {
    files: Myco.Files;
    console: Myco.Console;
    http: Myco.Http;
    setTimeout(callback: (value: any) => any, delay: number): void;
}

type WithSync<T extends { [K in keyof T]: (...args: any[]) => Promise<any> }> = T & {
    sync: {
        [K in keyof T]: T[K] extends (...args: infer A) => Promise<infer R>
            ? (...args: A) => R
            : T[K]
    }
};

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
        type ReadToken = WithSync<{
            read(): Promise<string>;
        }>;

        type WriteToken = WithSync<{
            write(contents: string): Promise<void>;
            remove(): Promise<void>;
        }>;

        type ReadWriteToken =
            & ReadToken
            & WriteToken;

        type ReadDirToken = WithSync<{
            read(path: string): Promise<string>;
        }>

        type WriteDirToken = WithSync<{
            write(path: string, contents: string): Promise<void>;
            remove(path: string): Promise<void>;
        }>

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
