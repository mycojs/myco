import {run} from "vendor/@myco/test";
import {listsTests} from "./collections/lists";
import {setsTests} from "./collections/sets";
import {streamsTest} from "./streams";
import {stacksTest} from './collections/stacks';
import {mapsTest} from "./collections/maps";

export default function () {
    run({
        "List": listsTests,
        "Set": setsTests,
        "Stream": streamsTest,
        "Stack": stacksTest,
        "Map": mapsTest
    })
}