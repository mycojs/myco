import ts from "../../vendor/typescript/typescript.js";
import {host, sys} from "./host";

export async function compile(fileNames: string[], options: ts.CompilerOptions, myco: Myco): Promise<void> {
    const {console} = myco;
    (ts as any).setSys(sys(myco));
    let program = ts.createProgram(fileNames, options, await host(myco));
    let emitResult = program.emit();

    let allDiagnostics = ts
        .getPreEmitDiagnostics(program)
        .concat(emitResult.diagnostics);

    allDiagnostics.forEach(diagnostic => {
        if (diagnostic.file) {
            let {line, character} = ts.getLineAndCharacterOfPosition(diagnostic.file, diagnostic.start!);
            let message = ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n");
            console.error(`${diagnostic.file.fileName} (${line + 1},${character + 1}): ${message}`);
        } else {
            console.error(ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"));
        }
    });

    let exitCode = emitResult.emitSkipped ? 1 : 0;
    console.log(`Process exiting with code '${exitCode}'.`);
    // TODO: process.exit(exitCode);
}