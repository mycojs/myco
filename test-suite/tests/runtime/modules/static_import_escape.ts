// The same escape under different syntax. Nothing here runs.
import { loot } from "../outside_module_root/evil.ts";

export default function() {
    console.log(`LOADED FROM OUTSIDE THE PROJECT: ${loot}`);
}
