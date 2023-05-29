import {run, expect} from "../src";

export default async function (_myco: Myco) {
    run({
        "first level test": {
            "second level test": {
                "successful sync test": () => {
                    expect(10).toBe(10);
                },
                "failing sync test": () => {
                    expect(10).toBe(11);
                },
                "successful async test": async () => {
                    await new Promise<void>((resolve) => {
                        resolve();
                    });
                    expect(10).toBe(10);
                },
                "failing async test": async () => {
                    await new Promise<void>((resolve) => {
                        resolve();
                    });
                    expect(10).toBe(11);
                },
            },
            "second level primary test": () => {
                expect(10).toBe(10);
            },
        }
    })
}