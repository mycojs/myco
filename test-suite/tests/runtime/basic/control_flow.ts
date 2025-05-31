export default function (_myco: Myco) {
    // If/else statements
    const age = 25;
    if (age >= 18) {
        console.log("Adult");
    } else {
        console.log("Minor");
    }
    
    const score = 85;
    if (score >= 90) {
        console.log("Grade: A");
    } else if (score >= 80) {
        console.log("Grade: B");
    } else if (score >= 70) {
        console.log("Grade: C");
    } else {
        console.log("Grade: F");
    }
    
    // For loop
    console.log("For loop:");
    for (let i = 1; i <= 3; i++) {
        console.log("  Iteration:", i);
    }
    
    // For...of loop
    console.log("For...of loop:");
    const fruits = ["apple", "banana", "orange"];
    for (const fruit of fruits) {
        console.log("  Fruit:", fruit);
    }
    
    // For...in loop
    console.log("For...in loop:");
    const obj: any = { a: 1, b: 2, c: 3 };
    for (const key in obj) {
        console.log("  " + key + ":", obj[key]);
    }
    
    // While loop
    console.log("While loop:");
    let count = 0;
    while (count < 3) {
        console.log("  Count:", count);
        count++;
    }
    
    // Switch statement
    const day: string = "Monday";
    switch (day) {
        case "Monday":
            console.log("Start of work week");
            break;
        case "Friday":
            console.log("TGIF!");
            break;
        case "Saturday":
        case "Sunday":
            console.log("Weekend!");
            break;
        default:
            console.log("Regular day");
    }
} 