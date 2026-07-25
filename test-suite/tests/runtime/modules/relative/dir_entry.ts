// Importing a directory appends index.ts, so the path no longer matches the
// specifier; the index's own import only resolves if that is accounted for.
import { pkg } from "./pkg";

export default function() {
    console.log(`dir: ${pkg()}`);
}
