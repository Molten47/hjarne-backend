use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url:              String,
    pub redis_url:                 String,
    pub jwt_private_key:           String,
    pub jwt_public_key:            String,
    pub jwt_access_expiry_seconds: i64,
    pub jwt_refresh_expiry_days:   i64,
    pub app_host:                  String,
    pub app_port:                  u16,
    pub resend_api_key:            String,
    pub resend_from_email:         String,
    pub app_base_url:              String,
    pub daily_api_key:             String,
    pub daily_domain:              String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        // Try env var first (production/Render), fall back to file path (local dev)
        let jwt_private_key = if let Ok(key) = std::env::var("JWT_PRIVATE_KEY") {
            key
        } else {
            let path = std::env::var("JWT_PRIVATE_KEY_PATH")
                .expect("Either JWT_PRIVATE_KEY or JWT_PRIVATE_KEY_PATH must be set");
            std::fs::read_to_string(&path).expect("failed to read private key file")
        };

        let jwt_public_key = if let Ok(key) = std::env::var("JWT_PUBLIC_KEY") {
            key
        } else {
            let path = std::env::var("JWT_PUBLIC_KEY_PATH")
                .expect("Either JWT_PUBLIC_KEY or JWT_PUBLIC_KEY_PATH must be set");
            std::fs::read_to_string(&path).expect("failed to read public key file")
        };

        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set"),
            redis_url: std::env::var("REDIS_URL")
                .expect("REDIS_URL must be set"),
            jwt_private_key,
            jwt_public_key,
            jwt_access_expiry_seconds: std::env::var("JWT_ACCESS_EXPIRY_SECONDS")
                .unwrap_or_else(|_| "900".to_string())
                .parse()
                .expect("JWT_ACCESS_EXPIRY_SECONDS must be a number"),
            jwt_refresh_expiry_days: std::env::var("JWT_REFRESH_EXPIRY_DAYS")
                .unwrap_or_else(|_| "7".to_string())
                .parse()
                .expect("JWT_REFRESH_EXPIRY_DAYS must be a number"),
            app_host: std::env::var("APP_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            app_port: std::env::var("APP_PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .expect("APP_PORT must be a number"),
            resend_api_key: std::env::var("RESEND_API_KEY")
                .expect("RESEND_API_KEY must be set"),
            resend_from_email: std::env::var("RESEND_FROM_EMAIL")
                .expect("RESEND_FROM_EMAIL must be set"),
            app_base_url: std::env::var("APP_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:5173".to_string()),
            daily_api_key: std::env::var("DAILY_API_KEY")
                .expect("DAILY_API_KEY must be set"),
            daily_domain: std::env::var("DAILY_DOMAIN")
                .expect("DAILY_DOMAIN must be set"),
        })
    }

    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.app_host, self.app_port)
    }
}
