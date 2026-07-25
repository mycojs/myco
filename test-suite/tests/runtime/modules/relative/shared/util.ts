// Its own import only resolves if this module reached the resolution stack.
import { helper } from "./helper.ts";
export function util() { return `util-ok ${helper()}`; }
