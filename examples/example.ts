let console: Myco.Console;

export default async function ({files, console: _console}: Myco) {
    console = _console;
    const directory = await files.requestReadWriteDir("./data");
    await childFunction(directory);
}

async function childFunction(token: Myco.Files.ReadWriteDirToken) {
    const contents = await token.read("../build.rs");
    const transform = contents.replace("a", "b");
    await token.write("./log.txt", transform);
}
