import {Queue, queueOf} from "./collections";

export interface Channel<T> {
    send: SendChannel<T>;
    receive: ReceiveChannel<T>;
}

export interface SendChannel<T> {
    send(value: T): Promise<void>;
    close(): void;
}

export interface ReceiveChannel<T> extends AsyncIterable<T> {
    receive(): Promise<T>;
    close(): void;
}

export function channel<T>(): Channel<T> {
    let readyToReceive: Queue<(value: T) => void> = queueOf();
    let readyToSend: Queue<() => T> = queueOf();
    let closed = false;
    const channel = {
        send: {
            async send(value: T): Promise<void> {
                if (readyToReceive.size()) {
                    readyToReceive.dequeue()!(value);
                } else {
                    await new Promise<void>(resolve => {
                        readyToSend.enqueue(() => {
                            resolve();
                            return value;
                        });
                    });
                }
            },
            close(): void {
                closed = true;
            }
        },
        receive: {
            async receive(): Promise<T> {
                if (readyToSend.size()) {
                    return readyToSend.dequeue()!();
                } else {
                    return await new Promise<T>(resolve => {
                        readyToReceive.enqueue(resolve);
                    });
                }
            },
            [Symbol.asyncIterator](): AsyncIterator<T> {
                return async function*() {
                    while (!closed) {
                        yield await channel.receive.receive();
                    }
                }();
            },
            close(): void {
                closed = true;
            }
        }
    };
    return channel;
}
