declare const Deno: {
    core: {
        print(msg: string, isErr: boolean): void;
        opAsync(opName: string, ...args: any[]): Promise<any>;
        ops: {
            op_read_file(path: string): string;
            op_write_file(path: string, contents: string): void;
            op_remove_file(path: string): void;
        };
    };
}
