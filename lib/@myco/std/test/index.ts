import {run} from "vendor/@myco/test";
import {channelsTests} from "./channels";
import {listsTests} from "./collections/lists";
import {queuesTests} from './collections/queues';
import {setsTests} from "./collections/sets";
import {streamsTest} from "./streams";
import {stacksTest} from './collections/stacks';
import {mapsTest} from "./collections/maps";

export default async function () {
    await run({
        "Channels": channelsTests,
        "List": listsTests,
        "Queue": queuesTests,
        "Set": setsTests,
        "Stream": streamsTest,
        "Stack": stacksTest,
        "Map": mapsTest
    })
}