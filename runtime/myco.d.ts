declare interface Myco {
    files: Myco.Files;
    console: Myco.Console;
    http: Myco.Http;
    setTimeout(callback: (value: any) => any, delay: number): void;
}

declare namespace Myco {
    interface Files {
        requestRead(path: string): Promise<FileReadToken>;

        requestWrite(path: string): Promise<FileWriteToken>;

        requestReadWrite(path: string): Promise<FileReadWriteToken>;
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

declare type FileReadToken = {
    read(): Promise<string>;
};

declare type FileWriteToken = {
    write(contents: string): Promise<void>;
    remove(): Promise<void>;
};

declare type FileReadWriteToken =
    & FileReadToken
    & FileWriteToken;
