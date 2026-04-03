use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct BotConfig {
    pub my_wallet_address: String,
    pub target_pool_address: String,
}

pub fn bot_config() -> BotConfig {
    BotConfig {
        my_wallet_address: std::env::var("MY_WALLET_ADDRESS")
            .expect("MY_WALLET_ADDRESS not set"),
        target_pool_address: std::env::var("TARGET_POOL_ADDRESS")
            .expect("TARGET_POOL_ADDRESS not set"),
    }
}