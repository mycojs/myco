import ts from "vendor/@myco/typescript/typescript.js";

export async function sys(myco: Myco, workingDir: Myco.Files.ReadWriteDirToken): Promise<ts.System> {
    return {
        args: [],
        newLine: '\n',
        useCaseSensitiveFileNames: true,
        write(s: string): void {
            myco.console.log("Call to write", s);
            throw new Error("Not implemented");
        },
        writeOutputIsTTY(): boolean {
            myco.console.log("Call to writeOutputIsTTY");
            throw new Error("Not implemented");
        },
        getWidthOfTerminal(): number {
            myco.console.log("Call to getWidthOfTerminal");
            throw new Error("Not implemented");
        },
        readFile(path: string, encoding?: string): string | undefined {
            return workingDir.sync.read(path); // TODO: Add encoding attribute to read ops
        },
        getFileSize(path: string): number {
            const stats = workingDir.sync.stat(path);
            return stats?.size ?? 0;
        },
        writeFile(path: string, data: string, writeOrderByteMark?: boolean): void {
            const directory = path.split('/').slice(0, -1).join('/');
            workingDir.sync.mkdirp(directory);
            workingDir.sync.write(path, data); // TODO: writeByteOrderMark?
        },
        /**
         * @pollingInterval - this parameter is used in polling-based watchers and ignored in watchers that
         * use native OS file watching
         */
        watchFile(path: string, callback: ts.FileWatcherCallback, pollingInterval?: number, options?: ts.WatchOptions): ts.FileWatcher {
            myco.console.log("Call to watchFile", path, callback, pollingInterval, options);
            throw new Error("Not implemented");
        },
        watchDirectory(path: string, callback: ts.DirectoryWatcherCallback, recursive?: boolean, options?: ts.WatchOptions): ts.FileWatcher {
            myco.console.log("Call to watchDirectory", path, callback, recursive, options);
            throw new Error("Not implemented");
        },
        resolvePath(path: string): string {
            myco.console.log("Call to resolvePath", path);
            throw new Error("Not implemented");
        },
        fileExists(path: string): boolean {
            const stats = workingDir.sync.stat(path);
            return stats?.is_file ?? false;
        },
        directoryExists(path: string): boolean {
            const stats = workingDir.sync.stat(path);
            return stats?.is_dir ?? false;
        },
        createDirectory(path: string): void {
            workingDir.sync.mkdirp(path);
        },
        getExecutingFilePath(): string {
            return '/vendor/@myco/typescript/typescript.js';
        },
        getCurrentDirectory(): string {
            return '/';
        },
        getDirectories(path: string): string[] {
            const files = workingDir.sync.list(path);
            return files.filter(file => file.stats.is_dir).map(file => file.name);
        },
        readDirectory(rootDir: string, extensions?: readonly string[], exclude?: readonly string[], include?: readonly string[], depth?: number): string[] {
            let files = workingDir.sync.list(rootDir, {
                extensions,
                // TODO: Excludes, includes, depth: implement glob in list
            });
            return files.map(file => rootDir + '/' + file.name);
        },
        getModifiedTime(path: string): Date | undefined {
            const stat = workingDir.sync.stat(path);
            const timestamp = stat?.modified;
            if (timestamp) {
                return new Date(timestamp);
            } else {
                return undefined;
            }
        },
        setModifiedTime(path: string, time: Date): void {
            myco.console.log("Call to setModifiedTime", path, time);
            throw new Error("Not implemented");
        },
        deleteFile(path: string): void {
            myco.console.log("Call to deleteFile", path);
            throw new Error("Not implemented");
        },
        /**
         * A good implementation is node.js' `crypto.createHash`. (https://nodejs.org/api/crypto.html#crypto_crypto_createhash_algorithm)
         */
        createHash(data: string): string {
            myco.console.log("Call to createHash", data);
            throw new Error("Not implemented");
        },
        /** This must be cryptographically secure. Only implement this method using `crypto.createHash("sha256")`. */
        createSHA256Hash(data: string): string {
            myco.console.log("Call to createSHA256Hash", data);
            throw new Error("Not implemented");
        },
        getMemoryUsage(): number {
            myco.console.log("Call to getMemoryUsage");
            throw new Error("Not implemented");
        },
        exit(exitCode?: number): void {
            myco.console.log("Call to exit", exitCode);
            throw new Error("Not implemented");
        },
        realpath(path: string): string {
            myco.console.log("Call to realpath", path);
            throw new Error("Not implemented");
        },
        setTimeout(callback: (...args: any[]) => void, ms: number, ...args: any[]): any {
            myco.console.log("Call to setTimeout", callback, ms, args);
            throw new Error("Not implemented");
        },
        clearTimeout(timeoutId: any): void {
            myco.console.log("Call to clearTimeout", timeoutId);
            throw new Error("Not implemented");
        },
        clearScreen(): void {
            myco.console.log("Call to clearScreen");
            throw new Error("Not implemented");
        },
        base64decode(input: string): string {
            myco.console.log("Call to base64decode", input);
            throw new Error("Not implemented");
        },
        base64encode(input: string): string {
            myco.console.log("Call to base64encode", input);
            throw new Error("Not implemented");
        }
    };
}

export async function host(myco: Myco): Promise<ts.CompilerHost> {
    const workingDir = await myco.files.requestReadWriteDir('.');
    const system = await sys(myco, workingDir);
    // noinspection UnnecessaryLocalVariableJS
    const host: ts.CompilerHost = {
        getSourceFile(fileName: string, languageVersionOrOptions: ts.ScriptTarget | ts.CreateSourceFileOptions, onError?: (message: string) => void, shouldCreateNewSourceFile?: boolean): ts.SourceFile | undefined {
            if (!fileName.startsWith('/')) {
                fileName = this.getCurrentDirectory() + '/' + fileName;
            }
            if (fileName.startsWith('/')) {
                fileName = fileName.replace(/^\/*/g, '');
            }
            const sourceText = workingDir.sync.read(fileName);
            return ts.createSourceFile(fileName, sourceText, languageVersionOrOptions);
        },
        getSourceFileByPath(fileName: string, path: ts.Path, languageVersionOrOptions: ts.ScriptTarget | ts.CreateSourceFileOptions, onError?: (message: string) => void, shouldCreateNewSourceFile?: boolean): ts.SourceFile | undefined {
            myco.console.log("Call to getSourceFileByPath", fileName, path, languageVersionOrOptions, onError, shouldCreateNewSourceFile);
            throw new Error("Not implemented");
        },
        getCancellationToken(): ts.CancellationToken {
            myco.console.log("Call to getCancellationToken", arguments);
            throw new Error("Not implemented");
        },
        getDefaultLibFileName(options: ts.CompilerOptions): string {
            return "lib.esnext.d.ts";
        },
        getDefaultLibLocation(): string {
            return "/vendor/@myco/typescript";
        },
        writeFile(path: string, data: string, writeByteOrderMark: boolean): void {
            if (!path.startsWith('/')) {
                path = this.getCurrentDirectory() + '/' + path;
            }
            system.writeFile(path, data, writeByteOrderMark);
        },
        getCurrentDirectory: system.getCurrentDirectory.bind(system),
        getCanonicalFileName(fileName: string): string {
            return this.getCurrentDirectory() + fileName;
        },
        useCaseSensitiveFileNames(): boolean {
            return system.useCaseSensitiveFileNames;
        },
        getNewLine(): string {
            return system.newLine;
        },
        getDirectories: system.getDirectories.bind(system),
        readDirectory: system.readDirectory.bind(system),
        realpath: system.realpath?.bind(system),
        readFile: system.readFile.bind(system),
        fileExists: system.fileExists.bind(system),
        directoryExists: system.directoryExists.bind(system),
        createHash: system.createHash?.bind(system),
    };
    return host;
}

export async function parseConfigFileHost(myco: Myco): Promise<ts.ParseConfigFileHost> {
    const workingDir = await myco.files.requestReadWriteDir('.');
    const system = await sys(myco, workingDir);
    // noinspection UnnecessaryLocalVariableJS
    const host: ts.ParseConfigFileHost = {
        onUnRecoverableConfigFileDiagnostic(diagnostic: ts.Diagnostic): void {
            throw new Error("Not implemented");
        },
        useCaseSensitiveFileNames: system.useCaseSensitiveFileNames,
        fileExists: system.fileExists.bind(system),
        getCurrentDirectory: system.getCurrentDirectory.bind(system),
        readDirectory: system.readDirectory.bind(system),
        readFile: system.readFile.bind(system),
    };
    return host;
}
