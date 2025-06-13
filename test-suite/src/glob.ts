export function generateGlobDiff(expectedPattern: string, actualOutput: string): string {
    const expectedLines = expectedPattern.split('\n');
    const actualLines = actualOutput.split('\n');

    // Use LCS-based diff to find optimal alignment
    const diffResult = computeLCSDiff(expectedLines, actualLines);

    // Group changes and add context
    const contextLines = 2;
    const changeGroups = groupConsecutiveChanges(diffResult);

    if (changeGroups.length === 0) {
        return '';
    }

    const diffLines: string[] = [];

    // Add unified diff headers
    diffLines.push('--- expected');
    diffLines.push('+++ actual');

    for (const group of changeGroups) {
        const contextStart = Math.max(0, group.startIndex - contextLines);
        const contextEnd = Math.min(diffResult.length - 1, group.endIndex + contextLines);

        // Calculate line numbers for this hunk
        const { fromStart, fromCount, toStart, toCount } = calculateHunkLineNumbers(
            diffResult, contextStart, contextEnd
        );

        // Add hunk header
        const fromRange = fromCount === 1 ? `${fromStart}` : `${fromStart},${fromCount}`;
        const toRange = toCount === 1 ? `${toStart}` : `${toStart},${toCount}`;
        diffLines.push(`@@ -${fromRange} +${toRange} @@`);

        // Add hunk content
        for (let i = contextStart; i <= contextEnd; i++) {
            const change = diffResult[i];
            const isInChangeGroup = i >= group.startIndex && i <= group.endIndex;

            switch (change.type) {
                case 'equal':
                    if (isInChangeGroup) {
                        // This is an equal line within a change group - check if pattern matches
                        if (linesAreEquivalent(change.expected, change.actual)) {
                            diffLines.push(` ${change.actual}`);
                        } else {
                            // Pattern exists but doesn't match
                            diffLines.push(`-${change.expected}`);
                            diffLines.push(`+${change.actual}`);
                        }
                    } else {
                        // Context line
                        diffLines.push(` ${change.actual}`);
                    }
                    break;
                case 'delete':
                    diffLines.push(`-${change.expected}`);
                    break;
                case 'insert':
                    diffLines.push(`+${change.actual}`);
                    break;
            }
        }
    }

    return diffLines.join('\n');
}

function calculateHunkLineNumbers(diffResult: DiffChange[], contextStart: number, contextEnd: number): {
    fromStart: number;
    fromCount: number;
    toStart: number;
    toCount: number;
} {
    let fromLineNumber = 1;
    let toLineNumber = 1;
    let fromCount = 0;
    let toCount = 0;
    let fromStart = 1;
    let toStart = 1;

    // Calculate starting line numbers by counting lines up to contextStart
    for (let i = 0; i < contextStart; i++) {
        const change = diffResult[i];
        switch (change.type) {
            case 'equal':
                fromLineNumber++;
                toLineNumber++;
                break;
            case 'delete':
                fromLineNumber++;
                break;
            case 'insert':
                toLineNumber++;
                break;
        }
    }

    fromStart = fromLineNumber;
    toStart = toLineNumber;

    // Calculate counts for the hunk range
    for (let i = contextStart; i <= contextEnd; i++) {
        const change = diffResult[i];
        const isInChangeGroup = isChangeInGroup(diffResult, i, contextStart, contextEnd);

        switch (change.type) {
            case 'equal':
                if (isInChangeGroup) {
                    if (linesAreEquivalent(change.expected, change.actual)) {
                        fromCount++;
                        toCount++;
                    } else {
                        // Pattern doesn't match - treat as delete + insert
                        fromCount++;
                        toCount++;
                    }
                } else {
                    // Context line
                    fromCount++;
                    toCount++;
                }
                break;
            case 'delete':
                fromCount++;
                break;
            case 'insert':
                toCount++;
                break;
        }
    }

    return { fromStart, fromCount, toStart, toCount };
}

function isChangeInGroup(diffResult: DiffChange[], index: number, contextStart: number, contextEnd: number): boolean {
    // Find the actual change groups within this context range
    const contextLines = 2;
    const changeGroups = groupConsecutiveChanges(diffResult);

    for (const group of changeGroups) {
        const groupContextStart = Math.max(0, group.startIndex - contextLines);
        const groupContextEnd = Math.min(diffResult.length - 1, group.endIndex + contextLines);

        if (groupContextStart === contextStart && groupContextEnd === contextEnd) {
            return index >= group.startIndex && index <= group.endIndex;
        }
    }

    return false;
}

interface ChangeGroup {
    startIndex: number;
    endIndex: number;
    hasChanges: boolean;
}

function groupConsecutiveChanges(diffResult: DiffChange[]): ChangeGroup[] {
    const groups: ChangeGroup[] = [];
    let currentGroup: ChangeGroup | null = null;

    for (let i = 0; i < diffResult.length; i++) {
        const change = diffResult[i];
        const isChange = change.type !== 'equal' || (
            change.type === 'equal' && !linesAreEquivalent(change.expected, change.actual)
        );

        if (isChange) {
            if (!currentGroup) {
                currentGroup = { startIndex: i, endIndex: i, hasChanges: true };
            } else {
                currentGroup.endIndex = i;
            }
        } else {
            // Equal line that matches - close current group if exists
            if (currentGroup) {
                groups.push(currentGroup);
                currentGroup = null;
            }
        }
    }

    // Close final group
    if (currentGroup) {
        groups.push(currentGroup);
    }

    return groups;
}

interface DiffChange {
    type: 'equal' | 'delete' | 'insert';
    expected: string;
    actual: string;
}

function linesAreEquivalent(expected: string, actual: string): boolean {
    // First check exact match
    if (expected === actual) {
        return true;
    }
    
    // Then check if expected is a glob pattern that matches actual
    try {
        const regex = globToRegex(expected);
        return regex.test(actual);
    } catch {
        return false;
    }
}

function computeLCSDiff(expected: string[], actual: string[]): DiffChange[] {
    const m = expected.length;
    const n = actual.length;

    // Create LCS matrix
    const lcs = Array(m + 1).fill(0).map(() => Array(n + 1).fill(0));

    // Fill LCS matrix
    for (let i = 1; i <= m; i++) {
        for (let j = 1; j <= n; j++) {
            if (linesAreEquivalent(expected[i - 1], actual[j - 1])) {
                lcs[i][j] = lcs[i - 1][j - 1] + 1;
            } else {
                lcs[i][j] = Math.max(lcs[i - 1][j], lcs[i][j - 1]);
            }
        }
    }

    // Backtrack to build diff
    const changes: DiffChange[] = [];
    let i = m, j = n;

    while (i > 0 || j > 0) {
        if (i > 0 && j > 0 && linesAreEquivalent(expected[i - 1], actual[j - 1])) {
            // Lines are equivalent (identical or glob match)
            changes.unshift({
                type: 'equal',
                expected: expected[i - 1],
                actual: actual[j - 1]
            });
            i--;
            j--;
        } else if (i > 0 && (j === 0 || lcs[i - 1][j] >= lcs[i][j - 1])) {
            // Deletion from expected
            changes.unshift({
                type: 'delete',
                expected: expected[i - 1],
                actual: ''
            });
            i--;
        } else {
            // Insertion in actual
            changes.unshift({
                type: 'insert',
                expected: '',
                actual: actual[j - 1]
            });
            j--;
        }
    }

    return changes;
}

export function globToRegex(pattern: string): RegExp {
    let result = '';
    let i = 0;

    while (i < pattern.length) {
        const char = pattern[i];

        if (char === '\\' && i + 1 < pattern.length) {
            // Handle escaped characters
            const nextChar = pattern[i + 1];
            if (nextChar === '*' || nextChar === '?') {
                // Escape the literal character
                result += '\\' + nextChar;
                i += 2;
            } else {
                // Regular escape - escape the backslash and the character
                result += '\\\\' + nextChar.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                i += 2;
            }
        } else if (char === '*') {
            // Wildcard - match 0 or more characters
            result += '[^\\n]*';
            i++;
        } else if (char === '?') {
            // Single character wildcard
            result += '[^\\n]';
            i++;
        } else {
            // Regular character - escape special regex characters
            result += char.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
            i++;
        }
    }

    return new RegExp('^' + result + '$', 's'); // 's' flag for dotall mode
}

