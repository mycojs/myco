console.log("Hello", "myco!");

interface Foo {
    bar: string;
    fizz: number;
}

let content: string;
content = await myco.fetch(
    "https://deno.land/std@0.177.0/examples/welcome.ts",
);
console.log("Content from fetch", content);

export {}
