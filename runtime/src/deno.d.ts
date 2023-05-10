type Token = string;

interface Ops {
    // Files
    op_request_read_file(path: string): Promise<Token>;
    op_request_write_file(path: string): Promise<Token>;
    op_request_read_dir(path: string): Promise<Token>;
    op_request_write_dir(path: string): Promise<Token>;
    op_read_file(token: Token): Promise<string>;
    op_write_file(token: Token, contents: string): Promise<void>;
    op_remove_file(token: Token): Promise<void>;
    op_read_file_in_dir(token: Token, path: string): Promise<string>;
    op_write_file_in_dir(token: Token, path: string, contents: string): Promise<void>;
    op_remove_file_in_dir(token: Token, path: string): Promise<void>;

    // Http
    op_request_fetch_url(url: string): Promise<Token>;
    op_request_fetch_prefix(url: string): Promise<Token>;
    op_fetch_url(token: Token): Promise<string>;

    // Core
    op_set_timeout(delay: number): Promise<void>;
}

declare const Deno: {
    core: {
        print(msg: string, isErr: boolean): void;
        opAsync<K extends keyof Ops>(opId: K, ...args: Parameters<Ops[K]>): ReturnType<Ops[K]>;
    };
}
