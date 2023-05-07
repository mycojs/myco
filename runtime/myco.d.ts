declare interface Myco {
    readFile(path: string): string;
    writeFile(path: string, content: string): void;
    removeFile(path: string): void;
    fetch(url: string): Promise<string>;
    setTimeout(callback: (value: any) => any, delay: number): void;
    log(...args: any[]): void;
    error(...args: any[]): void;
}
