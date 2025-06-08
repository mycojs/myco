export default function (_myco: Myco) {
    console.log("Before throwing error");
    throw new Error("This is an unhandled exception");
} 