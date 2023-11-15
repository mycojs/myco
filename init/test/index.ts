import {run, expect} from "vendor/@myco/test";
import {message} from "../src";

export default function () {
    run({
        "Hello, world!": () => {
            expect(message()).toBe("Hello, world!");
        }
    })
}
