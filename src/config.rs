use std::{
    error::Error, 
    fs::File, 
    io::{BufReader, Read}
};

use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Database {
    pub host: String,
    pub database: String,
    pub port: u32,
    pub username: String,
    pub password: String,
}

impl Database {
    pub fn connection_string(&self) -> String {
        return format!("postgresql://{username}:{password}@{host}:{port}/{database}", 
            username=self.username,
            password=self.password,
            host=self.host,
            port=self.port,
            database=self.database
        );
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Server {
    pub host: String,
    pub port: i32
}

impl Default for Server {
    fn default() -> Self {
        Self { 
            host: "0.0.0.0".to_owned(), 
            port: 3001
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub database: Database,
    pub server: Server
}

impl Default for Config {
    fn default() -> Self {
        Self { 
            database: Default::default(),
            server: Default::default() 
        }
    }
}

impl Config {
    pub fn from_path(path: &str) -> Result<Self, Box<dyn Error>> {
        let file: File = File::open(path)?;

        // Wrap the file in a BufReader to efficiently read the file line by line
        let mut reader: BufReader<File> = BufReader::new(file);
    
        // Iterate over each line in the file
        let mut buffer: String = String::new();
        reader.read_to_string(&mut buffer)?;

        let config: Config = toml::from_str(&buffer)?;
        Ok(config)
    }
}

#[cfg(test)]
mod test {
    use super::Config;

    #[test]
    fn test_config() {
        let config: Config = toml::from_str(r#"
            [database]
            host = 'HOSTNAME'
            port = 1234
            database = 'DB_NAME'
            username = 'USERNAME'
            password = 'PASSWORD'

            [server]
            host = 'HOSTNAME'
            port = 1234
        "#).unwrap();

        println!("{:#?}", config);
    }

    #[test]
    fn test_config_from_file() {
        let config: Config = Config::from_path("../../configs/dev.toml").unwrap();
        println!("{:#?}", config);
    }

}