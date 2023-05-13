import ts from "../../vendor/typescript/typescript.js";

export async function sys(myco: Myco): Promise<ts.System> {
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
            return '/vendor/myco_check/vendor/typescript/typescript.js';
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

export async function host(myco: Myco): Promise<ts.CompilerHost> {
    const dir = await myco.files.requestReadWriteDir('.');
    // noinspection UnnecessaryLocalVariableJS
    const host = {
        getSourceFile(fileName: string, languageVersionOrOptions: ts.ScriptTarget | ts.CreateSourceFileOptions, onError?: (message: string) => void, shouldCreateNewSourceFile?: boolean): ts.SourceFile | undefined {
            if (!fileName.startsWith('/')) {
                fileName = this.getCurrentDirectory() + '/' + fileName;
            }
            if (fileName.startsWith('/')) {
                fileName = fileName.replace(/^\/*/g, '');
            }
            myco.console.log('getSourceFile', fileName);
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
            return "/vendor/myco_check/vendor/typescript";
        },
        writeFile(path: string, data: string, writeByteOrderMark: boolean): void {
            if (!path.startsWith('/')) {
                path = this.getCurrentDirectory() + '/' + path;
            }
            if (path.startsWith('/')) {
                path = path.replace(/^\/*/g, '');
            }
            const directory = path.split('/').slice(0, -1).join('/');
            myco.console.log('writeFile', path, directory)
            dir.sync.mkdirp(directory);
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
            myco.console.log('readFile', path);
            return dir.sync.read(path); // TODO: Add encoding attribute to read ops
        },
        fileExists(path: string): boolean {
            const stats = dir.sync.stat(path);
            return stats?.is_file ?? false;
        },
        directoryExists(path: string): boolean {
            const stats = dir.sync.stat(path);
            return stats?.is_dir ?? false;
        },
        /**
         * A good implementation is node.js' `crypto.createHash`. (https://nodejs.org/api/crypto.html#crypto_crypto_createhash_algorithm)
         */
        createHash(data: string): string {
            throw new Error("Not implemented");
        },
    };
    return host;
}