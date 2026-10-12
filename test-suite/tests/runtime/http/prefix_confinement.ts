// Refusals are local; allowed paths attempt a loopback connection.
const REASONS = [
    ["outside the token's prefix", "outside the prefix"],
    ["may not contain an encoded separator", "encoded separator"],
    ["may not carry a query or a fragment", "query or fragment in the grant"],
    ["Invalid URL", "not a URL"],
] as const;

async function describe(label: string, action: () => Promise<unknown>) {
    try {
        await action();
        console.log(`${label}: allowed`);
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        const reason = REASONS.find(([needle]) => message.includes(needle));
        console.log(`${label}: refused, ${reason ? reason[1] : `unexpected: ${message}`}`);
    }
}

export default async function(myco: Myco) {
    console.log("Starting prefix confinement test");

    const prefix = await myco.http.requestFetchPrefix("https://raw.githubusercontent.com/mycojs/myco/");

    await describe("Encoded traversal", () => prefix.fetch("%2e%2e/%2e%2e/admin"));
    await describe("Literal traversal", () => prefix.fetch("../../admin"));
    await describe("Encoded slash", () => prefix.fetch("..%2f..%2fadmin"));
    await describe("Encoded backslash", () => prefix.fetch("..%5cadmin"));

    // A connection error means confinement admitted the path.
    const local = await myco.http.requestFetchPrefix("http://127.0.0.1:1/public/");
    for (const [label, path] of [
        ["plain path", "file.json"],
        ["nested path", "a/b/c.json"],
        ["traversal landing back inside", "a/../file.json"],
        ["query with an encoded URL", "cb?redirect=https%3A%2F%2Fx.test%2Fy"],
    ] as const) {
        const allowed = await local.fetch(path).then(
            () => true,
            (e: unknown) => e instanceof Error && e.message.includes("HTTP request failed"),
        );
        console.log(`Allowed, ${label}: ${allowed}`);
    }

    await describe("Prefix with a query", () =>
        myco.http.requestFetchPrefix("https://example.test/x/?k=v"));
    await describe("Prefix with a fragment", () =>
        myco.http.requestFetchPrefix("https://example.test/x/#frag"));
    await describe("Prefix that is not a URL", () =>
        myco.http.requestFetchPrefix("not a url"));

    console.log("Prefix confinement test completed");
}
