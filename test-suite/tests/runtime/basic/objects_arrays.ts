export default function (_myco: Myco) {
    // Array operations
    const numbers = [1, 2, 3, 4, 5];
    const names = ["Alice", "Bob", "Charlie"];
    
    console.log("numbers[0]:", numbers[0]);
    console.log("names.length:", names.length);
    
    // Array methods
    numbers.push(6);
    const doubled = numbers.map(n => n * 2);
    const sum = numbers.reduce((acc, n) => acc + n, 0);
    
    console.log("numbers after push:", numbers.join(","));
    console.log("doubled:", doubled.join(","));
    console.log("sum:", sum);
    
    // Object operations
    const person: any = {
        name: "John",
        age: 30,
        city: "New York"
    };
    
    console.log("person.name:", person.name);
    console.log("person['age']:", person['age']);
    
    // Object modification
    person.age = 31;
    person.country = "USA";
    
    console.log("updated age:", person.age);
    console.log("new country:", person.country);
    
    // Object methods
    const keys = Object.keys(person);
    console.log("keys:", keys.join(","));
    
    // Nested structures
    const data = {
        users: [
            { id: 1, name: "Alice" },
            { id: 2, name: "Bob" }
        ],
        count: 2
    };
    
    console.log("first user:", data.users[0].name);
    console.log("user count:", data.count);
} 