// Outside the package root but named by myco-local.toml, so reachable.
export function allowed() {
    console.log("allowed: reached through a myco-local resolve entry");
}
