// Test multi-path resolution
// The first path in @fixture/multi doesn't exist, so it should fall back to the second path
import { index } from "@fixture/multi";
import { file } from "@fixture/multi/file.ts";

export default function (_myco: Myco) {
    console.log("Testing multi-path resolution...");
    console.log("First import should resolve to second path in the array");

    // Test that the imports work
    index();
    file();

    console.log("✓ Multi-path resolution test passed - both imports worked correctly");
} 