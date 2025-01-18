export default async function ({http, files}: Myco) {
    const dir = await files.requestReadWriteDir(".");

    await removeRecursively(dir, 'src');

    const tgz = await http.fetch("https://registry.npmjs.org/typescript/-/typescript-5.7.3.tgz", 'raw');
    await dir.write("typescript.tar.gz", tgz);

    const echo = await files.requestExec("./extract.sh");
    const result = await echo.exec();

    if (!result.exit_code) {
        const libs = await dir.list("package/lib");

        const encoder = new TextEncoder();
        const decoder = new TextDecoder();

        // Copy all the package files into src
        await dir.mkdirp("src/");
        for (const file of libs) {
            if (file.stats.is_file) {
                let src = await dir.read("package/lib/" + file.name, 'raw');
                if (file.name == "typescript.js") {
                    let string = decoder.decode(src);
                    string = string.replace('if (typeof module !== "undefined" && module.exports) { module.exports = ts; }', 'export default ts;');
                    src = encoder.encode(string);
                } else if (file.name == "typescript.d.ts") {
                    let string = decoder.decode(src);
                    string = string.replace('export = ts;', 'export default ts;');
                    src = encoder.encode(string);
                }
                await dir.write("src/" + file.name, src);
            } else {
                const locale = file.name;
                const files = await dir.list(`package/lib/${locale}`);
                for (const file of files) {
                    if (file.stats.is_file) {
                        const src = await dir.read(`package/lib/${locale}/${file.name}`, 'raw');
                        await dir.mkdirp(`src/${locale}/`);
                        await dir.write(`src/${locale}/` + file.name, src);
                    } else {
                        console.error(`Unexpected directory in package: ${locale}/` + file.name);
                    }
                }
            }
        }

        const license = await dir.read("package/LICENSE.txt");
        await dir.write("src/LICENSE.txt", license);

        await dir.remove("typescript.tar.gz");
        await removeRecursively(dir, "package");
    }
}

async function removeRecursively(dir: Myco.Files.ReadWriteDirToken, path: string) {
    const stat = await dir.stat(path);
    if (stat) {
        const files = await dir.list(path);
        for (const file of files) {
            if (file.stats.is_file) {
                await dir.remove(`${path}/${file.name}`);
            } else {
                await removeRecursively(dir, `${path}/${file.name}`);
            }
        }
        await dir.rmdir(path);
    }
}
