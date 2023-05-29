import {TestSuite, expect} from "vendor/@myco/test";
import {channel} from "../src/channels";

export const channelsTests: TestSuite = {
    "Channel": {
        "should send and receive values": async () => {
            const ch = channel<number>();
            let sent = false;
            const promise = ch.send.send(42).then(() => {
                sent = true;
            });
            expect(sent).toBe(false);
            expect(await ch.receive.receive()).toBe(42);
            await promise;
            expect(sent).toBe(true);
        },
        "should send and receive multiple values": async () => {
            const ch = channel<number>();
            let sent = 0;
            const promise1 = ch.send.send(42).then(() => {
                sent++;
            });
            const promise2 = ch.send.send(43).then(() => {
                sent++;
            });
            expect(sent).toBe(0);
            expect(await ch.receive.receive()).toBe(42);
            expect(await ch.receive.receive()).toBe(43);
            await promise1;
            await promise2;
            expect(sent).toBe(2);
        },
        "receive channel should be iterable": async () => {
            const ch = channel<number>();
            let sent = 0;
            const promises = [
                ch.send.send(42).then(() => {
                    sent++;
                }),
                ch.send.send(43).then(() => {
                    sent++;
                    ch.send.close();
                }),
            ];
            expect(sent).toBe(0);
            let i = 0;
            for await (const x of ch.receive) {
                expect(x).toBe(42 + i);
                await promises[i];
                i++;
                expect(sent).toBe(i);
            }
        }
    }
}