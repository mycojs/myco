import ts from '../vendor/typescript/typescript.js';
import {compile} from "./wrapper";

export default async function (myco: Myco) {
    const {console, files} = myco;
    const configToken = await files.requestRead("./tsconfig.json");
    const configFile = await configToken.read();
    const tsconfig = JSON.parse(configFile);
    const {options, errors} = ts.convertCompilerOptionsFromJson(tsconfig.compilerOptions, "./")!;
    if (errors.length) {
        console.error(errors);
        return;
    }
    await compile(["myco.d.ts", "src/index.ts"], options, myco);
}

