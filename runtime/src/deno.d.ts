type Token = string;

interface Ops {
    // Files
    myco_op_request_read_file(path: string): Promise<Token>;
    myco_op_request_write_file(path: string): Promise<Token>;
    myco_op_request_read_dir(path: string): Promise<Token>;
    myco_op_request_write_dir(path: string): Promise<Token>;
    myco_op_read_file(token: Token): Promise<string>;
    myco_op_read_file_sync(token: Token): string;
    myco_op_write_file(token: Token, contents: string): Promise<void>;
    myco_op_write_file_sync(token: Token, contents: string): void;
    myco_op_remove_file(token: Token): Promise<void>;
    myco_op_remove_file_sync(token: Token): void;
    myco_op_read_file_in_dir(token: Token, path: string): Promise<string>;
    myco_op_read_file_in_dir_sync(token: Token, path: string): string;
    myco_op_write_file_in_dir(token: Token, path: string, contents: string): Promise<void>;
    myco_op_write_file_in_dir_sync(token: Token, path: string, contents: string): void;
    myco_op_remove_file_in_dir(token: Token, path: string): Promise<void>;
    myco_op_remove_file_in_dir_sync(token: Token, path: string): void;

    // Http
    myco_op_request_fetch_url(url: string): Promise<Token>;
    myco_op_request_fetch_prefix(url: string): Promise<Token>;
    myco_op_fetch_url(token: Token): Promise<string>;

    // Core
    myco_op_set_timeout(delay: number): Promise<void>;
}

type FunctionKeys<T> = { [K in keyof T]: T[K] extends Function ? K : never }[keyof T];

type AsyncOps = Pick<Ops, Exclude<FunctionKeys<Ops>, `${string}_sync`>>;
type SyncOps = Pick<Ops, Extract<FunctionKeys<Ops>, `${string}_sync`>>;

declare const Deno: {
    core: {
        print(msg: string, isErr: boolean): void;
        opAsync<K extends keyof AsyncOps>(opId: K, ...args: Parameters<AsyncOps[K]>): ReturnType<AsyncOps[K]>;
        ops: SyncOps;
    };
}
