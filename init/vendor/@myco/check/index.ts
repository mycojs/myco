import ts from 'vendor/@myco/typescript/typescript.js';
import {compile} from "./wrapper";
import {parseConfigFileHost} from "./wrapper/host";

export {compile} from "./wrapper";

export default async function (myco: Myco) {
    const {console, files} = myco;
    const tsconfig = ts.getParsedCommandLineOfConfigFile("./tsconfig.json", undefined, await parseConfigFileHost(myco));
    if (!tsconfig) {
        console.error("Could not load tsconfig.json");
        return;
    }
    const {fileNames, options} = tsconfig;
    await compile(fileNames, options, myco);
}

