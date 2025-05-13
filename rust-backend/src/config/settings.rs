use dotenv::dotenv;
use std::env;

#[derive(Debug, Clone)]
pub struct Settings {
    pub server_port: u16,
    pub blowfin_api_key: String,
    pub blowfin_api_secret: String,
    pub blowfin_api_passphrase: String,
    pub app_mode: String,
    pub default_strategy: String,
    pub database_url: String,
    pub redis_url: String,
}

impl Settings {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        dotenv().ok(); // loads `.env` file automatically

        let server_port = env::var("SERVER_PORT")
            .map_err(|_| "SERVER_PORT missing from env")?
            .parse::<u16>()
            .map_err(|_| "SERVER_PORT must be a valid u16")?;

        let blowfin_api_key = env::var("BLOFIN_API_KEY").map_err(|_| "BLOFIN_API_KEY missing")?;
        let blowfin_api_secret = env::var("BLOFIN_API_SECRET").map_err(|_| "BLOFIN_API_SECRET missing")?;
        let blowfin_api_passphrase = env::var("BLOFIN_API_PASSPHRASE").map_err(|_| "BLOFIN_API_PASSPHRASE missing")?;
        let app_mode = env::var("APP_MODE").map_err(|_| "APP_MODE missing")?.to_lowercase();
        let default_strategy = env::var("DEFAULT_STRATEGY").map_err(|_| "DEFAULT_STRATEGY missing")?;
        let database_url = env::var("DATABASE_URL").map_err(|_| "DATABASE_URL missing")?;
        let redis_url = env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".into());

        Ok(Self {
            server_port,
            blowfin_api_key,
            blowfin_api_secret,
            blowfin_api_passphrase,
            app_mode,
            default_strategy,
            database_url,
        })
    }

    pub fn is_demo(&self) -> bool {
        self.app_mode == "demo"
    }
}
