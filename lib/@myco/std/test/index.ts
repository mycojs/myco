import {run} from "vendor/@myco/test";
import {listsTests} from "./collections/lists";
import {setsTests} from "./collections/sets";
import {streamsTest} from "./streams";
import {mapsTest} from "./maps";

export default function () {
    run({
        "List": listsTests,
        "Set": setsTests,
        "Stream": streamsTest,
        "Map": mapsTest
    })
}