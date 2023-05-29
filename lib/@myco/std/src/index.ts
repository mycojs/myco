import {AsyncStream} from "./streams";

export * as core from './core';
export * as collections from './collections';
export * as streams from "./streams";
export * as channels from "./channels";

function delay(millis: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, millis));
}

export default async function main() {
    let i = 0;
    const stream = AsyncStream.from(async function*() {
        while (true) {
            await delay(1000);
            yield i++;
        }
    }())
    for await (const x of stream.filter(x => x % 2 == 0)) {
        console.log(x);
    }
}
