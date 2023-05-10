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
    compile(["./init/src/index.ts"], options, myco);
}

function compile(fileNames: string[], options: ts.CompilerOptions, myco: Myco): void {
    const {console} = myco;
    (ts as any).setSys(sys(myco));
    console.log("Set sys");
    let program = ts.createProgram(fileNames, options);
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

function sys(myco: Myco): ts.System {
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
            throw new Error("Not implemented");
        },
        getFileSize(path: string): number {
            throw new Error("Not implemented");
        },
        writeFile(path: string, data: string, writeByteOrderMark?: boolean): void {
            throw new Error("Not implemented");
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
            throw new Error("Not implemented");
        },
        getCurrentDirectory(): string {
            throw new Error("Not implemented");
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
