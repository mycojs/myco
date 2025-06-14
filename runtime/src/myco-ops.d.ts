type Token = string;

interface ExecResult {
    readonly stdout: Uint8Array;
    readonly stderr: Uint8Array;
    readonly exit_code: number;
}

declare global {
    interface MycoOps {
        sync: {
            // Filesystem
            read_file(args: { token: Token; path?: string }): Uint8Array;
            write_file(args: { token: Token; contents: Uint8Array; path?: string }): void;
            exec_file(args: { token: Token; path?: string; args: readonly string[] }): ExecResult;
            remove_file(args: { token: Token; path?: string }): void;
            stat_file(args: { token: Token; path?: string }): Myco.Files.Stats | null;
            list_dir(args: { token: Token; path: string }): Myco.Files.File[];
            mkdirp(args: { token: Token; path: string }): void;
            rmdir(args: { token: Token; path: string }): void;
            cwd(args: {}): string;
            chdir(path: string): Promise<void>;

            // Encoding
            encode_utf8(args: { text: string }): Uint8Array;
            decode_utf8(args: { bytes: Uint8Array }): string;
        
            // TOML
            toml_parse(args: { toml_string: string }): any;
            toml_stringify(args: { value: any }): string;
    
            // Core
            set_timeout(args: { delay: number }): number;
            clear_timeout(args: { timer_id: number }): void;
            print(args: { message: string }): void;
            eprint(args: { message: string }): void;
            trace(args: {}): string;
        };
        async: {
            // Token requests are always async
            request_read_file(path: string): Promise<Token>;
            request_write_file(path: string): Promise<Token>;
            request_exec_file(path: string): Promise<Token>;
            request_read_dir(path: string): Promise<Token>;
            request_write_dir(path: string): Promise<Token>;
            request_exec_dir(path: string): Promise<Token>;

            // Filesystem
            read_file(token: Token, path?: string): Promise<Uint8Array>;
            write_file(token: Token, contents: Uint8Array, path?: string): Promise<void>;
            exec_file(token: Token, path: string | undefined, args: readonly string[]): Promise<ExecResult>;
            remove_file(token: Token, path?: string): Promise<void>;
            stat_file(token: Token, path?: string): Promise<Myco.Files.Stats | null>;
            list_dir(token: Token, path: string): Promise<Myco.Files.File[]>;
            mkdirp(token: Token, path: string): Promise<void>;
            rmdir(token: Token, path: string): Promise<void>;
            rmdir_recursive(token: Token, path: string): Promise<void>;

            // HTTP
            request_fetch_url(url: string): Promise<Token>;
            request_fetch_prefix(url: string): Promise<Token>;
            fetch_url(token: Token, url?: string): Promise<Uint8Array>;
        };
    }

    const MycoOps: MycoOps;
}

export {};
