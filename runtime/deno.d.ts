interface Ops {
    op_read_file(path: string): Promise<string>;
    op_write_file(path: string, contents: string): Promise<void>;
    op_remove_file(path: string): Promise<void>;
    op_fetch(url: string): Promise<string>;
    op_set_timeout(delay: number): Promise<void>;
    op_secure_token(): Promise<string>;
}

declare const Deno: {
    core: {
        print(msg: string, isErr: boolean): void;
        opAsync<K extends keyof Ops>(opId: K, ...args: Parameters<Ops[K]>): ReturnType<Ops[K]>;
    };
}
