import ts from '../vendor/typescript.js';

export default async function (myco: Myco) {
    const {console, files} = myco;
    const configToken = await files.requestRead("./runtime/tsconfig.json");
    const configFile = await configToken.read();
    const tsconfig = JSON.parse(configFile);
    const {options, errors} = ts.convertCompilerOptionsFromJson(tsconfig.compilerOptions, "./")!;
    if (errors.length) {
        console.error(errors);
        return;
    }
    await compile(["./init/src/index.ts"], options, myco);
}

async function compile(fileNames: string[], options: ts.CompilerOptions, myco: Myco): Promise<void> {
    const {console} = myco;
    (ts as any).setSys(sys(myco));
    console.log("Set sys");
    let program = ts.createProgram(fileNames, options, await host(myco));
    console.log(program);
    let emitResult = program.emit();

    let allDiagnostics = ts
        .getPreEmitDiagnostics(program)
        .concat(emitResult.diagnostics);

    allDiagnostics.forEach(diagnostic => {
        if (diagnostic.file) {
            let { line, character } = ts.getLineAndCharacterOfPosition(diagnostic.file, diagnostic.start!);
            let message = ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n");
            console.log(`${diagnostic.file.fileName} (${line + 1},${character + 1}): ${message}`);
        } else {
            console.log(ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"));
        }
    });

    let exitCode = emitResult.emitSkipped ? 1 : 0;
    console.log(`Process exiting with code '${exitCode}'.`);
    // TODO: process.exit(exitCode);
}

async function sys(myco: Myco): Promise<ts.System> {
    const dir = await myco.files.requestReadWriteDir('./');
    return {
        args: [],
        newLine: '\n',
        useCaseSensitiveFileNames: true,
        write(s: string): void {
            throw new Error("Not implemented");
        },
        writeOutputIsTTY(): boolean {
            throw new Error("Not implemented");
        },
        getWidthOfTerminal(): number {
            throw new Error("Not implemented");
        },
        readFile(path: string, encoding?: string): string | undefined {
            return dir.sync.read(path); // TODO: Add encoding attribute to read ops
        },
        getFileSize(path: string): number {
            throw new Error("Not implemented");
        },
        writeFile(path: string, data: string): void {
            dir.sync.write(path, data);
        },
        /**
         * @pollingInterval - this parameter is used in polling-based watchers and ignored in watchers that
         * use native OS file watching
         */
        watchFile(path: string, callback: ts.FileWatcherCallback, pollingInterval?: number, options?: ts.WatchOptions): ts.FileWatcher {
            throw new Error("Not implemented");
        },
        watchDirectory(path: string, callback: ts.DirectoryWatcherCallback, recursive?: boolean, options?: ts.WatchOptions): ts.FileWatcher {
            throw new Error("Not implemented");
        },
        resolvePath(path: string): string {
            throw new Error("Not implemented");
        },
        fileExists(path: string): boolean {
            throw new Error("Not implemented");
        },
        directoryExists(path: string): boolean {
            throw new Error("Not implemented");
        },
        createDirectory(path: string): void {
            throw new Error("Not implemented");
        },
        getExecutingFilePath(): string {
            return '/runtime/vendor/typescript.js';
        },
        getCurrentDirectory(): string {
            return '/';
        },
        getDirectories(path: string): string[] {
            throw new Error("Not implemented");
        },
        readDirectory(path: string, extensions?: readonly string[], exclude?: readonly string[], include?: readonly string[], depth?: number): string[] {
            throw new Error("Not implemented");
        },
        getModifiedTime(path: string): Date | undefined {
            throw new Error("Not implemented");
        },
        setModifiedTime(path: string, time: Date): void {
            throw new Error("Not implemented");
        },
        deleteFile(path: string): void {
            throw new Error("Not implemented");
        },
        /**
         * A good implementation is node.js' `crypto.createHash`. (https://nodejs.org/api/crypto.html#crypto_crypto_createhash_algorithm)
         */
        createHash(data: string): string {
            throw new Error("Not implemented");
        },
        /** This must be cryptographically secure. Only implement this method using `crypto.createHash("sha256")`. */
        createSHA256Hash(data: string): string {
            throw new Error("Not implemented");
        },
        getMemoryUsage(): number {
            throw new Error("Not implemented");
        },
        exit(exitCode?: number): void {
            throw new Error("Not implemented");
        },
        realpath(path: string): string {
            throw new Error("Not implemented");
        },
        setTimeout(callback: (...args: any[]) => void, ms: number, ...args: any[]): any {
            throw new Error("Not implemented");
        },
        clearTimeout(timeoutId: any): void {
            throw new Error("Not implemented");
        },
        clearScreen(): void {
            throw new Error("Not implemented");
        },
        base64decode(input: string): string {
            throw new Error("Not implemented");
        },
        base64encode(input: string): string {
            throw new Error("Not implemented");
        }
    };
}

async function host(myco: Myco): Promise<ts.CompilerHost> {
    const dir = await myco.files.requestReadWriteDir('.');
    return {
        getSourceFile(fileName: string, languageVersionOrOptions: ts.ScriptTarget | ts.CreateSourceFileOptions, onError?: (message: string) => void, shouldCreateNewSourceFile?: boolean): ts.SourceFile | undefined {
            const sourceText = dir.sync.read(fileName);
            return ts.createSourceFile(fileName, sourceText, languageVersionOrOptions);
        },
        getSourceFileByPath(fileName: string, path: ts.Path, languageVersionOrOptions: ts.ScriptTarget | ts.CreateSourceFileOptions, onError?: (message: string) => void, shouldCreateNewSourceFile?: boolean): ts.SourceFile | undefined {
            throw new Error("Not implemented");
        },
        getCancellationToken(): ts.CancellationToken {
            throw new Error("Not implemented");
        },
        getDefaultLibFileName(options: ts.CompilerOptions): string {
            return "lib.esnext.d.ts";
        },
        getDefaultLibLocation(): string {
            return "/runtime/vendor/";
        },
        writeFile(path: string, data: string, writeByteOrderMark: boolean): void {
            dir.sync.write(path, data); // TODO: writeByteOrderMark?
        },
        getCurrentDirectory(): string {
            return '/';
        },
        getCanonicalFileName(fileName: string): string {
            return this.getCurrentDirectory() + fileName;
        },
        useCaseSensitiveFileNames(): boolean {
            return true;
        },
        getNewLine(): string {
            return '\n';
        },
        getDirectories(path: string): string[] {
            throw new Error("Not implemented");
        },
        readDirectory(rootDir: string, extensions: readonly string[], excludes: readonly string[] | undefined, includes: readonly string[], depth?: number): string[] {
            throw new Error("Not implemented");
        },
        realpath(path: string): string {
            throw new Error("Not implemented");
        },
        readFile(path: string, encoding?: string): string | undefined {
            return dir.sync.read(path); // TODO: Add encoding attribute to read ops
        },
        fileExists(path: string): boolean {
            throw new Error("Not implemented");
        },
        directoryExists(path: string): boolean {
            throw new Error("Not implemented");
        },
        /**
         * A good implementation is node.js' `crypto.createHash`. (https://nodejs.org/api/crypto.html#crypto_crypto_createhash_algorithm)
         */
        createHash(data: string): string {
            throw new Error("Not implemented");
        },
    };
}
