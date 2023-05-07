const {core} = Deno;
const {ops} = core;

function argsToMessage(...args: any[]) {
    return args.map((arg) => JSON.stringify(arg)).join(" ");
}

const Myco: Myco = {
    readFile(path: string) {
        return ops.op_read_file(path);
    },
    writeFile(path: string, contents: string) {
        return ops.op_write_file(path, contents);
    },
    removeFile(path: string) {
        return ops.op_remove_file(path);
    },

    async fetch(url: string) {
        return core.opAsync("op_fetch", url);
    },

    setTimeout(callback: (value: any) => any, delay: number) {
        core.opAsync("op_set_timeout", delay).then(callback);
    },

    log(...args: any[]) {
        core.print(`[out]: ${argsToMessage(...args)}\n`, false);
    },

    error(...args: any[]) {
        core.print(`[err]: ${argsToMessage(...args)}\n`, true);
    },
};

(globalThis as any).Myco = Myco;
