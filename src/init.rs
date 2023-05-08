use std::path::PathBuf;
use std::fs;
use crate::myco_toml::MycoToml;

static TSCONFIG_JSON: &str = include_str!("../init/tsconfig.json");

static INDEX_TS: &str = include_str!("../init/src/index.ts");

static MYCO_D_TS: &str = include_str!("../init/myco.d.ts");

static MYCO_TOML: &str = include_str!("../init/myco.toml");

pub fn init(dir: String) {
    let dir = PathBuf::from(dir);
    if dir.exists() {
        eprintln!("error: Directory already exists");
        return;
    }
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/main.ts"), INDEX_TS).unwrap();
    fs::write(dir.join("tsconfig.json"), TSCONFIG_JSON).unwrap();
    fs::write(dir.join("myco.d.ts"), MYCO_D_TS).unwrap();
    let mut myco_toml = MycoToml::from_string(MYCO_TOML).unwrap();
    myco_toml.package.name = dir.file_name().unwrap().to_str().unwrap().to_string();
    fs::write(dir.join("myco.toml"), myco_toml.to_string()).unwrap();
    println!("Initialized Myco project in {}", dir.to_string_lossy());
}
