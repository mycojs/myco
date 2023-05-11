type Token = string;

interface Ops {
    // Files
    myco_op_request_read_file(path: string): Promise<Token>;
    myco_op_request_write_file(path: string): Promise<Token>;
    myco_op_request_read_dir(path: string): Promise<Token>;
    myco_op_request_write_dir(path: string): Promise<Token>;
    myco_op_read_file(token: Token): Promise<string>;
    myco_op_write_file(token: Token, contents: string): Promise<void>;
    myco_op_remove_file(token: Token): Promise<void>;
    myco_op_read_file_in_dir(token: Token, path: string): Promise<string>;
    myco_op_write_file_in_dir(token: Token, path: string, contents: string): Promise<void>;
    myco_op_remove_file_in_dir(token: Token, path: string): Promise<void>;

    // Http
    myco_op_request_fetch_url(url: string): Promise<Token>;
    myco_op_request_fetch_prefix(url: string): Promise<Token>;
    myco_op_fetch_url(token: Token): Promise<string>;

    // Core
    myco_op_set_timeout(delay: number): Promise<void>;
}

declare const Deno: {
    core: {
        print(msg: string, isErr: boolean): void;
        opAsync<K extends keyof Ops>(opId: K, ...args: Parameters<Ops[K]>): ReturnType<Ops[K]>;
    };
}
