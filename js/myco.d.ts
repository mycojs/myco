declare interface Myco {
    readFile(path: string): string;
    writeFile(path: string, content: string): void;
    removeFile(path: string): void;
    fetch(url: string): string;
    setTimeout(callback: Function, delay: number): number;
    log(...args: any[]): void;
    error(...args: any[]): void;
}

declare const Myco: Myco;
