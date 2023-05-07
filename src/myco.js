const { core } = Deno;
const { ops } = core;

function argsToMessage(...args) {
    return args.map((arg) => JSON.stringify(arg)).join(" ");
}

globalThis.Myco = {
    readFile: (path) => {
        return ops.op_read_file(path);
    },
    writeFile: (path, contents) => {
        return ops.op_write_file(path, contents);
    },
    removeFile: (path) => {
        return ops.op_remove_file(path);
    },

    fetch: async (url) => {
        return core.opAsync("op_fetch", url);
    },

    setTimeout: (callback, delay) => {
        core.opAsync("op_set_timeout", delay).then(callback);
    },

    log: (...args) => {
        core.print(`[out]: ${argsToMessage(...args)}\n`, false);
    },

    error: (...args) => {
        core.print(`[err]: ${argsToMessage(...args)}\n`, true);
    },
};
