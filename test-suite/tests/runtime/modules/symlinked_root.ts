// The project declared this root, so confinement must admit it.
import { lib } from "@fixture/symlinked";

export default function() {
    console.log(lib());
}
