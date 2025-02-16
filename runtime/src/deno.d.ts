type Token = string;

interface ExecResult {
    readonly stdout: Uint8Array;
    readonly stderr: Uint8Array;
    readonly exit_code: number;
}

interface Ops {
    // Files
    myco_op_request_read_file(path: string): Promise<Token>;
    myco_op_request_write_file(path: string): Promise<Token>;
    myco_op_request_exec_file(path: string): Promise<Token>;
    myco_op_request_read_dir(path: string): Promise<Token>;
    myco_op_request_write_dir(path: string): Promise<Token>;
    myco_op_request_exec_dir(path: string): Promise<Token>;
    myco_op_read_file(token: Token, path?: string): Promise<Uint8Array>;
    myco_op_read_file_sync(token: Token, path?: string): Uint8Array;
    myco_op_write_file(token: Token, contents: Uint8Array, path?: string): Promise<void>;
    myco_op_write_file_sync(token: Token, contents: Uint8Array, path?: string): void;
    myco_op_remove_file(token: Token, path?: string): Promise<void>;
    myco_op_remove_file_sync(token: Token, path?: string): void;
    myco_op_stat_file(token: Token, path?: string): Promise<Myco.Files.Stats | null>;
    myco_op_stat_file_sync(token: Token, path?: string): Myco.Files.Stats | null;
    myco_op_list_dir(token: Token, path: string): Promise<Myco.Files.File[]>;
    myco_op_list_dir_sync(token: Token, path: string): Myco.Files.File[];
    myco_op_mkdirp(token: Token, path: string): Promise<void>;
    myco_op_mkdirp_sync(token: Token, path: string): void;
    myco_op_rmdir(token: Token, path: string): Promise<void>;
    myco_op_rmdir_sync(token: Token, path: string): void;
    myco_op_exec_file(token: Token, path: string | undefined, args: readonly string[]): Promise<ExecResult>;
    myco_op_exec_file_sync(token: Token, path: string | undefined, args: readonly string[]): ExecResult;

    // Http
    myco_op_request_fetch_url(url: string): Promise<Token>;
    myco_op_request_fetch_prefix(url: string): Promise<Token>;
    myco_op_fetch_url(token: Token): Promise<Uint8Array>;
    myco_op_bind_tcp_listener(addr: string): Promise<Token>;
    myco_op_accept_tcp_stream(token: Token): Promise<Token>;
    myco_op_read_all_tcp_stream(token: Token): Promise<Uint8Array>;
    myco_op_write_all_tcp_stream(token: Token, buf: Uint8Array): Promise<void>;
    myco_op_close_tcp_stream(token: Token): Promise<void>;
    myco_op_close_tcp_listener(token: Token): Promise<void>;

    // Encoding
    myco_op_encode_utf8_sync(str: string): Uint8Array;
    myco_op_decode_utf8_sync(bytes: Uint8Array): string;

    // Core
    myco_op_set_timeout(delay: number): Promise<void>;
    myco_op_argv_sync(): string[];
}

type FunctionKeys<T> = { [K in keyof T]: T[K] extends Function ? K : never }[keyof T];

type AsyncOps = Pick<Ops, Exclude<FunctionKeys<Ops>, `${string}_sync`>>;
type SyncOps = Pick<Ops, Extract<FunctionKeys<Ops>, `${string}_sync`>>;

declare const Deno: {
    core: {
        print(msg: string, isErr: boolean): void;
        // noinspection JSAnnotator
        opAsync<K extends keyof AsyncOps>(opId: K, ...args: Parameters<AsyncOps[K]>): ReturnType<AsyncOps[K]>;
        ops: SyncOps;
    };
}
