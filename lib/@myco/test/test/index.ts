import {run, expect} from "../src";

export default function (_myco: Myco) {
    run({
        "first level test": {
            "second level test": {
                "third level test": () => {
                    expect(10).toBe(10);
                },
                "third level test 2": () => {
                    expect("test").toBeInstanceOf(String);
                }
            },
            "second level primary test": () => {
                expect(10).toBe(10);
            },
        }
    })
}