type Token = string;

interface ExecResult {
    readonly stdout: Uint8Array;
    readonly stderr: Uint8Array;
    readonly exit_code: number;
}

declare global {
    interface MycoOps {
        // Files
        request_read_file(path: string): Promise<Token>;
        request_write_file(path: string): Promise<Token>;
        request_exec_file(path: string): Promise<Token>;
        request_read_dir(path: string): Promise<Token>;
        request_write_dir(path: string): Promise<Token>;
        request_exec_dir(path: string): Promise<Token>;
        read_file(token: Token, path?: string): Promise<Uint8Array>;
        read_file_sync(token: Token, path?: string): Uint8Array;
        write_file(token: Token, contents: Uint8Array, path?: string): Promise<void>;
        write_file_sync(token: Token, contents: Uint8Array, path?: string): void;
        remove_file(token: Token, path?: string): Promise<void>;
        remove_file_sync(token: Token, path?: string): void;
        stat_file(token: Token, path?: string): Promise<Myco.Files.Stats | null>;
        stat_file_sync(token: Token, path?: string): Myco.Files.Stats | null;
        list_dir(token: Token, path: string): Promise<Myco.Files.File[]>;
        list_dir_sync(token: Token, path: string): Myco.Files.File[];
        mkdirp(token: Token, path: string): Promise<void>;
        mkdirp_sync(token: Token, path: string): void;
        rmdir(token: Token, path: string): Promise<void>;
        rmdir_sync(token: Token, path: string): void;
        exec_file(token: Token, path: string | undefined, args: readonly string[]): Promise<ExecResult>;
        exec_file_sync(token: Token, path: string | undefined, args: readonly string[]): ExecResult;
    
        // Encoding
        encode_utf8_sync(str: string): Uint8Array;
        decode_utf8_sync(bytes: Uint8Array): string;
    
        // Core
        set_timeout(delay: number): number;
        clear_timeout(timerId: number): void;
        print(msg: string, isErr: boolean): void;
        trace(): string;
    }

    const MycoOps: MycoOps;
}

export {};
