export default function(Myco: any) {
    console.log("Testing TOML.parse():");
    
    // Test basic key-value pairs
    const basicToml = `
name = "test-project"
version = "1.0.0"
debug = true
count = 42
pi = 3.14159
`;
    
    const basicParsed = TOML.parse(basicToml);
    console.log("Basic TOML parsed:");
    console.log("  name:", basicParsed.name);
    console.log("  version:", basicParsed.version); 
    console.log("  debug:", basicParsed.debug);
    console.log("  count:", basicParsed.count);
    console.log("  pi:", basicParsed.pi);
    
    // Test arrays
    const arrayToml = `
numbers = [1, 2, 3, 4, 5]
fruits = ["apple", "banana", "cherry"]
mixed = [1, "hello", true]
`;
    
    const arrayParsed = TOML.parse(arrayToml);
    console.log("Array TOML parsed:");
    console.log("  numbers:", arrayParsed.numbers);
    console.log("  fruits:", arrayParsed.fruits);
    console.log("  mixed:", arrayParsed.mixed);
    
    // Test nested objects (tables)
    const tableToml = `
[database]
server = "192.168.1.1"
ports = [8001, 8001, 8002]
connection_max = 5000
enabled = true

[servers.alpha]
ip = "10.0.0.1"
dc = "eqdc10"

[servers.beta]
ip = "10.0.0.2"
dc = "eqdc10"
`;
    
    const tableParsed = TOML.parse(tableToml);
    console.log("Table TOML parsed:");
    console.log("  database.server:", tableParsed.database.server);
    console.log("  database.ports:", tableParsed.database.ports);
    console.log("  database.enabled:", tableParsed.database.enabled);
    console.log("  servers.alpha.ip:", tableParsed.servers.alpha.ip);
    console.log("  servers.beta.dc:", tableParsed.servers.beta.dc);
    
    console.log("Testing TOML.stringify():");
    
    // Test basic object stringification
    const basicObj = {
        name: "my-project",
        version: "2.0.0",
        active: true,
        count: 100,
        score: 95.5
    };
    
    const basicStringified = TOML.stringify(basicObj);
    console.log("Basic object stringified:");
    console.log(basicStringified);
    
    // Test array stringification
    const arrayObj = {
        numbers: [1, 2, 3],
        names: ["Alice", "Bob", "Charlie"],
        flags: [true, false, true]
    };
    
    const arrayStringified = TOML.stringify(arrayObj);
    console.log("Array object stringified:");
    console.log(arrayStringified);
    
    // Test nested object stringification
    const nestedObj = {
        config: {
            host: "localhost",
            port: 3000,
            ssl: false
        },
        users: {
            admin: {
                name: "Administrator",
                level: 10
            },
            guest: {
                name: "Guest User", 
                level: 1
            }
        }
    };
    
    const nestedStringified = TOML.stringify(nestedObj);
    console.log("Nested object stringified:");
    console.log(nestedStringified);
    
    // Test round-trip (parse -> stringify -> parse)
    console.log("Testing round-trip conversion:");
    const originalObj = {
        title: "TOML Example",
        owner: {
            name: "Tom Preston-Werner",
            age: 35
        },
        database: {
            server: "192.168.1.1",
            ports: [8001, 8001, 8002],
            connection_max: 5000,
            enabled: true
        }
    };
    
    const tomlString = TOML.stringify(originalObj);
    const reparsedObj = TOML.parse(tomlString);
    
    console.log("Original title:", originalObj.title);
    console.log("Reparsed title:", reparsedObj.title);
    console.log("Round-trip title match:", originalObj.title === reparsedObj.title);
    
    console.log("Original owner.name:", originalObj.owner.name);
    console.log("Reparsed owner.name:", reparsedObj.owner.name);
    console.log("Round-trip owner.name match:", originalObj.owner.name === reparsedObj.owner.name);
    
    console.log("Original database.enabled:", originalObj.database.enabled);
    console.log("Reparsed database.enabled:", reparsedObj.database.enabled);
    console.log("Round-trip database.enabled match:", originalObj.database.enabled === reparsedObj.database.enabled);
    
    console.log("TOML parser tests completed");
} 