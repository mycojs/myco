// A mid-path `.` collapses the same way `..` does.
import { mod } from "./shared/sub/./mod.ts";

export default function() {
    console.log(`dot: ${mod()}`);
}
