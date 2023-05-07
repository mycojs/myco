export default async function ({files, console}: Myco) {
    const path = "./log.txt";
    try {
        const rToken = await files.requestRead(path);
        const contents = await rToken.read();
        console.log("Read from a file", contents);
    } catch (err) {
        console.error("Unable to read file", path, err);
    }

    const rwToken = await files.requestReadWrite(path);
    await rwToken.write("I can write to a file.");
    const contents = await rwToken.read();
    console.log("Read from a file", path, "contents:", contents);
    console.log("Removing file", path);
    await rwToken.remove();
    console.log("File removed");
}
