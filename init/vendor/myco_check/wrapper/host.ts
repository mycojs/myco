import ts from "vendor/typescript/typescript.js";

export async function sys(myco: Myco, workingDir: Myco.Files.ReadWriteDirToken): Promise<ts.System> {
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
            return workingDir.sync.read(path); // TODO: Add encoding attribute to read ops
        },
        getFileSize(path: string): number {
            throw new Error("Not implemented");
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
            throw new Error("Not implemented");
        },
        watchDirectory(path: string, callback: ts.DirectoryWatcherCallback, recursive?: boolean, options?: ts.WatchOptions): ts.FileWatcher {
            throw new Error("Not implemented");
        },
        resolvePath(path: string): string {
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
            throw new Error("Not implemented");
        },
        getExecutingFilePath(): string {
            return '/vendor/myco_check/vendor/typescript/typescript.js';
        },
        getCurrentDirectory(): string {
            return '/';
        },
        getDirectories(path: string): string[] {
            throw new Error("Not implemented");
        },
        readDirectory(rootDir: string, extensions?: readonly string[], exclude?: readonly string[], include?: readonly string[], depth?: number): string[] {
            let files = workingDir.sync.list(rootDir, {
                extensions,
                // TODO: Excludes, includes, depth: implement glob in list
            });
            return files.map(file => rootDir + '/' + file.name);
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

export async function host(myco: Myco): Promise<ts.CompilerHost> {
    const workingDir = await myco.files.requestReadWriteDir('.');
    const system = await sys(myco, workingDir);
    // noinspection UnnecessaryLocalVariableJS
    const host = {
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
            throw new Error("Not implemented");
        },
        getCancellationToken(): ts.CancellationToken {
            throw new Error("Not implemented");
        },
        getDefaultLibFileName(options: ts.CompilerOptions): string {
            return "lib.esnext.d.ts";
        },
        getDefaultLibLocation(): string {
            return "/vendor/myco_check/vendor/typescript";
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
