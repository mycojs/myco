// What a prefix token refuses, and what it must still allow. None of these
// reach the network: the request is refused before it is made, or the grant is
// refused before a token exists.

// Classified rather than printed: the messages name the URL, which would make
// the expectation depend on the prefix rather than on what was refused.
const REASONS = [
    ["outside the token's prefix", "outside the prefix"],
    ["may not contain an encoded slash", "encoded slash"],
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

    // Encoded dot segments are decoded by the URL parser after any substring
    // check would have run, so these have to be caught by the prefix boundary.
    await describe("Encoded traversal", () => prefix.fetch("%2e%2e/%2e%2e/admin"));
    await describe("Literal traversal", () => prefix.fetch("../../admin"));
    // An encoded slash keeps the client and the server from agreeing on where
    // the segments are.
    await describe("Encoded slash", () => prefix.fetch("..%2f..%2fadmin"));

    // A grant that cannot mean what it says is refused when it is made, rather
    // than silently ignoring every path passed to it.
    await describe("Prefix with a query", () =>
        myco.http.requestFetchPrefix("https://example.test/x/?k=v"));
    await describe("Prefix with a fragment", () =>
        myco.http.requestFetchPrefix("https://example.test/x/#frag"));
    await describe("Prefix that is not a URL", () =>
        myco.http.requestFetchPrefix("not a url"));

    console.log("Prefix confinement test completed");
}
