// Passes even with the resolution-stack bug: this shape alone proves nothing.
import { flat } from "../shared/flat.ts";

export default function() {
    console.log(`naive: ${flat()}`);
}
