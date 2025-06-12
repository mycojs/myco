export interface TestManifest {
    name: string;
    description: string;
    tests: TestCase[];
}

export interface TestMeta {
    suite: string;
    name: string;
}

export interface TestCase {
    name: string;
    script: string;
    args?: string[];
    working_directory?: string;
    environment_variables?: Record<string, string>;
    timeout_ms?: number;
    expected_stdout?: string;
    expected_stderr?: string;
    expected_exit_code?: number;
}
