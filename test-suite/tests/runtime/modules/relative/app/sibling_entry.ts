// A sibling import after one whose path was rewritten during resolution.
import { flat } from "../shared/flat.ts";
import { sib } from "./sib.ts";

export default function() {
    console.log(`sibling: ${flat()} ${sib()}`);
}
