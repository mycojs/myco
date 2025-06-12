export function indent(text: string, indent: number): string {
    return text.split('\n').map(line => ' '.repeat(indent) + line).join('\n');
}
